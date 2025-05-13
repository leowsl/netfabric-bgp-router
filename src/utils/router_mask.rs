use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, PartialEq, Hash, Eq, Copy, Clone, serde::Serialize, serde::Deserialize)]
pub struct RouterMask(pub u64);
impl Default for RouterMask {
    fn default() -> Self {
        Self(0)
    }
}
impl RouterMask {
    pub fn len(&self) -> usize {
        self.0.count_ones() as usize
    }
    pub fn all() -> Self {
        Self(u64::MAX)
    }
    pub fn contains(&self, other: &Self) -> bool {
        other.0 != 0 && self.0 & other.0 == other.0
    }
    pub fn remove(&mut self, other: &Self) {
       self.0 &= !other.0;
    }
    pub fn combine(&self, other: &Self) -> Self {
        Self(self.0 | other.0)
    }
    pub fn combine_with(&mut self, other: &Self) {
        self.0 |= other.0;
    }
}

#[derive(Debug, Clone)]
pub struct RouterMaskMap {
    default: RouterMask,
    mask: RouterMask,
    map: HashMap<Uuid, RouterMask>,
}
impl RouterMaskMap {
    pub fn new() -> Self {
        Self {
            default: RouterMask::default(),
            mask: RouterMask::default(),
            map: HashMap::new(),
        }
    }
    pub fn try_get(&self, router_id: &Uuid) -> &RouterMask {
        return self.map.get(router_id).unwrap_or(&self.default);
    }
    pub fn get(&mut self, router_id: &Uuid) -> &RouterMask {
        if self.map.contains_key(router_id) {
            return self.map.get(router_id).unwrap();
        }
        if self.mask.0 == u64::MAX {
            panic!("Router mask map is full");
        }
        for bit in 0..64 {
            let router_bit = 1 << bit;
            if self.mask.0 & router_bit == 0 {
                self.map.insert(router_id.clone(), RouterMask(router_bit));
                self.mask.0 |= router_bit;
                return self.map.get(router_id).unwrap();
            }
        }
        panic!("RouterMaskMap is corrupted");
    }
    pub fn get_all(&self) -> &RouterMask {
        &self.mask
    }
    pub fn remove(&mut self, router_id: &Uuid) {
        self.mask.0 ^= self.map.remove(router_id).unwrap().0;
    }
    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn get_all_ids(&self) -> Vec<Uuid> {
        self.map.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_router_mask_map_create() {
        let router_mask_map = RouterMaskMap::new();
        assert_eq!(router_mask_map.get_all(), &RouterMask(0));
    }

    #[test]
    fn test_router_mask_map_get() {
        let mut router_mask_map = RouterMaskMap::new();
        let r1 = Uuid::new_v4();
        let r2 = Uuid::new_v4();
        let _ = router_mask_map.get(&r1);
        let _ = router_mask_map.get(&r2);

        assert_eq!(router_mask_map.len(), 2);
        assert_eq!(router_mask_map.get(&r1), &RouterMask(0b01));
        assert_eq!(router_mask_map.get(&r2), &RouterMask(0b10));
        assert_eq!(router_mask_map.get_all(), &RouterMask(0b11))
    }

    #[test]
    fn test_router_mask_map_remove() {
        let mut router_mask_map = RouterMaskMap::new();
        let r1 = Uuid::new_v4();
        assert_eq!(router_mask_map.get(&r1), &RouterMask(0b1));
        router_mask_map.remove(&r1);
        assert_eq!(router_mask_map.get_all(), &RouterMask(0));
    }

    #[test]
    fn test_router_mask_map_get_all_ids() {
        let mut router_mask_map = RouterMaskMap::new();
        let r1 = Uuid::new_v4();
        let r2 = Uuid::new_v4();
        let _ = router_mask_map.get(&r1);
        let _ = router_mask_map.get(&r2);
        assert_eq!(router_mask_map.get_all_ids().sort(), vec![r1, r2].sort());
    }
}
