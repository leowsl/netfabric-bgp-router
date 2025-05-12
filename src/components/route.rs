use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PathElement {
    ASN(u64),
    ASSet(Vec<u64>),
}
pub type Path = Vec<PathElement>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Route {
    pub prefix: String,
    pub next_hop: String,
    pub as_path: Path,
}

impl Route {
    pub fn new() -> Self {
        Default::default()
    }
}