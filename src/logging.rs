use std::io;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
pub fn init(level: LevelFilter) -> WorkerGuard {
    let filter = EnvFilter::builder()
        .with_default_directive(level.into())
        .with_env_var("SHA_LOG")
        .from_env_lossy();
    let (non_blocking, guard) = tracing_appender::non_blocking(io::stderr());
    tracing_subscriber::fmt()
        .with_line_number(false)
        .with_thread_ids(false)
        .with_ansi(true)
        .with_target(false)
        .with_level(true)
        .with_ansi_sanitization(false)
        .with_file(false)
        .with_thread_names(false)
        .with_env_filter(filter)
        .with_writer(non_blocking)
        .init();
    guard
}
