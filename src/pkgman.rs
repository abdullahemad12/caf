use std::path::PathBuf;

use crate::package;

pub struct PackageManager {
    // should be maintained in a db
    package_id: package::PackageId,
    package_path: PathBuf,
}

impl PackageManager {
    pub fn new(package_id: package::PackageId, package_path: PathBuf) -> Self {
        return PackageManager {
            package_id,
            package_path,
        };
    }

    pub fn retrieve_package(&self, request: package::PackageId) -> Option<package::Package> {
        if request.name == self.package_id.name {
            return Some(package::Package(
                std::fs::read(self.package_path.clone()).ok()?,
            ));
        }
        return None;
    }
}
