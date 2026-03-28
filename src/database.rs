use serde::{Deserialize, Serialize};

use std::{
    cmp, fs,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use crate::{package, utils::log_error};

pub struct Database {
    root_dir: String,
}

#[derive(Deserialize, Serialize)]
pub struct Metadata {
    name: String,
    active_version: String,
}

// TODO: move to pkgman
// Design:
// all packages are stored as
//  /root_dir/pkgs/{package_name}/{package_version}/content.zip
// each package has a metadata file stored at
//  /root_dir/pkgs/{package_name}/metadata.json
impl Database {
    const PKGS_PATH: &'static str = "pkgs";

    pub fn new(root_dir: String) -> Result<Database, std::io::Error> {
        Ok(Database { root_dir })
    }

    pub fn install_package(
        &self,
        package_name: &String,
        package_version: &String,
        zip_content: Vec<u8>,
    ) -> Result<(), std::io::Error> {
        let pkg_path = Database::get_package_path(&self.root_dir, package_name);
        let install_path = pkg_path.join(package_version);

        fs::create_dir_all(&install_path)?;
        fs::write(
            install_path.join(format!("{}.zip", package_name)),
            zip_content,
        )?;

        Ok(())
    }

    pub fn get_pkgs_metadata(&self, package_name: String) -> Option<Metadata> {
        let path = Database::get_metadata_path(package_name);

        self.metadata_file_names
            .binary_search_by(|file_name| {
                let file_path = path.join(file_name);

                let file = File::open(file_path).expect("to be able to open the file");

                let reader = BufReader::new(file);

                let metadata_content = serde_json::from_reader::<_, Vec<Metadata>>(reader)
                    .expect("to be able to deserialize the file");

                metadata_content
                    .binary_search_by(|meta| meta.name.cmp(&package_name))
                    .map(|_| cmp::Ordering::Equal)
                    .unwrap_or_else(|_| {
                        metadata_content
                            .first()
                            .map(|first_meta| package_name.cmp(&first_meta.name))
                            .unwrap_or(cmp::Ordering::Less)
                    })
            })
            .ok()?;
    }

    pub fn get_all_pkgs_metadata(&self) -> Vec<Metadata> {}

    pub fn add(&self, package_id: &package::PackageId) -> Result<(), std::io::Error> {
        let path = self.get_metadata_path(package_id.name.clone());
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let meta = Metadata {
            name: package_id.name.clone(),
            version: package_id.version.clone(),
        };

        let data = serde_json::to_string(&meta)?;

        fs::write(path, data)
    }

    fn get_package_path(root_dir: &String, package_name: &String) -> PathBuf {
        Path::new(&root_dir)
            .join(Database::PKGS_PATH)
            .join(package_name)
    }

    fn read_json_metadata_file(path: &PathBuf) -> Option<Vec<Metadata>> {
        let reader = BufReader::new(File::open(path).map_err(log_error).ok()?);
        serde_json::from_reader(reader).map_err(log_error).ok()
    }
}
