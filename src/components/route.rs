use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PathElement {
    ASN(u64),
    ASPath(Vec<u64>),
}
pub type Path = Vec<PathElement>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Route {
    pub prefix: String,
    pub next_hop: String,
    pub as_path: Path,
    pub community: Vec<Vec<u64>>,
}

impl Route {
    pub fn new() -> Self {
        Default::default()
    }
}