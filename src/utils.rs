use std::{error::Error, path::PathBuf};

const PROJECT_NAME: &str = env!("CARGO_PKG_NAME");

pub fn log_error(err: impl Error) -> impl Error {
    eprintln!("{:?}", err);
    err
}

pub fn get_tmp_path(filename: &str) -> PathBuf {
    let mut path = std::env::temp_dir();

    path.push(PROJECT_NAME);
    path.push(filename);

    path
}
