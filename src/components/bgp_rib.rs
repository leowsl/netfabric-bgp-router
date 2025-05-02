use radix_trie::{Trie, TrieCommon, TrieKey};
use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BgpTableKey(IpAddr);

impl BgpTableKey {
    pub fn new(addr: IpAddr) -> Self {
        Self(addr)
    }
}

impl TrieKey for BgpTableKey {
    fn encode_bytes(&self) -> Vec<u8> {
        match self.0 {
            IpAddr::V4(addr) => addr.octets().to_vec(),
            IpAddr::V6(addr) => addr.octets().to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RibEntry {
    data: String,
}

impl RibEntry {
    pub fn new(data: String) -> Self {
        Self { data }
    }
}

#[derive(Debug, Clone)]
pub struct BgpTable {
    trie: Trie<BgpTableKey, RibEntry>,
}

impl BgpTable {
    pub fn new() -> Self {
        Self { trie: Trie::new() }
    }

    pub fn insert(&mut self, key: BgpTableKey, entry: RibEntry) {
        self.trie.insert(key, entry);
    }

    pub fn remove(&mut self, key: BgpTableKey) {
        self.trie.remove(&key);
    }

    pub fn get(&self, key: BgpTableKey) -> Option<&RibEntry> {
        self.trie.get(&key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&BgpTableKey, &RibEntry)> {
        self.trie.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    #[test]
    fn test_insert() {
        let mut table = BgpTable::new();
        let key = BgpTableKey::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        let entry = RibEntry::new("test".to_string());

        table.insert(key, entry);
        assert_eq!(table.iter().count(), 1);
    }

    #[test]
    fn test_remove() {
        let mut table = BgpTable::new();
        let key = BgpTableKey::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        let entry = RibEntry::new("test".to_string());

        table.insert(key.clone(), entry);
        table.remove(key);
        assert_eq!(table.iter().count(), 0);
    }

    #[test]
    fn test_get() {
        let mut table = BgpTable::new();
        let key = BgpTableKey::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        let entry = RibEntry::new("test".to_string());

        table.insert(key.clone(), entry.clone());
        let res = table.get(key).unwrap();
        assert_eq!(res, &entry);
    }
}
