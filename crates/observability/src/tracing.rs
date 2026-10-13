use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime},
};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};

use crate::error::{CafError, WrapErrorInResult};

const BASE_LOG_FILE_NAME: &str = "caf.log";

const MAX_KEEP_FILES: Duration = Duration::from_hours(24 * 14); // keep for 14 days
fn cleanup_old_logs(logs_directory: &PathBuf) -> Result<(), CafError> {
    let retension_threshold = SystemTime::now() - MAX_KEEP_FILES;

    let dir = fs::read_dir(logs_directory).wrap_err("failed to open logs directory")?;

    dir.for_each(|entry| {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                eprintln!(
                    "failed to open the logs directory for the cleanup: {:#?}",
                    err
                );
                return;
            }
        };

        if !entry.path().is_file() {
            return;
        };

        let modified = match entry.metadata() {
            Ok(metadata) => metadata.modified(),
            Err(err) => {
                eprintln!(
                    "unable to get the metadata for one of the logs files: {:#?}",
                    err
                );
                return;
            }
        };

        match modified {
            Ok(modified) => {
                if modified < retension_threshold {
                    let _ = fs::remove_file(entry.path()).inspect_err(|err| {
                        eprintln!("unable to cleanup an old log file: {:#?}", err);
                    });
                }
            }
            Err(err) => {
                eprintln!(
                    "unable to read the modified date for one of the logs files: {:#?}",
                    err
                );
            }
        }
    });

    Ok(())
}

// the worker gaurd must not be dropped for the duration of this program. So it needs to be
// captured in main
pub async fn setup_tracing(logs_directory: PathBuf) -> Result<WorkerGuard, CafError> {
    let file_appender = tracing_appender::rolling::daily(logs_directory, BASE_LOG_FILE_NAME);

    let (non_blocking_writer, _guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking_writer)
        .with_ansi(false);

    // if needed, setting with_target to true will include the
    // origin of the log message in the output
    let stdout_layer = tracing_subscriber::fmt::layer().with_target(false);

    Registry::default()
        .with(file_layer)
        .with(stdout_layer)
        .init();

    Ok(_guard)
}
