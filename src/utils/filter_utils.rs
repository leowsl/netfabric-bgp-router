use crate::components::filters::Filter;

pub fn apply_filters<'a, T: 'static + Send + Sync>(
    element: &'a T,
    filters: &Vec<Box<dyn Filter<T>>>,
) -> Option<&'a T> {
    if filters.iter().all(|filter| filter.filter(&element)) {
        return Some(element);
    }
    None
}
pub fn apply_filters_vec<'a, T: 'static + Send + Sync>(
    elements: Vec<&'a T>,
    filters: &Vec<Box<dyn Filter<T>>>,
) -> Vec<&'a T> {
    elements
        .into_iter()
        .filter(|element| filters.iter().all(|filter| filter.filter(element)))
        .collect()
}
