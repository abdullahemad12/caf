use std::path::PathBuf;

const CRATE_NAME: &str = env!("CARGO_PKG_NAME");

pub fn get_tmp_path(filename: &str) -> PathBuf {
    let mut path = std::env::temp_dir();

    path.push(CRATE_NAME);
    path.push(filename);

    path
}
