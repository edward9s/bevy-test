package benchmark;
import android.app.NativeActivity;
import android.content.Intent;
import android.os.Bundle;
public final class BenchmarkActivity extends NativeActivity {
    private Bridge benchmark;
    @Override protected void onCreate(Bundle state) { System.loadLibrary("bevy_forest"); super.onCreate(state); benchmark=new Bridge(this); }
    @Override protected void onPause() { if (benchmark != null) benchmark.pause(); super.onPause(); }
    @Override protected void onDestroy() { if (benchmark != null) benchmark.close(); super.onDestroy(); }
    @Override protected void onActivityResult(int request,int code,Intent data) {
        if (benchmark == null || !benchmark.result(request,code,data)) super.onActivityResult(request,code,data);
    }
}
