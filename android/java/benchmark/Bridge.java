package benchmark;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.Intent;
import android.graphics.Bitmap;
import android.os.Build;
import android.os.Handler;
import android.os.PowerManager;
import android.view.Gravity;
import android.view.PixelCopy;
import android.view.View;
import android.view.ViewGroup;
import android.view.SurfaceView;
import android.widget.Button;
import android.widget.FrameLayout;
import android.widget.Toast;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.zip.ZipEntry;
import java.util.zip.ZipOutputStream;
import org.json.JSONObject;

/** Android-only UI and SAF export. Native collectors own timing and validity. */
public final class Bridge {
    public static native void command(int value);
    public static native int status();
    public static native int repetition();
    private final Activity activity;
    private final Handler handler = new Handler();
    private final Button button;
    private boolean closed;
    private boolean capturing;
    private volatile boolean exporting;
    private int deviceRepetition;
    private float refresh;
    private static final int SAVE = 7319;
    private static final String[] PRESETS = {"standard", "gpu", "baseline", "no_shadows", "no_aa", "no_bloom", "scale_65", "no_effects", "Reference at 38.52 s", "View results", "Export ZIP"};
    public Bridge(Activity activity) {
        this.activity = activity;
        recover();
        button = new Button(activity);
        button.setText("Benchmark");
        FrameLayout.LayoutParams params = new FrameLayout.LayoutParams(-2, -2, Gravity.TOP | Gravity.RIGHT);
        activity.addContentView(button, params);
        button.setOnClickListener(v -> {
            if (exporting) return;
            if (active(status())) command(90);
            else menu();
        });
        handler.post(poll);
    }
    private static boolean active(int value) { return value == 7000 || value >= 1000 && value < 3000; }
    private final Runnable poll = new Runnable() {
        public void run() {
            if (closed) return;
            int state = status();
            if (exporting) { button.setText("Exporting ZIP…"); handler.postDelayed(this,1000); return; }
            if (active(state)) {
                int repeat = repetition();
                if (repeat != deviceRepetition && (state == 7000 || state < 2000)) {
                    snapshot(); deviceRepetition = repeat;
                }
                float hz = activity.getWindowManager().getDefaultDisplay().getRefreshRate();
                if (refresh > 0 && Math.abs(hz - refresh) > .1) command(98);
                button.setText((state == 7000 ? "Initializing" : state < 2000 ? "Warmup " + (state - 1000) + "/30s" : "Measure " + (state - 2000) + "/60s") + " · " + repeat + "/3 · cancel");
            } else {
                deviceRepetition = 0;
                button.setText(state == 3000 ? "Benchmark complete" : state == 4000 ? "Benchmark invalid" : state == 5000 ? "Benchmark cancelled" : "Benchmark");
            }
            if (state == 6000 && !capturing) capture();
            handler.postDelayed(this, 1000);
        }
    };
    private void menu() {
        new AlertDialog.Builder(activity).setTitle("Forest benchmark").setItems(PRESETS, (d, which) -> {
            if (which < 9) {
                if (!snapshot()) return;
                deviceRepetition = 0; command(which + 1);
            } else if (which == 9) results();
            else {
                Intent save = new Intent(Intent.ACTION_CREATE_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("application/zip").putExtra(Intent.EXTRA_TITLE, "forest-benchmark.zip");
                activity.startActivityForResult(save, SAVE);
            }
        }).show();
    }
    private boolean snapshot() {
        try {
            JSONObject device = new JSONObject();
            device.put("brand", Build.BRAND); device.put("model", Build.MODEL);
            device.put("android", Build.VERSION.RELEASE); device.put("fingerprint", Build.FINGERPRINT);
            device.put("api", Build.VERSION.SDK_INT); device.put("captured_at", System.currentTimeMillis());
            device.put("thermal_status", Build.VERSION.SDK_INT >= 29 ? ((PowerManager) activity.getSystemService(Activity.POWER_SERVICE)).getCurrentThermalStatus() : JSONObject.NULL);
            write(new File(activity.getFilesDir(), "benchmark-thermal.json"), device.get("thermal_status").toString());
            refresh = activity.getWindowManager().getDefaultDisplay().getRefreshRate();
            write(new File(activity.getFilesDir(), "benchmark-refresh.json"), refresh > 0 ? Float.toString(refresh) : "null");
            write(new File(activity.getFilesDir(), "benchmark-device.json"), device.toString());
            return true;
        } catch (Exception error) { message("Device metadata failed: " + error.getMessage()); command(99); return false; }
    }
    private File root() { return new File(activity.getFilesDir(), "benchmarks"); }
    private void recover() {
        File[] dirs = root().listFiles();
        if (dirs == null) return;
        for (File dir : dirs) {
            File marker = new File(dir,".pending");
            if (!marker.isFile()) continue;
            try {
                JSONObject m = new JSONObject(read(new File(dir,"metadata.json")));
                JSONObject summary = new JSONObject(read(new File(dir,"summary.json")));
                if (!m.getBoolean("valid") && !summary.getBoolean("valid") &&
                        m.getString("run_id").equals(summary.getString("run_id")) &&
                        m.optString("invalid_reason").startsWith("process interrupted") &&
                        summary.optString("invalid_reason").startsWith("process interrupted")) {
                    if (!marker.delete()) message("Incomplete benchmark marker could not be recovered");
                }
            } catch (Exception error) { /* Partial files remain marked and excluded. */ }
        }
    }
    private void results() {
        StringBuilder text = new StringBuilder();
        File[] dirs = root().listFiles();
        if (dirs != null) {
            Arrays.sort(dirs, (a,b) -> b.getName().compareTo(a.getName()));
            for (File dir : dirs) {
                File summary = new File(dir,"summary.json");
                if (new File(dir,".pending").exists()) { text.append("Interrupted or incomplete run: ").append(dir.getName()).append("\n\n"); continue; }
                if (!summary.isFile()) continue;
                try { text.append(new JSONObject(read(summary)).toString(2)).append("\n\n"); }
                catch (Exception error) { text.append("Unreadable result: ").append(dir.getName()).append('\n'); }
            }
        }
        new AlertDialog.Builder(activity).setTitle("Saved benchmark results").setMessage(text.length() == 0 ? "No results yet." : text.toString()).setPositiveButton("OK",null).show();
    }
    public boolean result(int request, int code, Intent data) {
        if (request != SAVE) return false;
        if (code != Activity.RESULT_OK || data == null || data.getData() == null) return true;
        exporting=true; button.setEnabled(false);
        new Thread(() -> {
            try (OutputStream stream = activity.getContentResolver().openOutputStream(data.getData()); ZipOutputStream zip = new ZipOutputStream(stream)) {
                archive(zip, root(), "");
            } catch (Exception error) { message("Export failed: " + error.getMessage()); return; }
            finally { activity.runOnUiThread(() -> { exporting=false; button.setEnabled(true); }); }
            message("Benchmark ZIP saved");
        }, "benchmark-export").start();
        return true;
    }
    private static void archive(ZipOutputStream zip, File dir, String prefix) throws IOException {
        if (new File(dir,".pending").exists()) return;
        File[] files = dir.listFiles();
        if (files == null) throw new IOException("Cannot list " + dir);
        Arrays.sort(files);
        byte[] buffer = new byte[16384];
        for (File file : files) {
            String name = prefix + file.getName();
            if (file.isDirectory()) archive(zip,file,name+"/");
            else {
                zip.putNextEntry(new ZipEntry(name));
                try (InputStream input = new FileInputStream(file)) { int n; while ((n=input.read(buffer))!=-1) zip.write(buffer,0,n); }
                zip.closeEntry();
            }
        }
    }
    private static SurfaceView findSurface(View view) {
        if (view instanceof SurfaceView) return (SurfaceView) view;
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int i=0; i<group.getChildCount(); i++) {
                SurfaceView result = findSurface(group.getChildAt(i));
                if (result != null) return result;
            }
        }
        return null;
    }
    private void capture() {
        capturing = true; button.setVisibility(View.INVISIBLE);
        handler.postDelayed(() -> {
            Bitmap image = null;
            try {
                if (closed || activity.isFinishing() || activity.isDestroyed()) throw new IOException("Activity closed");
                View decor = activity.getWindow().getDecorView();
                SurfaceView surface = findSurface(decor);
                View source = surface == null ? decor : surface;
                image = Bitmap.createBitmap(source.getWidth(), source.getHeight(), Bitmap.Config.ARGB_8888);
                final Bitmap pixels = image;
                PixelCopy.OnPixelCopyFinishedListener callback = code -> {
                    try {
                        if (code != PixelCopy.SUCCESS) throw new IOException("PixelCopy " + code);
                        File dir = new File(root(),"references");
                        if (!dir.isDirectory() && !dir.mkdirs()) throw new IOException("Cannot create reference directory");
                        String name = "reference-" + System.currentTimeMillis();
                        try (OutputStream out = new FileOutputStream(new File(dir,name+".png"))) {
                            if (!pixels.compress(Bitmap.CompressFormat.PNG,100,out)) throw new IOException("PNG encoding failed");
                        }
                        write(new File(dir,name+".json"),read(new File(root(),"reference-settings.json")));
                        message("Reference screenshot saved");
                    } catch (Exception error) { message("Reference failed: " + error.getMessage()); }
                    finally { pixels.recycle(); captureFinished(); }
                };
                // SDL renders into a separate SurfaceView, which a Window copy excludes.
                if (surface != null) PixelCopy.request(surface, pixels, callback, handler);
                else PixelCopy.request(activity.getWindow(), pixels, callback, handler);
            } catch (Exception error) {
                if (image != null) image.recycle();
                message("Reference failed: " + error.getMessage()); captureFinished();
            }
        },250);
    }
    private void captureFinished() {
        command(10); button.setVisibility(View.VISIBLE); capturing=false;
    }
    private static void write(File file, String text) throws IOException {
        try (OutputStream out = new FileOutputStream(file)) { out.write(text.getBytes(StandardCharsets.UTF_8)); }
    }
    private static String read(File file) throws IOException { return new String(java.nio.file.Files.readAllBytes(file.toPath()),StandardCharsets.UTF_8); }
    private void message(String text) { activity.runOnUiThread(() -> Toast.makeText(activity,text,Toast.LENGTH_LONG).show()); }
    public void pause() { command(99); }
    public void close() { closed=true;handler.removeCallbacks(poll); }
}
