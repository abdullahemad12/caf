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

    fn get_packages_dir_path(&self) -> PathBuf {
        self.root_dir.join(PackageManager::PKGS_PATH)
    }

    fn get_package_path(&self, package_name: &String) -> PathBuf {
        self.get_packages_dir_path().join(package_name)
    }

    pub fn new(root_dir: PathBuf) -> Result<Self, CafError> {
        if !root_dir.is_dir() {
            return Err(CafError::new("{} is not a valid directory"));
        }

        let pkgman = PackageManager { root_dir };

        fs::create_dir_all(pkgman.get_packages_dir_path())
            .wrap_err("unable to create the packages directory")?;

        Ok(pkgman)
    }

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

    pub fn all_package_ids_iter(&self) -> Result<impl Iterator<Item = PackageId>, CafError> {
        let packages_dir = self.get_packages_dir_path();

        Ok(fs::read_dir(self.get_packages_dir_path())
            .wrap_err(format!(
                "unable to read subdirectories of {}",
                packages_dir.to_string_lossy()
            ))?
            .flat_map(move |pkg_res| {
                let pkg = match pkg_res {
                    Ok(pkg) => pkg,
                    Err(err) => {
                        // TODO: maybe this needs to be reported better than this
                        eprintln!(
                            "unable to read subdirectory of {}: {}",
                            packages_dir.to_string_lossy(),
                            err
                        );
                        return vec![];
                    }
                };

                let versions = match fs::read_dir(pkg.path()) {
                    Ok(pkg_dirs) => pkg_dirs,
                    Err(err) => {
                        // TODO: maybe this needs to be reported better than this
                        eprintln!(
                            "unable to read subdirectory of {}: {}",
                            pkg.path().to_string_lossy(),
                            err
                        );

                        return vec![];
                    }
                };

                versions
                    .filter_map(|version_res| {
                        let version = match version_res {
                            Ok(it) => it,
                            Err(err) => {
                                // TODO: maybe this needs to be reported better than this
                                eprintln!(
                                    "unable to read subdirectory of {}: {}",
                                    pkg.path().to_string_lossy(),
                                    err
                                );
                                return None;
                            }
                        };

                        if !version.path().is_dir() {
                            None
                        } else {
                            Some(PackageId {
                                name: pkg
                                    .file_name()
                                    .to_str()
                                    .or_else(|| {
                                        eprintln!(
                                            "package name is not a valid unicode: {}",
                                            pkg.file_name().to_string_lossy()
                                        );
                                        None
                                    })?
                                    .to_string(),
                                version: version
                                    .file_name()
                                    .to_str()
                                    .or_else(|| {
                                        eprintln!(
                                            "version name is not a valid unicode: {}",
                                            pkg.file_name().to_string_lossy()
                                        );
                                        None
                                    })?
                                    .to_string(),
                            })
                        }
                    })
                    .collect()
            }))
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
