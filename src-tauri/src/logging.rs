use std::{path::PathBuf, sync::OnceLock};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::Rotation;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize file logging. The returned guard must remain alive for the
/// lifetime of the process; non-blocking log lines are flushed by it.
#[allow(clippy::expect_used)]
#[allow(clippy::missing_panics_doc)]
pub fn init() -> &'static WorkerGuard {
    static GUARD: OnceLock<WorkerGuard> = OnceLock::new();

    GUARD.get_or_init(|| {
        let log_dir = log_dir();
        if let Err(error) = std::fs::create_dir_all(&log_dir) {
            eprintln!(
                "failed to create log directory {}: {error}",
                log_dir.display()
            );
        }

        let appender = tracing_appender::rolling::RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("corpusbot")
            .filename_suffix("log")
            .max_log_files(14)
            .build(log_dir)
            .expect("build rolling log appender");
        let (writer, guard) = tracing_appender::non_blocking(appender);
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

        tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(writer),
            )
            .init();

        guard
    })
}

#[must_use]
pub fn log_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".corpusbot")
        .join("logs")
}
