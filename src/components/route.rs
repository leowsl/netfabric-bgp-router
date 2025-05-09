use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Route {
    pub prefix: String,
    pub next_hop: String,
    pub as_path: Vec<u64>,
    pub community: Vec<Vec<u64>>,
}

impl Route {
    pub fn new() -> Self {
        Default::default()
    }
}