use serde::{Deserialize, Serialize};

// TODO: probably some metadata needs to be propagated here like the version
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageId {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Package(pub Vec<u8>);
