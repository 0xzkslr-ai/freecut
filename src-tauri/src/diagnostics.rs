//! Startup diagnostics must work before Tauri or WebView2 can create a window.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Condvar, Mutex, OnceLock,
    },
    time::Duration,
};
use windows_sys::{
    Wdk::System::SystemServices::RtlGetVersion,
    Win32::{
        Foundation::SYSTEMTIME,
        System::SystemInformation::{GetLocalTime, OSVERSIONINFOW},
    },
};

const MAX_LOG_BYTES: usize = 256 * 1024;
const MAX_LOG_FILES: usize = 10;
const MAX_MESSAGE_CHARS: usize = 4000;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
static LOG: OnceLock<Mutex<LogFile>> = OnceLock::new();
static LOG_INIT_ERROR: OnceLock<String> = OnceLock::new();
static FAILURE_SHOWN: AtomicBool = AtomicBool::new(false);
static STARTUP: Startup = Startup::new();

struct LogFile {
    path: PathBuf,
    file: File,
    bytes: usize,
    full: bool,
    error: Option<String>,
}

impl LogFile {
    fn create(path: PathBuf) -> io::Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(Self {
            path,
            file,
            bytes: 0,
            full: false,
            error: None,
        })
    }

    fn write(&mut self, level: &str, message: &str) {
        if self.full {
            return;
        }
        let mut line = format!("{} [{level}] {}\n", timestamp(), clean_message(message));
        const LIMIT_NOTICE: &str = "[WARN] Log size limit reached; further messages omitted.\n";
        if self.bytes + line.len() + LIMIT_NOTICE.len() > MAX_LOG_BYTES {
            line = LIMIT_NOTICE.into();
            self.full = true;
        }
        if let Err(error) = self
            .file
            .write_all(line.as_bytes())
            .and_then(|_| self.file.flush())
        {
            self.error = Some(error.to_string());
        } else {
            self.bytes += line.len();
        }
    }
}

struct Startup {
    settled: Mutex<bool>,
    changed: Condvar,
}

impl Startup {
    const fn new() -> Self {
        Self {
            settled: Mutex::new(false),
            changed: Condvar::new(),
        }
    }

    fn finish(&self) {
        *self.settled.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.changed.notify_all();
    }

    fn wait(&self, timeout: Duration) -> bool {
        let settled = self.settled.lock().unwrap_or_else(|e| e.into_inner());
        let (settled, _) = self
            .changed
            .wait_timeout_while(settled, timeout, |settled| !*settled)
            .unwrap_or_else(|e| e.into_inner());
        *settled
    }
}

fn timestamp() -> String {
    // SAFETY: GetLocalTime writes a complete SYSTEMTIME into this valid buffer.
    let time = unsafe {
        let mut time: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut time);
        time
    };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        time.wYear,
        time.wMonth,
        time.wDay,
        time.wHour,
        time.wMinute,
        time.wSecond,
        time.wMilliseconds
    )
}

fn clean_message(message: &str) -> String {
    // Bound frontend/panic payloads and prevent forged log lines.
    message
        .chars()
        .take(MAX_MESSAGE_CHARS)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn prune_logs(directory: &Path, current: &Path) -> io::Result<()> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("startup-") && name.ends_with(".log") && entry.file_type()?.is_file() {
            paths.push(entry.path());
        }
    }
    paths.sort();
    let excess = paths.len().saturating_sub(MAX_LOG_FILES);
    for path in paths
        .into_iter()
        .filter(|path| path != current)
        .take(excess)
    {
        // Best effort: another process may prevent removal of an old file.
        let _ = fs::remove_file(path);
    }
    Ok(())
}

pub(super) fn init() {
    let name = format!(
        "startup-{}-{}.log",
        timestamp().replace([':', ' '], "-"),
        std::process::id()
    );
    let mut directories = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA").filter(|path| !path.is_empty()) {
        directories.push(PathBuf::from(local).join("FreeCut").join("logs"));
    }
    directories.push(std::env::temp_dir().join("FreeCut").join("logs"));
    let mut errors = Vec::new();
    for directory in directories {
        match fs::create_dir_all(&directory).and_then(|_| LogFile::create(directory.join(&name))) {
            Ok(file) => {
                let path = file.path.clone();
                let _ = LOG.set(Mutex::new(file));
                if let Err(error) = prune_logs(&directory, &path) {
                    write("WARN", &format!("Cannot prune old logs: {error}"));
                }
                break;
            }
            Err(error) => errors.push(format!("{}: {error}", directory.display())),
        }
    }
    if LOG.get().is_none() {
        let _ = LOG_INIT_ERROR.set(errors.join("; "));
    } else {
        for error in errors {
            write(
                "WARN",
                &format!("Log directory unavailable; using fallback: {error}"),
            );
        }
    }
    std::panic::set_hook(Box::new(|info| {
        failure("FreeCut 发生异常", &info.to_string());
    }));
    write(
        "INFO",
        &format!(
            "Starting FreeCut {}; arch={}; pid={}; background={}",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::ARCH,
            std::process::id(),
            std::env::args().any(|arg| arg == "--background")
        ),
    );
    if let Ok(path) = std::env::current_exe() {
        write("INFO", &format!("Executable: {}", path.display()));
    }
    // SAFETY: the initialized size and live buffer satisfy RtlGetVersion's contract.
    unsafe {
        let mut version: OSVERSIONINFOW = std::mem::zeroed();
        version.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
        if RtlGetVersion(&mut version) >= 0 {
            write(
                "INFO",
                &format!(
                    "Windows {}.{}.{}",
                    version.dwMajorVersion, version.dwMinorVersion, version.dwBuildNumber
                ),
            );
        } else {
            write("WARN", "Cannot determine Windows version");
        }
    }
}

pub(super) fn write(level: &str, message: &str) {
    if let Some(log) = LOG.get() {
        log.lock()
            .unwrap_or_else(|e| e.into_inner())
            .write(level, message);
    }
}

pub(super) fn log_location() -> String {
    match LOG.get() {
        Some(log) => {
            let log = log.lock().unwrap_or_else(|e| e.into_inner());
            match &log.error {
                Some(error) => format!("{}（写入失败：{error}）", log.path.display()),
                None => log.path.display().to_string(),
            }
        }
        None => format!(
            "日志无法创建：{}",
            LOG_INIT_ERROR
                .get()
                .map(String::as_str)
                .unwrap_or("日志目录不可写")
        ),
    }
}

fn show_failure(title: &str, detail: &str) {
    write("ERROR", &format!("{title}: {detail}"));
    if FAILURE_SHOWN.swap(true, Ordering::SeqCst) {
        return;
    }
    // A native dialog remains usable when WebView2 or the web frontend cannot start.
    rfd::MessageDialog::new()
        .set_title(title)
        .set_level(rfd::MessageLevel::Error)
        .set_description(format!(
            "{}\n\n日志：{}\n\n反馈问题时请附上该日志文件。",
            detail.chars().take(MAX_MESSAGE_CHARS).collect::<String>(),
            log_location()
        ))
        .show();
}

pub(super) fn failure(title: &str, detail: &str) {
    STARTUP.finish();
    show_failure(title, detail);
}

pub(super) fn ready() {
    write("INFO", "Main frontend initialized; startup complete");
    STARTUP.finish();
}

pub(super) fn stop() {
    STARTUP.finish();
}

pub(super) fn watch_startup() {
    if let Err(error) = std::thread::Builder::new()
        .name("freecut-startup-watch".into())
        .spawn(|| {
            if !STARTUP.wait(STARTUP_TIMEOUT) {
                show_failure(
                    "FreeCut 启动超时",
                    "启动超过 30 秒，主界面仍未就绪。程序可能仍在初始化；若一直没有出现界面，请退出后重新启动，并提供日志排查。",
                );
            }
        })
    {
        write("WARN", &format!("Cannot start startup watchdog: {error}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_are_flushed_bounded_and_do_not_overwrite_existing_sessions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("startup-test.log");
        let mut log = LogFile::create(path.clone()).unwrap();
        log.write("INFO", "first\nforged\0line");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.ends_with("[INFO] first forged line\n"));
        assert_eq!(text.lines().count(), 1);
        assert!(LogFile::create(path.clone()).is_err());
        for _ in 0..100 {
            log.write("ERROR", &"中".repeat(MAX_MESSAGE_CHARS + 100));
        }
        let size = fs::metadata(&path).unwrap().len();
        assert!(size <= MAX_LOG_BYTES as u64);
        assert!(fs::read_to_string(&path)
            .unwrap()
            .contains("Log size limit reached"));
        log.write("INFO", "after limit");
        assert_eq!(fs::metadata(path).unwrap().len(), size);
    }

    #[test]
    fn cleanup_retains_recent_sessions_and_unrelated_files() {
        let directory = tempfile::tempdir().unwrap();
        for i in 0..15 {
            fs::write(directory.path().join(format!("startup-{i:03}.log")), b"old").unwrap();
        }
        let current = directory.path().join("startup-014.log");
        let unrelated = directory.path().join("settings.json");
        fs::write(&unrelated, b"{}").unwrap();
        fs::create_dir(directory.path().join("startup-directory.log")).unwrap();
        prune_logs(directory.path(), &current).unwrap();
        assert!(!directory.path().join("startup-004.log").exists());
        assert!(directory.path().join("startup-005.log").exists());
        assert!(current.exists());
        assert!(unrelated.exists());
        assert!(directory.path().join("startup-directory.log").is_dir());
    }

    #[test]
    fn startup_timeout_does_not_prevent_late_readiness() {
        let startup = Startup::new();
        assert!(!startup.wait(Duration::ZERO));
        startup.finish();
        assert!(startup.wait(Duration::ZERO));
        startup.finish();
        assert!(startup.wait(Duration::ZERO));
    }

    #[test]
    fn oversized_unicode_errors_are_truncated_without_splitting_characters() {
        let cleaned = clean_message(&"中".repeat(MAX_MESSAGE_CHARS + 1));
        assert_eq!(cleaned.chars().count(), MAX_MESSAGE_CHARS);
        assert_eq!(clean_message("\r\n\t\0test"), "    test");
    }
}
