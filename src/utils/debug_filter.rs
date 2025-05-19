use crate::components::advertisement::Advertisement;
use crate::components::filters::Filter;


pub struct DebugFilter;
impl DebugFilter {
    pub fn new() -> Self {
        Self
    }
}
impl Filter<Advertisement> for DebugFilter {
    fn filter(&self, advertisement: &mut Advertisement) -> bool {
        println!("DebugFilter: {:?}", advertisement);
        true
    }
}
