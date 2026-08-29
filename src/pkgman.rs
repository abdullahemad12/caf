use crate::errors::{CafError, WrapErrorInResult};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub struct PackageManager {
    root_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PackageMetadata {
    name: String,
    active_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageId {
    pub name: String,
    pub version: String,
}

impl std::fmt::Display for PackageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}

// Contents of the packages are compressed into a single file (e.g. zip, tar.gz, etc..)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CompressedPackageContent(pub Vec<u8>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Package {
    pub id: PackageId,
    pub content: CompressedPackageContent,
}

impl std::fmt::Display for Package {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.id.fmt(f)
    }
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

    pub fn new(root_dir: PathBuf) -> Self {
        return PackageManager { root_dir };
    }

    // The design needs to be reworked https://chatgpt.com/c/6a80b9d5-3ce0-83eb-b1a4-04ef0cd92d3f
    pub fn install_package(&self, package: Package) -> Result<(), CafError> {
        let pkg_path = self.get_package_path(&package.id.name);
        let install_path = pkg_path.join(package.id.version.clone());

        let metadata_json = serde_json::to_string_pretty(&PackageMetadata {
            name: package.id.name.clone(),
            active_version: package.id.version.clone(),
        })
        .wrap_err(format!(
            "unable to serialize the package metadata into JSON for package {}",
            package.id
        ))?;

        fs::create_dir_all(&install_path).wrap_err(format!(
            "unable to create the package directory for package {}",
            package.id
        ))?;

        let installed_package_path = install_path.join(PackageManager::CONTENT_FILE_NAME);
        fs::write(installed_package_path.clone(), package.content.0).wrap_err(format!(
            "unabel to write the file content of the package {} to the file {}",
            package.id,
            installed_package_path.to_string_lossy(),
        ))?;

        fs::write(
            pkg_path.join(PackageManager::METADATA_FILE_NAME),
            metadata_json,
        )
        .wrap_err(format!(
            "unable to write the package metadata for package {}",
            package.id
        ))?;

        Ok(())
    }

    pub fn retrieve_package(&self, request: &PackageId) -> Result<Package, CafError> {
        let pkg_content_path = self
            .get_package_path(&request.name)
            .join(&request.version)
            .join(PackageManager::CONTENT_FILE_NAME);

        let content =
            fs::read(pkg_content_path).wrap_err("unable to retrieve the package from the db")?;

        return Ok(Package {
            id: request.clone(),
            content: CompressedPackageContent(content),
        });
    }

    pub fn retrieve_active_package_version(
        &self,
        package_name: &String,
    ) -> Result<PackageId, CafError> {
        let metadata_file = fs::File::open(
            self.get_package_path(&package_name)
                .join(PackageManager::METADATA_FILE_NAME),
        )
        .wrap_err(format!(
            "unable to open the metadata file for the package version retrieval of package {}",
            package_name,
        ))?;

        let reader = std::io::BufReader::new(metadata_file);

        let metadata: PackageMetadata = serde_json::from_reader(reader).wrap_err(format!(
            "unable to serialize the package metadata into JSON for package {}",
            package_name,
        ))?;

        return Ok(PackageId {
            name: metadata.name,
            version: metadata.active_version,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let manager = PackageManager::new(temp_dir.path().to_path_buf());
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
        let manager = PackageManager::new(temp_dir.path().to_path_buf());
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
        let manager = PackageManager::new(temp_dir.path().to_path_buf());
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
