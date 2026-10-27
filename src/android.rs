//! Preserve the first Rust panic in Android's error log, including worker threads.
use std::ffi::{CString, c_char, c_int};

#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

pub fn install_panic_log() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = format!("Rust panic: {info}").replace('\0', "\\0");
        if let Ok(message) = CString::new(message) {
            // Both strings remain valid and NUL-terminated for the synchronous call.
            unsafe {
                __android_log_write(6, c"BevyForest".as_ptr(), message.as_ptr());
            }
        }
        previous(info);
    }));
}
