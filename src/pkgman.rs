use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use crate::errors::WrapError;
use crate::{errors, package, utils};

pub struct PackageManager {
    root_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PackageMetadata {
    name: String,
    active_version: String,
}

// Design:
// all packages are stored as
//  /root_dir/pkgs/{package_name}/{package_version}/content.zip
// each package has a metadata file stored at
//  /root_dir/pkgs/{package_name}/metadata.json
impl PackageManager {
    const PKGS_PATH: &'static str = "pkgs";
    const CONTENT_FILE_NAME: &'static str = "pkg.zip";
    const METADATA_FILE_NAME: &'static str = "metadata.json";

    fn get_package_path(&self, package_name: &String) -> PathBuf {
        self.root_dir
            .join(PackageManager::PKGS_PATH)
            .join(package_name)
    }

    pub fn new(root_dir: PathBuf) -> Result<Self, errors::CafError> {
        return Ok(PackageManager { root_dir });
    }

    pub fn install_package(&self, package: package::Package) -> Result<(), std::io::Error> {
        let pkg_path = self.get_package_path(&package.id.name);
        let install_path = pkg_path.join(package.id.version.clone());

        let metadata_json = serde_json::to_string_pretty(&PackageMetadata {
            name: package.id.name.clone(),
            active_version: package.id.version.clone(),
        })?;

        fs::create_dir_all(&install_path)?;
        fs::write(
            install_path.join(PackageManager::CONTENT_FILE_NAME),
            package.content.0,
        )?;

        fs::write(
            pkg_path.join(PackageManager::METADATA_FILE_NAME),
            metadata_json,
        )?;

        Ok(())
    }

    pub fn retrieve_package(
        &self,
        request: &package::PackageId,
    ) -> Result<package::Package, std::io::Error> {
        let pkg_content_path = self
            .get_package_path(&request.name)
            .join(&request.version)
            .join(PackageManager::CONTENT_FILE_NAME);

        let content = fs::read(pkg_content_path)?;

        return Ok(package::Package {
            id: request.clone(),
            content: package::CompressedPackageContent(content),
        });
    }

    pub fn retrieve_active_package_version(
        &self,
        package_name: &String,
    ) -> Result<package::PackageId, std::io::Error> {
        let metadata_file = fs::File::open(
            PackageManager::get_package_path(&self.root_dir, &package_name)
                .join(PackageManager::METADATA_FILE_NAME),
        )?;

        let reader = std::io::BufReader::new(metadata_file);

        let metadata: PackageMetadata = serde_json::from_reader(reader)?;

        return Ok(package::PackageId {
            name: metadata.name,
            version: metadata.active_version,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::{CompressedPackageContent, Package, PackageId};
    use tempfile::tempdir;

    fn fixture_package() -> Package {
        Package {
            id: PackageId {
                name: "widget".into(),
                version: "1.0.0".into(),
            },
            content: CompressedPackageContent(vec![1, 2, 3, 4]),
        }
    }

    #[test]
    fn install_package_writes_content_and_metadata() {
        // given:
        let temp_dir = tempdir().unwrap();
        let manager = PackageManager::new(temp_dir.path().to_string_lossy().into_owned());
        let package = fixture_package();

        // when:
        manager.install_package(package.clone()).unwrap();

        // then:
        let install_path = temp_dir
            .path()
            .join("pkgs")
            .join(&package.id.name)
            .join(&package.id.version);

        let content_path = install_path.join(PackageManager::CONTENT_FILE_NAME);

        let metadata_path = install_path
            .parent()
            .unwrap()
            .join(PackageManager::METADATA_FILE_NAME);

        assert!(content_path.exists());
        assert_eq!(fs::read(content_path).unwrap(), package.content.0);

        let metadata_bytes = fs::read(metadata_path).unwrap();
        let metadata: PackageMetadata = serde_json::from_slice(&metadata_bytes).unwrap();

        let expected_metadata = PackageMetadata {
            name: package.id.name.clone(),
            active_version: package.id.version.clone(),
        };
        assert_eq!(metadata, expected_metadata);
    }

    #[test]
    fn retrieve_package_returns_stored_package() {
        // given:
        let temp_dir = tempdir().unwrap();
        let manager = PackageManager::new(temp_dir.path().to_string_lossy().into_owned());
        let package = fixture_package();

        manager.install_package(package.clone()).unwrap();

        // when:
        let retrieved = manager.retrieve_package(&package.id).unwrap();

        // then:
        assert_eq!(retrieved, package);
    }

    #[test]
    fn retrieve_active_package_version_reads_metadata() {
        // given:
        let temp_dir = tempdir().unwrap();
        let manager = PackageManager::new(temp_dir.path().to_string_lossy().into_owned());
        let package = fixture_package();
        manager.install_package(package.clone()).unwrap();

        // when:
        let active = manager
            .retrieve_active_package_version(&package.id.name)
            .unwrap();

        // then:
        assert_eq!(active, package.id);
    }
}
