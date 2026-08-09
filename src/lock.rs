use crate::errors::{CafError, WrapError};
use crate::utils;
use fs2::FileExt;
use std::fs::{self, File};

const LOCK_FILE_NAME: &str = "caf.lock";

// The lock is automatically released once this instance of this struct is dropped
pub struct CafLock {
    _file: File,
}

pub fn acquire_caf_lock() -> Result<CafLock, CafError> {
    let lock_path = utils::get_tmp_path(LOCK_FILE_NAME);
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent).wrap_err("failed to create lock directory in tmp dir")?;
    }

    let file = File::create(lock_path).wrap_err("unable to create lock file")?;

    // try_lock_exclusive returns an error if another process holds the lock
    file.try_lock_exclusive()
        .wrap_err("unable to exclusively lock the lock file")?;

    Ok(CafLock { _file: file })
}
