//! Persistent file logging for long-running subcommands (`serve`, `daemon`).
//!
//! Logs keep going to stdout and are additionally appended to a capped file
//! under the data dir — no new dependencies, just a tee `MakeWriter` over
//! `std::fs::File`.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Truncate the log at startup once it exceeds this size (bytes).
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

/// Default log file location: `<db_dir>/logs/alltokens.log`.
pub fn default_log_path(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("logs")
        .join("alltokens.log")
}

#[derive(Clone)]
struct TeeWriter {
    file: Arc<Mutex<fs::File>>,
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::stdout().write(buf)?;
        self.file.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stdout().flush()?;
        self.file.lock().unwrap().flush()
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for TeeWriter {
    type Writer = TeeWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Initialize the global tracing subscriber. When `log_file` is provided,
/// logs are additionally appended to that file (truncated first if it has
/// grown past 5 MB).
pub fn init(log_file: Option<PathBuf>) -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive("alltokens=info".parse()?);

    match log_file {
        Some(path) => {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
            let oversized = fs::metadata(&path)
                .map(|m| m.len() > MAX_LOG_BYTES)
                .unwrap_or(false);
            let file = if oversized {
                OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&path)?
            } else {
                OpenOptions::new().create(true).append(true).open(&path)?
            };
            let tee = TeeWriter {
                file: Arc::new(Mutex::new(file)),
            };
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(tee)
                .init();
            tracing::info!("Persistent log file: {}", path.display());
        }
        None => {
            tracing_subscriber::fmt().with_env_filter(filter).init();
        }
    }
    Ok(())
}

/// Initialize tracing to stderr only. Required for `mcp`, whose stdout is the
/// JSON-RPC protocol channel and must stay free of log output.
pub fn init_stderr() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive("alltokens=info".parse()?);
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_log_path_lives_under_db_dir() {
        let path = default_log_path(Path::new("/tmp/alltokens/data.db"));
        assert_eq!(path, PathBuf::from("/tmp/alltokens/logs/alltokens.log"));
    }

    #[test]
    fn tee_writer_writes_to_both_sinks() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let file = OpenOptions::new()
            .write(true)
            .open(tmp.path())
            .unwrap();
        let mut tee = TeeWriter {
            file: Arc::new(Mutex::new(file)),
        };
        let n = tee.write(b"hello log line\n").unwrap();
        assert_eq!(n, 15);
        tee.flush().unwrap();
        let written = fs::read_to_string(tmp.path()).unwrap();
        assert_eq!(written, "hello log line\n");
    }
}
