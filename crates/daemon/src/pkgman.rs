use libp2p::kad::RecordKey;
use observability::error::{CafError, CafErrorCode, WrapErrorInResult};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationMetadata {
    name: String,
    active_version: Version,
    verified_downloaded_versions: Vec<Version>,
}

impl InstallationMetadata {
    fn new(package_id: &PackageId) -> Self {
        InstallationMetadata {
            name: package_id.name.clone(),
            active_version: package_id.version.clone(),
            verified_downloaded_versions: vec![package_id.version.clone()],
        }
    }

    fn with_new_version(&self, new_version: &Version) -> Self {
        InstallationMetadata {
            name: self.name.clone(),
            active_version: new_version.clone(),
            verified_downloaded_versions: {
                let mut versions = self.verified_downloaded_versions.clone();
                if !versions.contains(new_version) {
                    versions.push(new_version.clone());
                }
                versions
            },
        }
    }
}

// TODO(AZ): add information about the OS and the CPU architecture
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageId {
    pub name: String,
    pub version: Version,
}

impl PackageId {
    pub fn new(name: impl Into<String>, raw_version: impl Into<String>) -> Result<Self, CafError> {
        let version = Version::parse(raw_version.into().as_str())
            .wrap_err_with_code(CafErrorCode::InvalidVersionFormat, "invalid version format")?;

        let name: String = name.into();

        Ok(PackageId { name, version })
    }
}

impl From<PackageId> for Vec<u8> {
    fn from(value: PackageId) -> Self {
        value.to_string().into_bytes()
    }
}

impl From<PackageId> for RecordKey {
    fn from(value: PackageId) -> Self {
        let b: Vec<u8> = value.into();
        RecordKey::new(&b)
    }
}

impl TryFrom<String> for PackageId {
    type Error = CafError;
    fn try_from(raw_user_pkg_id: String) -> Result<Self, Self::Error> {
        let (pkg_name, pkg_version) = raw_user_pkg_id.split_once("@").ok_or(CafError::new(
            CafErrorCode::InvalidUserPackageIdFormat,
            "invalid package id",
        ))?;

        PackageId::new(pkg_name, pkg_version)
    }
}

impl std::fmt::Display for PackageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}

// Contents of the packages are compressed into a single file tar.xz file
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TarballCompressedPackageContent(pub Vec<u8>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Package {
    pub id: PackageId,
    pub content: TarballCompressedPackageContent,
}

impl Package {
    pub fn new(id: PackageId, content: TarballCompressedPackageContent) -> Self {
        Package { id, content }
    }
}

impl std::fmt::Display for Package {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.id.fmt(f)
    }
}

pub struct ReadOnlyPackageManager {
    root_dir: PathBuf,
}

impl ReadOnlyPackageManager {
    const PKGS_PATH: &'static str = "pkgs";
    const CONTENT_FILE_NAME: &'static str = "pkg.zip";
    const METADATA_FILE_NAME: &'static str = "metadata.json";

    fn get_packages_dir_path(&self) -> PathBuf {
        self.root_dir.join(ReadOnlyPackageManager::PKGS_PATH)
    }

    fn get_package_path(&self, package_name: &String) -> PathBuf {
        self.get_packages_dir_path().join(package_name)
    }

    pub fn new(root_dir: PathBuf) -> Result<Self, CafError> {
        if !root_dir.is_dir() {
            return Err(CafError::new(
                CafErrorCode::InvalidConfig,
                "{} is not a valid directory",
            ));
        }

        let pkgman = ReadOnlyPackageManager { root_dir };

        fs::create_dir_all(pkgman.get_packages_dir_path())
            .wrap_err("unable to create the packages directory")?;

        Ok(pkgman)
    }

    pub fn pkg_exists_locally(&self, package_id: &PackageId) -> Result<bool, CafError> {
        let metadata = match self.read_installation_metadata(&package_id.name) {
            Ok(metadata) => metadata,
            Err(err) => {
                if *err.code() == CafErrorCode::PackageNotFound {
                    return Ok(false);
                } else {
                    return Err(err);
                }
            }
        };

        if metadata
            .verified_downloaded_versions
            .iter()
            .all(|it| *it != package_id.version)
        {
            return Ok(false);
        }

        self.get_package_path(&package_id.name)
            .join(package_id.version.to_string())
            .join(ReadOnlyPackageManager::CONTENT_FILE_NAME)
            .try_exists()
            .wrap_err("checking for package existance failed")
    }

    pub fn retrieve_package(&self, request: &PackageId) -> Result<Package, CafError> {
        let pkg_content_path = self
            .get_package_path(&request.name)
            .join(request.version.to_string())
            .join(ReadOnlyPackageManager::CONTENT_FILE_NAME);

        let content =
            fs::read(pkg_content_path).wrap_err("unable to retrieve the package from the db")?;

        Ok(Package::new(
            request.clone(),
            TarballCompressedPackageContent(content),
        ))
    }

    fn read_installation_metadata(
        &self,
        package_name: &String,
    ) -> Result<InstallationMetadata, CafError> {
        let metadata_file = fs::File::open(
            self.get_package_path(package_name)
                .join(ReadOnlyPackageManager::METADATA_FILE_NAME),
        )
        .wrap_err_with_code(
            CafErrorCode::PackageNotFound,
            format!(
                "unable to open the metadata file for the package version retrieval of package {}",
                package_name,
            ),
        )?;

        let reader = std::io::BufReader::new(metadata_file);

        serde_json::from_reader(reader).wrap_err_with_code(
            CafErrorCode::BrokenDatabaseState,
            format!(
                "unable to serialize the package metadata into JSON for package {}",
                package_name,
            ),
        )
    }

    pub fn retrieve_active_package_version(
        &self,
        package_name: &String,
    ) -> Result<PackageId, CafError> {
        let metadata = self.read_installation_metadata(package_name)?;

        Ok(PackageId {
            name: metadata.name,
            version: metadata.active_version,
        })
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
                        tracing::error!(
                            "failed to read subdirectory of {}: {err}",
                            packages_dir.to_string_lossy()
                        );
                        return vec![];
                    }
                };

                let versions = match fs::read_dir(pkg.path()) {
                    Ok(pkg_dirs) => pkg_dirs,
                    Err(err) => {
                        // TODO: maybe this needs to be reported better than this
                        tracing::error!(
                            "failed to read subdirectory of {}: {err}",
                            pkg.path().to_string_lossy()
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
                                tracing::error!(
                                    "failed to read subdirectory of {}: {err}",
                                    pkg.path().to_string_lossy()
                                );
                                return None;
                            }
                        };

                        if !version.path().is_dir() {
                            None
                        } else {
                            let pkg_name = pkg
                                .file_name()
                                .to_str()
                                .or_else(|| {
                                    tracing::error!(
                                        "package name is not valid UTF-8: {}",
                                        pkg.file_name().to_string_lossy()
                                    );
                                    None
                                })?
                                .to_string();

                            let pkg_version = version
                                .file_name()
                                .to_str()
                                .or_else(|| {
                                    tracing::error!(
                                        "version name is not valid UTF-8: {}",
                                        version.file_name().to_string_lossy()
                                    );
                                    None
                                })?
                                .to_string();

                            PackageId::new(pkg_name, pkg_version)
                                .inspect_err(|err| {
                                    tracing::error!(
                                        "failed to create package id for {}: {err}",
                                        pkg.file_name().to_string_lossy()
                                    )
                                })
                                .ok()
                        }
                    })
                    .collect()
            }))
    }
}

pub struct PackageManager {
    read_only_pkgman: ReadOnlyPackageManager,
}

// Design:
// all packages are stored as
//  /root_dir/pkgs/{package_name}/{package_version}/content.tar.xz
// each package has a metadata file stored at
//  /root_dir/pkgs/{package_name}/metadata.json
impl PackageManager {
    pub fn new(root_dir: PathBuf) -> Result<Self, CafError> {
        let read_only_pkgman = ReadOnlyPackageManager::new(root_dir.clone())?;
        Ok(PackageManager { read_only_pkgman })
    }

    pub fn pkg_exists_locally(&self, package_id: &PackageId) -> Result<bool, CafError> {
        self.read_only_pkgman.pkg_exists_locally(package_id)
    }

    // don't check if the package already exists. This allows the users to force
    // re-install a package if they want to
    pub fn install_package(&self, package: Package) -> Result<(), CafError> {
        let pkg_path = self.read_only_pkgman.get_package_path(&package.id.name);
        let install_path = pkg_path.join(package.id.version.to_string());

        let installation_metadata = self
            .read_only_pkgman
            .read_installation_metadata(&package.id.name)
            .map(|it| it.with_new_version(&package.id.version))
            .or_else(|it| {
                if *it.code() == CafErrorCode::PackageNotFound {
                    Ok(InstallationMetadata::new(&package.id))
                } else {
                    Err(it)
                }
            })?;

        let metadata_json = serde_json::to_string_pretty(&installation_metadata)
            .wrap_err_with_code(
                CafErrorCode::BrokenDatabaseState,
                format!(
                    "unable to serialize the package metadata into JSON for package {}",
                    package.id
                ),
            )?;

        fs::create_dir_all(&install_path).wrap_err(format!(
            "unable to create the package directory for package {}",
            package.id
        ))?;

        let installed_package_path = install_path.join(ReadOnlyPackageManager::CONTENT_FILE_NAME);
        fs::write(installed_package_path.clone(), package.content.0).wrap_err(format!(
            "unabel to write the file content of the package {} to the file {}",
            package.id,
            installed_package_path.to_string_lossy(),
        ))?;

        fs::write(
            pkg_path.join(ReadOnlyPackageManager::METADATA_FILE_NAME),
            metadata_json,
        )
        .wrap_err(format!(
            "unable to write the package metadata for package {}",
            package.id
        ))?;

        Ok(())
    }

    pub fn retrieve_active_package_version(
        &self,
        package_name: &String,
    ) -> Result<PackageId, CafError> {
        self.read_only_pkgman
            .retrieve_active_package_version(package_name)
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
                version: Version::parse("1.0.0").unwrap(),
            },
            content: TarballCompressedPackageContent(vec![1, 2, 3, 4]),
        }
    }

    #[test]
    fn install_package_writes_content_and_metadata() {
        // given:
        let temp_dir = tempdir().unwrap();
        let manager = PackageManager::new(temp_dir.path().to_path_buf()).unwrap();
        let package = fixture_package();

        // when:
        manager.install_package(package.clone()).unwrap();

        // then:
        let install_path = temp_dir
            .path()
            .join("pkgs")
            .join(&package.id.name)
            .join(package.id.version.to_string());

        let content_path = install_path.join(ReadOnlyPackageManager::CONTENT_FILE_NAME);

        let metadata_path = install_path
            .parent()
            .unwrap()
            .join(ReadOnlyPackageManager::METADATA_FILE_NAME);

        assert!(content_path.exists());
        assert_eq!(fs::read(content_path).unwrap(), package.content.0);

        let metadata_bytes = fs::read(metadata_path).unwrap();
        let metadata: InstallationMetadata = serde_json::from_slice(&metadata_bytes).unwrap();

        let expected_metadata = InstallationMetadata {
            name: package.id.name.clone(),
            active_version: package.id.version.clone(),
            verified_downloaded_versions: vec![package.id.version.clone()],
        };
        assert_eq!(metadata, expected_metadata);
    }

    #[test]
    fn retrieve_package_returns_stored_package() {
        // given:
        let temp_dir = tempdir().unwrap();
        let manager = PackageManager::new(temp_dir.path().to_path_buf()).unwrap();
        let package = fixture_package();

        manager.install_package(package.clone()).unwrap();

        // when:
        let retrieved = manager.read_only_pkgman.retrieve_package(&package.id).unwrap();

        // then:
        assert_eq!(retrieved, package);
    }

    #[test]
    fn retrieve_active_package_version_reads_metadata() {
        // given:
        let temp_dir = tempdir().unwrap();
        let manager = PackageManager::new(temp_dir.path().to_path_buf()).unwrap();
        let package = fixture_package();
        manager.install_package(package.clone()).unwrap();

        // when:
        let active = manager
            .retrieve_active_package_version(&package.id.name)
            .unwrap();

        // then:
        assert_eq!(active, package.id);
    }

    #[test]
    fn all_package_ids_iter_empty() {
        // given:
        let temp_dir = tempdir().unwrap();
        let manager = ReadOnlyPackageManager::new(temp_dir.path().to_path_buf()).unwrap();

        // when:
        let ids: Vec<PackageId> = manager.all_package_ids_iter().unwrap().collect();

        // then:
        assert!(ids.is_empty());
    }

    #[test]
    fn all_package_ids_iter_multiple_packages_and_versions() {
        // given:
        let temp_dir = tempdir().unwrap();
        let manager = PackageManager::new(temp_dir.path().to_path_buf()).unwrap();

        let pkgs = vec![
            PackageId {
                name: "a".into(),
                version: Version::parse("1.0.0").unwrap(),
            },
            PackageId {
                name: "a".into(),
                version: Version::parse("1.1.0").unwrap(),
            },
            PackageId {
                name: "b".into(),
                version: Version::parse("0.1.0").unwrap(),
            },
        ];

        for pkg_id in &pkgs {
            manager
                .install_package(Package {
                    id: pkg_id.clone(),
                    content: TarballCompressedPackageContent(vec![0]),
                })
                .unwrap();
        }

        // when:
        let mut ids: Vec<PackageId> = manager
            .read_only_pkgman
            .all_package_ids_iter()
            .unwrap()
            .collect();

        // then:
        ids.sort_by(|a, b| a.name.cmp(&b.name).then(a.version.cmp(&b.version)));

        let mut expected = pkgs;
        expected.sort_by(|a, b| a.name.cmp(&b.name).then(a.version.cmp(&b.version)));

        assert_eq!(ids, expected);
    }

    #[test]
    fn all_package_ids_iter_ignores_non_directory_files() {
        // given:
        let temp_dir = tempdir().unwrap();
        let manager = ReadOnlyPackageManager::new(temp_dir.path().to_path_buf()).unwrap();

        let pkgs_dir = temp_dir.path().join("pkgs");
        fs::create_dir_all(&pkgs_dir).unwrap();

        // A file in pkgs dir should be ignored (it might print an error but should not return it)
        fs::write(pkgs_dir.join("random_file"), "content").unwrap();

        // A package dir with a file and a version dir
        let pkg_dir = pkgs_dir.join("pkg_a");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("metadata.json"), "{}").unwrap();

        let version_dir = pkg_dir.join("1.0.0");
        fs::create_dir_all(&version_dir).unwrap();

        // when:
        let ids: Vec<PackageId> = manager.all_package_ids_iter().unwrap().collect();

        // then:
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].name, "pkg_a");
        assert_eq!(ids[0].version, Version::parse("1.0.0").unwrap());
    }

    #[test]
    fn pkg_exists_locally_returns_false_when_package_never_installed() {
        let temp_dir = tempdir().unwrap();
        let manager = PackageManager::new(temp_dir.path().to_path_buf()).unwrap();
        let pkg_id = PackageId::new("nonexistent", "1.0.0").unwrap();

        assert!(!manager.pkg_exists_locally(&pkg_id).unwrap());
    }

    #[test]
    fn pkg_exists_locally_returns_true_when_package_installed() {
        let temp_dir = tempdir().unwrap();
        let manager = PackageManager::new(temp_dir.path().to_path_buf()).unwrap();
        let package = fixture_package();
        manager.install_package(package.clone()).unwrap();

        assert!(manager.pkg_exists_locally(&package.id).unwrap());
    }

    #[test]
    fn pkg_exists_locally_returns_false_for_uninstalled_version_of_existing_package() {
        let temp_dir = tempdir().unwrap();
        let manager = PackageManager::new(temp_dir.path().to_path_buf()).unwrap();
        let package = fixture_package();
        manager.install_package(package.clone()).unwrap();

        let other_version = PackageId::new(&package.id.name, "2.0.0").unwrap();
        assert!(!manager.pkg_exists_locally(&other_version).unwrap());
    }

    #[test]
    fn pkg_exists_locally_returns_false_if_content_file_is_missing() {
        let temp_dir = tempdir().unwrap();
        let manager = PackageManager::new(temp_dir.path().to_path_buf()).unwrap();
        let package = fixture_package();
        manager.install_package(package.clone()).unwrap();

        let content_path = temp_dir
            .path()
            .join("pkgs")
            .join(&package.id.name)
            .join(package.id.version.to_string())
            .join(ReadOnlyPackageManager::CONTENT_FILE_NAME);

        fs::remove_file(content_path).unwrap();

        assert!(!manager.pkg_exists_locally(&package.id).unwrap());
    }

    #[test]
    fn pkg_exists_locally_returns_error_on_corrupted_metadata() {
        let temp_dir = tempdir().unwrap();
        let manager = PackageManager::new(temp_dir.path().to_path_buf()).unwrap();
        let pkg_dir = temp_dir.path().join("pkgs").join("corrupt_pkg");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(
            pkg_dir.join(ReadOnlyPackageManager::METADATA_FILE_NAME),
            "invalid json",
        )
        .unwrap();

        let pkg_id = PackageId::new("corrupt_pkg", "1.0.0").unwrap();
        let err = manager.pkg_exists_locally(&pkg_id).unwrap_err();
        assert_eq!(*err.code(), CafErrorCode::BrokenDatabaseState);
    }

    #[test]
    fn install_package_multiple_versions_updates_metadata_and_availability() {
        let temp_dir = tempdir().unwrap();
        let manager = PackageManager::new(temp_dir.path().to_path_buf()).unwrap();

        let pkg_v1 = Package {
            id: PackageId::new("widget", "1.0.0").unwrap(),
            content: TarballCompressedPackageContent(vec![1, 2, 3]),
        };
        let pkg_v2 = Package {
            id: PackageId::new("widget", "2.0.0").unwrap(),
            content: TarballCompressedPackageContent(vec![4, 5, 6]),
        };

        manager.install_package(pkg_v1.clone()).unwrap();
        assert!(manager.pkg_exists_locally(&pkg_v1.id).unwrap());
        assert!(!manager.pkg_exists_locally(&pkg_v2.id).unwrap());

        // Install second version
        manager.install_package(pkg_v2.clone()).unwrap();
        assert!(manager.pkg_exists_locally(&pkg_v1.id).unwrap());
        assert!(manager.pkg_exists_locally(&pkg_v2.id).unwrap());

        // Check active version is updated to latest installed
        let active = manager
            .retrieve_active_package_version(&"widget".to_string())
            .unwrap();
        assert_eq!(active, pkg_v2.id);

        // Check metadata contents
        let metadata_path = temp_dir
            .path()
            .join("pkgs")
            .join("widget")
            .join(ReadOnlyPackageManager::METADATA_FILE_NAME);
        let metadata_bytes = fs::read(metadata_path).unwrap();
        let metadata: InstallationMetadata = serde_json::from_slice(&metadata_bytes).unwrap();

        assert_eq!(metadata.active_version, pkg_v2.id.version);
        assert_eq!(
            metadata.verified_downloaded_versions,
            vec![pkg_v1.id.version, pkg_v2.id.version]
        );
    }

    #[test]
    fn install_package_reinstall_same_version_deduplicates_verified_versions() {
        let temp_dir = tempdir().unwrap();
        let manager = PackageManager::new(temp_dir.path().to_path_buf()).unwrap();
        let package = fixture_package();

        manager.install_package(package.clone()).unwrap();
        manager.install_package(package.clone()).unwrap();

        let metadata_path = temp_dir
            .path()
            .join("pkgs")
            .join(&package.id.name)
            .join(ReadOnlyPackageManager::METADATA_FILE_NAME);
        let metadata_bytes = fs::read(metadata_path).unwrap();
        let metadata: InstallationMetadata = serde_json::from_slice(&metadata_bytes).unwrap();

        assert_eq!(
            metadata.verified_downloaded_versions,
            vec![package.id.version]
        );
    }

    #[test]
    fn read_only_pkgman_pkg_exists_locally() {
        let temp_dir = tempdir().unwrap();
        let manager = PackageManager::new(temp_dir.path().to_path_buf()).unwrap();
        let package = fixture_package();
        manager.install_package(package.clone()).unwrap();

        let ro_manager = ReadOnlyPackageManager::new(temp_dir.path().to_path_buf()).unwrap();
        assert!(ro_manager.pkg_exists_locally(&package.id).unwrap());

        let nonexistent = PackageId::new("other", "1.0.0").unwrap();
        assert!(!ro_manager.pkg_exists_locally(&nonexistent).unwrap());
    }

    #[test]
    fn installation_metadata_serialization_camel_case() {
        let metadata = InstallationMetadata {
            name: "test-pkg".to_string(),
            active_version: Version::parse("1.2.3").unwrap(),
            verified_downloaded_versions: vec![
                Version::parse("1.0.0").unwrap(),
                Version::parse("1.2.3").unwrap(),
            ],
        };

        let json_str = serde_json::to_string(&metadata).unwrap();
        assert!(json_str.contains("\"activeVersion\":\"1.2.3\""));
        assert!(json_str.contains("\"verifiedDownloadedVersions\":[\"1.0.0\",\"1.2.3\"]"));

        let deserialized: InstallationMetadata = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized, metadata);
    }
}
