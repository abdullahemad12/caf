use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime},
};

use tracing::{Level, level_filters::LevelFilter};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::{
    Layer, Registry, filter::filter_fn, layer::SubscriberExt, util::SubscriberInitExt,
};

use crate::error::{CafError, WrapErrorInResult};

const BASE_LOG_FILE_NAME: &str = "caf.log";
const BASE_DEBUG_LOG_FILE_NAME: &str = "caf.debug.log";
const LOG_DIR_NAME: &str = "logs";

const MAX_KEEP_FILES: Duration = Duration::from_hours(24 * 14); // keep for 14 days
fn cleanup_old_logs(logs_directory: PathBuf) -> Result<(), CafError> {
    let retension_threshold = SystemTime::now() - MAX_KEEP_FILES;

    let dir = fs::read_dir(logs_directory).wrap_err("failed to open logs directory")?;

    dir.for_each(|entry| {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                eprintln!("failed to read log directory entry during cleanup: {err:#?}");
                return;
            }
        };

        if !entry.path().is_file() {
            return;
        };

        let modified = match entry.metadata() {
            Ok(metadata) => metadata.modified(),
            Err(err) => {
                eprintln!("failed to get metadata for log file: {err:#?}");
                return;
            }
        };

        match modified {
            Ok(modified) => {
                if modified < retension_threshold {
                    let _ = fs::remove_file(entry.path()).inspect_err(|err| {
                        eprintln!("failed to clean up old log file: {err:#?}");
                    });
                }
            }
            Err(err) => {
                eprintln!("failed to read modified date for log file: {err:#?}");
            }
        }
    });

    Ok(())
}

fn create_file_appender(logs_dir: PathBuf, log_file_name: String) -> (NonBlocking, WorkerGuard) {
    let file_appender = tracing_appender::rolling::daily(logs_dir, log_file_name);
    let (non_blocking_writer, _guard) = tracing_appender::non_blocking(file_appender);

    (non_blocking_writer, _guard)
}

// the worker gaurd must not be dropped for the duration of this program. So it needs to be
// captured in main
pub fn setup_tracing(data_directory: impl Into<PathBuf>) -> [WorkerGuard; 2] {
    let logs_directory = data_directory.into().join(LOG_DIR_NAME);

    let _ = fs::create_dir_all(logs_directory.clone())
        .wrap_err("unable to create the logs directory")
        .inspect_err(|err| eprintln!("{err:?}"));

    let _ = cleanup_old_logs(logs_directory.clone()).inspect_err(|err| eprintln!("{err:?}"));

    let (file_writer, _file_guard) =
        create_file_appender(logs_directory.clone(), BASE_LOG_FILE_NAME.to_string());

    let (debug_file_writer, _debug_file_guard) =
        create_file_appender(logs_directory.clone(), BASE_DEBUG_LOG_FILE_NAME.to_string());

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_writer)
        .pretty()
        .with_ansi(false)
        .with_filter(LevelFilter::ERROR);

    let debug_file_layer = tracing_subscriber::fmt::layer()
        .with_writer(debug_file_writer)
        .pretty()
        .with_ansi(false)
        .with_filter(LevelFilter::DEBUG);

    // if needed, setting with_target to true will include the
    // origin of the log message in the output
    let stdout_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_level(false)
        .without_time()
        .with_target(false)
        .with_filter(filter_fn(|meta| {
            [Level::WARN, Level::INFO].contains(meta.level())
        }));

    let stderr_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_level(false)
        .with_ansi(true)
        .without_time()
        .with_target(false)
        .with_writer(std::io::stderr)
        .with_filter(LevelFilter::ERROR);

    Registry::default()
        .with(file_layer)
        .with(debug_file_layer)
        .with(stdout_layer)
        .with(stderr_layer)
        .init();

    [_file_guard, _debug_file_guard]
}
