#!/usr/bin/env python3
"""Generate explicitly synthetic three-engine fixtures and an example report."""
from pathlib import Path
import argparse
from test_compare import fixture
from compare import ENGINES, load, report
import csv

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('output',type=Path)
args=parser.parse_args()
args.output.mkdir(parents=True,exist_ok=True)
for engine in ENGINES:
    for repetition in (1,2,3):
        folder=args.output/(engine.lower()+'-'+str(repetition))
        folder.mkdir(exist_ok=True)
        for name,text in fixture(engine,repetition).items(): (folder/name).write_text(text)
md,rows=report(load([args.output]))
(args.output/'example.md').write_text(md)
with (args.output/'example.csv').open('w',newline='') as f:
    writer=csv.DictWriter(f,fieldnames=list(rows[0]));writer.writeheader();writer.writerows(rows)
print('Synthetic examples saved to',args.output)
