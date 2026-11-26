use chrono::Utc;
use std::backtrace::Backtrace;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::storage::logs_dir;

static LOG_LOCK: Mutex<()> = Mutex::new(());

fn write_line(prefix: &str, level: &str, msg: &str) {
    let _guard = LOG_LOCK.lock();
    let dir = logs_dir();
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let now = Utc::now();
    let date = now.format("%Y-%m-%d");
    let ts = now.to_rfc3339();
    let file_path = dir.join(format!("{prefix}-{date}.log"));
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(file_path) {
        let _ = writeln!(f, "[{ts}] [{level}] {msg}");
        let _ = f.flush();
    }
}

pub fn log_info(msg: &str) {
    write_line("app", "INFO", msg);
}

pub fn log_warn(msg: &str) {
    write_line("app", "WARN", msg);
}

pub fn log_error(msg: &str) {
    write_line("app", "ERROR", msg);
}

pub fn log_timing(op: &str, elapsed_ms: u128, extra: &str) {
    if elapsed_ms >= 500 {
        log_warn(&format!("SLOW_OP {op} took {elapsed_ms}ms ({extra})"));
    } else {
        log_info(&format!("OP {op} took {elapsed_ms}ms ({extra})"));
    }
}

/// Installs a global panic hook that records panics + backtraces immediately to
/// `%APPDATA%\ScanCleaner\logs\crash-YYYY-MM-DD.log` and `app-YYYY-MM-DD.log`.
pub fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string();
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "non-string panic payload".to_string()
        };
        let bt = Backtrace::force_capture();
        let summary = format!("PANIC on thread '{thread}' at {location}: {payload}\n{bt}");
        write_line("crash", "CRASH", &summary);
        write_line(
            "app",
            "CRASH",
            &format!("PANIC on thread '{thread}' at {location}: {payload}"),
        );
        prev(info);
    }));
}

pub fn read_diagnostic_logs(limit: usize) -> Vec<String> {
    let dir = logs_dir();
    let Ok(rd) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("log"))
        .collect();
    files.sort();
    files.reverse();

    let mut lines = Vec::new();
    for file_path in files {
        let Ok(file) = fs::File::open(file_path) else {
            continue;
        };
        let reader = BufReader::new(file);
        let mut file_lines: Vec<String> = reader.lines().map_while(Result::ok).collect();
        file_lines.reverse();
        for l in file_lines {
            lines.push(l);
            if lines.len() >= limit {
                return lines;
            }
        }
    }
    lines
}
