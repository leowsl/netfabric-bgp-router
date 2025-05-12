pub trait Filter<T>: 'static + Send + Sync {
    fn filter(&self, element: &T) -> bool;
}
