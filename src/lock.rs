use crate::errors::{CafError, WrapErrorInResult};
use crate::utils;
use fs2::FileExt;
use std::fs::{self, File};

const LOCK_FILE_NAME: &str = "caf.lock";

// The lock is automatically released once this instance of this struct is dropped
#[derive(Debug)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{mpsc, Mutex, OnceLock};
    use std::thread;
    use std::time::Duration;

    static TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

    fn get_test_mutex() -> &'static Mutex<()> {
        TEST_MUTEX.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_acquire_caf_lock_sequential() {
        // given:
        let _guard = get_test_mutex().lock().unwrap();

        // when:
        let lock1 = acquire_caf_lock();

        // then:
        assert!(lock1.is_ok());
        let lock1 = lock1.unwrap();

        // and when: we drop the first lock
        drop(lock1);

        // then: we can acquire it again
        let lock2 = acquire_caf_lock();
        assert!(lock2.is_ok());
    }

    #[test]
    fn test_acquire_caf_lock_exclusive() {
        // given:
        let _guard = get_test_mutex().lock().unwrap();

        // when: we acquire the lock
        let lock1 = acquire_caf_lock();
        assert!(lock1.is_ok());
        let _lock1 = lock1.unwrap();

        // then: trying to acquire it again must fail
        let lock2 = acquire_caf_lock();
        assert!(lock2.is_err());
        let err = lock2.unwrap_err();
        assert!(err.to_string().contains("unable to exclusively lock the lock file"));

        // and when: we drop the first lock
        drop(_lock1);

        // then: we can successfully acquire the lock now
        let lock3 = acquire_caf_lock();
        assert!(lock3.is_ok());
    }

    #[test]
    fn test_acquire_caf_lock_across_threads() {
        // given:
        let _guard = get_test_mutex().lock().unwrap();

        let (tx_to_thread, rx_in_thread) = mpsc::channel();
        let (tx_to_main, rx_in_main) = mpsc::channel();

        // when: we spawn a thread that acquires the lock
        let handle = thread::spawn(move || {
            let lock = acquire_caf_lock().expect("Thread failed to acquire lock initially");
            // Signal main thread that the lock is held
            tx_to_main.send("locked").unwrap();

            // Wait for signal from main thread to drop the lock
            rx_in_thread.recv().unwrap();
            drop(lock);

            // Signal main thread that lock has been dropped
            tx_to_main.send("unlocked").unwrap();
        });

        // then: wait for thread to hold lock
        let status = rx_in_main.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(status, "locked");

        // and then: trying to acquire the lock in main thread should fail
        let lock_attempt = acquire_caf_lock();
        assert!(lock_attempt.is_err());

        // when: we signal thread to release lock
        tx_to_thread.send("release").unwrap();

        // then: wait for thread to signal release
        let status2 = rx_in_main.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(status2, "unlocked");

        // and then: we can successfully acquire the lock in main thread
        let lock_success = acquire_caf_lock();
        assert!(lock_success.is_ok());

        handle.join().unwrap();
    }
}
