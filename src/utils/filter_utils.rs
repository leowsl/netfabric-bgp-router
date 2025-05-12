use crate::components::filters::Filter;

pub fn apply_filters<'a, T: 'static + Send + Sync>(
    element: &'a mut T,
    filters: &Vec<Box<dyn Filter<T>>>,
) -> Option<&'a mut T> 
where
    T: Clone,
{
    for filter in filters {
        if !filter.filter(element){
            return None;
        }
    }
    Some(element)
}
pub fn apply_filters_vec<'a, T: 'static + Send + Sync>(
    elements: Vec<&'a mut T>,
    filters: &Vec<Box<dyn Filter<T>>>,
) -> Vec<&'a mut T> 
where
    T: Clone,
{
    elements
        .into_iter()
        .filter_map(|element| apply_filters(element, filters))
        .collect()
}
