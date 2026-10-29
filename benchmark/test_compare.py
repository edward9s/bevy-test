import copy
import csv
import io
import json
from pathlib import Path
import tempfile
import unittest
import zipfile
import compare as b


def fixture(engine='Filament', repetition=1):
    m=dict(schema_version=b.VERSION,run_id=engine+str(repetition),preset_id='standard',repetition=repetition,valid=True,invalid_reason=None,started_at=1800000000000,warmup_seconds=30,measure_seconds=60,
           engine=dict(name=engine,version='fixture',commit='a'*40,build='release'),device=dict(brand='SYNTHETIC',model='SYNTHETIC',android='fixture',fingerprint='fixture'),
           environment=dict(backend='Vulkan',surface_size=[1280,720],render_size=[1280,720],dynamic_resolution=False,vsync_requested='off',presentation_mode='FIFO',refresh_hz=120,gpu_profiling=False,refresh_limited_possible=True,gpu=None,driver=None,thermal_status=None),
           scene=dict(id='fixture',seed='fixture',camera='fixture',animation='fixture',geometry_sha256='a'*64,sources_sha256='b'*64),settings=dict(msaa_samples=2,taa=False,fxaa=False,shadows=True,bloom=True,effects=True,pipeline='fixture'),metrics={},limitations=['Synthetic protocol fixture; no physical device measurement.'],test_data=True)
    s={k:m[k] for k in ('schema_version','run_id','preset_id','valid','invalid_reason')}
    s.update(application=b.stats([100.]*600),presentation_fps=None)
    f=io.StringIO();w=csv.writer(f);w.writerow(b.FRAMES)
    for i in range(600): w.writerow([i,(i+1)*100,100,30+(i+1)*.1,'','','',''])
    return {'metadata.json':json.dumps(m),'summary.json':json.dumps(s),'frames.csv':f.getvalue(),'metrics.csv':','.join(b.METRICS)+'\n'}


class StatisticsTests(unittest.TestCase):
    def test_tail_and_nearest_rank(self):
        s=b.stats([10.]*99+[100.])
        self.assertEqual(s['p99_ms'],10)
        self.assertEqual(s['low_1_fps'],10)
        self.assertAlmostEqual(s['average_fps'],100000/1090)
        self.assertAlmostEqual(b.stats([10.]*100+[50.,100.])['low_1_fps'],1000/75)
    def test_reject_intervals(self):
        for data in ([],[0],[-1],[True],[float('nan')],[float('inf')]):
            with self.assertRaises(ValueError): b.stats(data)
    def test_round_trip(self):
        r=b.validate(fixture());self.assertEqual(r['summary']['application']['average_fps'],10)
    def test_reject_tampering(self):
        for filename,field,value in [('summary.json','application',dict(b.stats([100.]*600),p95_ms=1)),('metadata.json','warmup_seconds',0),('metadata.json','schema_version','9')]:
            f=fixture();j=json.loads(f[filename]);j[field]=value;f[filename]=json.dumps(j)
            with self.assertRaises(ValueError):b.validate(f)
    def test_group_conditions(self):
        r=b.validate(fixture());m=r['metadata'];key=b.condition(m)
        for field,value in [('backend','OpenGL ES'),('vsync_requested','on'),('gpu_profiling',True),('render_size',[832,468]),('presentation_mode',None)]:
            n=copy.deepcopy(m);n['environment'][field]=value
            self.assertNotEqual(key,b.condition(n))
        n=copy.deepcopy(m);n['environment']['presentation_mode']=None
        o=copy.deepcopy(n);o['engine']['name']='Godot'
        self.assertNotEqual(b.condition(n),b.condition(o))
    def test_gpu_pass_is_not_frame(self):
        f=fixture();m=json.loads(f['metadata.json']);m['metrics']={'shadow':dict(unit='ms',source='fixture',scope='shadow pass',scope_id='fixture.shadow',kind='gpu_pass')};f['metadata.json']=json.dumps(m)
        f['metrics.csv']+='native-1,1,shadow,80\n'
        md,_=b.report([b.validate(f)])
        self.assertIn('| GPU Frame Time | unavailable |',md)
        self.assertIn('`shadow`',md)
        f['metrics.csv']+='native-1,1,shadow,80\n'
        with self.assertRaises(ValueError):b.validate(f)
    def test_zip_and_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);folder=root/'run';folder.mkdir();f=fixture()
            for name,text in f.items():(folder/name).write_text(text)
            self.assertEqual(len(b.load([folder])),1)
            archive=root/'run.zip'
            with zipfile.ZipFile(archive,'w') as z:
                for name,text in f.items():z.writestr('exports/run/'+name,text)
            self.assertEqual(len(b.load([archive])),1)
            with self.assertRaises(ValueError):b.load([folder,archive])
    def test_invalid_and_mixed_build(self):
        r=b.validate(fixture());bad=copy.deepcopy(r);bad['metadata'].update(valid=False,invalid_reason='backgrounded',run_id='bad')
        other=copy.deepcopy(r);other['metadata']['engine']['commit']='b'*40
        md,_=b.report([r,other,bad]);self.assertIn('aggregation refused',md);self.assertIn('backgrounded',md)
    def test_ab_allows_only_one_workload_variable(self):
        base=b.validate(fixture());base['metadata']['preset_id']='baseline'
        other=copy.deepcopy(base);other['metadata'].update(preset_id='no_shadows',run_id='no-shadows');other['metadata']['settings']['shadows']=False
        gpu=copy.deepcopy(base);gpu['metadata'].update(preset_id='gpu',run_id='gpu');gpu['metadata']['environment']['gpu_profiling']=True
        text='\n'.join(b.ab_report([base,other,gpu]))
        self.assertIn('| no_shadows | 1 | 10.000 | +0.00% |',text)
        self.assertIn('profiling overhead only',text)
        other['metadata']['settings']['bloom']=False
        self.assertIn('| no_shadows | 1 | incompatible settings |', '\n'.join(b.ab_report([base,other])))
    def test_documented_example(self):
        root=Path(__file__).resolve().parent/'fixtures'
        names=('metadata.json','summary.json','frames.csv','metrics.csv')
        run=b.validate({name:(root/('example-'+name)).read_text() for name in names})
        self.assertTrue(run['metadata']['test_data'])
        self.assertEqual(run['summary']['application']['average_fps'],10)

    def test_unavailable_cpu_rows(self):
        md,_=b.report([b.validate(fixture())])
        self.assertIn('| CPU Render Time | unavailable | unavailable | unavailable |',md)

    def test_json_duplicate_rejected(self):
        with self.assertRaises(ValueError): b.json_object('{"a":1,"a":2}')


if __name__=='__main__':unittest.main()
