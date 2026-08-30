use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Cap on buffered lines; when exceeded the file is rotated down to
/// `ROTATE_KEEP` lines so the log never grows without bound.
const MAX_LINES: usize = 8000;
const ROTATE_KEEP: usize = 4000;

#[derive(Clone, Copy, Debug)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub at: String,
    pub level: String,
    pub message: String,
}

struct LoggerInner {
    file: File,
    count: usize,
    path: PathBuf,
}

/// A simple structured event log written under `<data_dir>/logs/app.log`.
///
/// Every write goes straight to the file (no user-space buffering) and is
/// flushed to the OS immediately, so a crash cannot silently discard the
/// very entries that would explain it.
#[derive(Default)]
pub struct Logger {
    inner: Mutex<Option<LoggerInner>>,
}

fn timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn count_lines(path: &Path) -> usize {
    match File::open(path) {
        Ok(file) => BufReader::new(file).lines().count(),
        Err(_) => 0,
    }
}

fn read_lines(path: &Path) -> Vec<String> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file).lines().filter_map(Result::ok).collect()
}

impl Logger {
    /// Open (or append to) the log file for this data directory. Safe to call
    /// once at startup; a Default logger no-ops until opened.
    pub fn open(&self, data_dir: PathBuf) {
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        if guard.is_some() {
            return;
        }
        let logs_dir = data_dir.join("logs");
        if std::fs::create_dir_all(&logs_dir).is_err() {
            return;
        }
        let path = logs_dir.join("app.log");
        let file = match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(file) => file,
            Err(err) => {
                eprintln!("could not open log file: {err}");
                return;
            }
        };
        let count = count_lines(&path);
        *guard = Some(LoggerInner { file, count, path });
    }

    /// Append a timestamped line and flush immediately.
    pub fn log(&self, level: Level, message: &str) {
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        let Some(inner) = guard.as_mut() else {
            return;
        };
        let line = format!("{} {} {}", timestamp(), level.as_str(), message);
        let _ = writeln!(inner.file, "{line}");
        let _ = inner.file.flush();
        inner.count += 1;

        if inner.count >= MAX_LINES {
            let lines = read_lines(&inner.path);
            let keep: Vec<&String> = lines
                .iter()
                .skip(lines.len().saturating_sub(ROTATE_KEEP))
                .collect();
            if let Ok(mut file) = OpenOptions::new().write(true).truncate(true).open(&inner.path) {
                for line in keep {
                    let _ = writeln!(file, "{line}");
                }
                let _ = file.flush();
                inner.file = file;
                inner.count = lines.len().min(ROTATE_KEEP);
            }
        }
    }

    /// The most recent entries, newest first.
    pub fn recent(&self, limit: usize) -> Vec<LogEntry> {
        let Ok(guard) = self.inner.lock() else {
            return Vec::new();
        };
        let Some(inner) = guard.as_ref() else {
            return Vec::new();
        };
        read_lines(&inner.path)
            .iter()
            .rev()
            .take(limit)
            .filter_map(|line| {
                let mut parts = line.splitn(3, ' ');
                Some(LogEntry {
                    at: parts.next()?.to_string(),
                    level: parts.next()?.to_string(),
                    message: parts.next().unwrap_or("").to_string(),
                })
            })
            .collect()
    }

    /// Copy the current log file contents to another location (for sharing).
    pub fn export_to(&self, dest: &Path) -> std::io::Result<()> {
        let guard = self.inner.lock().map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::Other, "log file unavailable")
        })?;
        let Some(inner) = guard.as_ref() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "log file not open",
            ));
        };
        std::fs::copy(&inner.path, dest)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mp-log-{label}-{}-{:x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn recent_is_newest_first() {
        let dir = temp_dir("recent");
        let logger = Logger::default();
        logger.open(dir.clone());
        for i in 0..5 {
            logger.log(Level::Info, &format!("line {i}"));
        }
        let entries = logger.recent(10);
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].message, "line 4");
        assert_eq!(entries[4].message, "line 0");
        assert_eq!(entries[0].level, "INFO");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn entries_flush_immediately() {
        let dir = temp_dir("flush");
        let logger = Logger::default();
        logger.open(dir.clone());
        logger.log(Level::Warn, "witness line");
        drop(logger);
        let raw = std::fs::read_to_string(dir.join("logs/app.log")).unwrap();
        assert!(raw.contains("WARN witness line"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rotates_after_cap() {
        let dir = temp_dir("rotate");
        let logger = Logger::default();
        logger.open(dir.clone());
        let total = MAX_LINES + 1000;
        for i in 0..total {
            logger.log(Level::Info, &format!("line {i}"));
        }
        let entries = logger.recent(50_000);
        assert!(entries.len() > ROTATE_KEEP);
        assert!(entries.len() <= MAX_LINES);
        assert_eq!(entries[0].message, format!("line {}", total - 1));
        assert_eq!(entries.last().unwrap().message, "line 4000");
        std::fs::remove_dir_all(&dir).ok();
    }
}