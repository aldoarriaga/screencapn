use std::backtrace::Backtrace;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::panic;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Once, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();
static INSTALL_HOOK: Once = Once::new();
static USER_ENABLED: AtomicBool = AtomicBool::new(false);
const MAX_LOG_BYTES: u64 = 1024 * 1024;

pub fn install() {
    INSTALL_HOOK.call_once(|| {
        panic::set_hook(Box::new(|info| {
            let thread = std::thread::current();
            let thread_name = thread.name().unwrap_or("unnamed");
            let location = info
                .location()
                .map(|location| {
                    format!(
                        "{}:{}:{}",
                        location.file(),
                        location.line(),
                        location.column()
                    )
                })
                .unwrap_or_else(|| "unknown".to_string());
            let payload = info
                .payload()
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
                .unwrap_or("non-string panic payload");
            log_event(
                "panic",
                &format!(
                    "thread={thread_name} location={location} message={payload}\nbacktrace:\n{}",
                    Backtrace::force_capture()
                ),
            );
        }));
    });
    log_event("app", "diagnostics-installed");
}

pub fn set_enabled(enabled: bool) {
    USER_ENABLED.store(enabled, Ordering::Relaxed);
    if enabled {
        let path = log_path();
        let _ = fs::create_dir_all(path.parent().unwrap_or(path.as_path()));
        log_event("settings", "local-diagnostics-enabled");
    }
}

pub fn log_breadcrumb(message: impl AsRef<str>) {
    log_event("breadcrumb", message.as_ref());
}

pub fn log_event(scope: &str, message: &str) {
    if !enabled() {
        return;
    }
    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    rotate_if_needed(path);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} [{scope}] {message}", timestamp_millis());
    }
}

pub fn log_path() -> &'static PathBuf {
    LOG_PATH.get_or_init(|| {
        let mut path = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        path.push("ScreenCaptn");
        path.push("logs");
        path.push("screencaptn.log");
        path
    })
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

pub fn enabled() -> bool {
    cfg!(debug_assertions)
        || USER_ENABLED.load(Ordering::Relaxed)
        || std::env::var_os("SCREENCAPTN_DIAGNOSTICS").is_some()
}

fn rotate_if_needed(path: &PathBuf) {
    if fs::metadata(path).map(|meta| meta.len()).unwrap_or(0) < MAX_LOG_BYTES {
        return;
    }
    let previous = path.with_extension("previous.log");
    let _ = fs::remove_file(&previous);
    let _ = fs::rename(path, previous);
}
