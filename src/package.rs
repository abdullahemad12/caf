use serde::{Deserialize, Serialize};

// TODO: probably some metadata needs to be propagated here like the version
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageId {
    pub name: String,
    pub version: String,
}

// Contents of the packages are compressed into a single file (e.g. zip, tar.gz, etc..)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CompressedPackageContent(pub Vec<u8>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Package {
    pub id: PackageId,
    pub content: CompressedPackageContent,
}
