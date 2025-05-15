use crate::components::advertisement::Advertisement;
use crate::components::route::{PathElement, Route};
use ip_network::IpNetwork;
use std::fmt::Debug;

pub trait Filter<T>: 'static + Send + Sync
where
    T: 'static + Send + Sync + Clone,
{
    fn filter(&self, element: &mut T) -> bool;
    fn filter_type(&self) -> &'static str {
        std::any::type_name::<T>()
    }
}

impl<T> Debug for dyn Filter<T>
where
    T: 'static + Send + Sync + Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", std::any::type_name::<Self>())
    }
}

pub struct NoFilter;
impl<T> Filter<T> for NoFilter
where
    T: 'static + Send + Sync + Clone,
{
    fn filter(&self, _element: &mut T) -> bool {
        true
    }
}

pub struct VecFilterAny<T>(Box<dyn Filter<T>>);
impl<T> VecFilterAny<T>
where
    T: Clone + 'static + Send + Sync,
{
    pub fn new<F: Filter<T>>(filter: F) -> Self {
        Self(Box::new(filter))
    }
}
impl<T> Filter<Vec<T>> for VecFilterAny<T>
where
    T: Clone + 'static + Send + Sync,
{
    fn filter(&self, element: &mut Vec<T>) -> bool {
        element.iter_mut().map(|e| self.0.filter(e)).any(|e| e)
    }
}

pub struct VecFilterAll<T>(Box<dyn Filter<T>>);
impl<T> VecFilterAll<T>
where
    T: Clone + 'static + Send + Sync,
{
    pub fn new<F: Filter<T>>(filter: F) -> Self {
        Self(Box::new(filter))
    }
}
impl<T> Filter<Vec<T>> for VecFilterAll<T>
where
    T: Clone + 'static + Send + Sync,
{
    fn filter(&self, element: &mut Vec<T>) -> bool {
        element.iter_mut().map(|e| self.0.filter(e)).all(|e| e)
    }
}

pub struct CombinedOrFilter<T>
where
    T: 'static + Send + Sync + Clone,
{
    filters: Vec<Box<dyn Filter<T>>>,
}
impl<T> CombinedOrFilter<T>
where
    T: 'static + Send + Sync + Clone,
{
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
        }
    }

    pub fn from_vec<F: Filter<T>>(filters: Vec<F>) -> Self {
        let filters = filters
            .into_iter()
            .map(|f| Box::new(f) as Box<dyn Filter<T>>)
            .collect();
        Self { filters }
    }

    pub fn add_filter<F: Filter<T>>(&mut self, filter: F) {
        self.filters.push(Box::new(filter));
    }
}
impl<T> Filter<T> for CombinedOrFilter<T>
where
    T: 'static + Send + Sync + Clone,
{
    fn filter(&self, element: &mut T) -> bool {
        self.filters.iter().any(|filter| filter.filter(element))
    }
}

pub struct CombinedAndFilter<T>
where
    T: 'static + Send + Sync,
    T: Clone,
{
    filters: Vec<Box<dyn Filter<T>>>,
}
impl<T> CombinedAndFilter<T>
where
    T: 'static + Send + Sync + Clone,
{
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
        }
    }

    pub fn from_vec<F: Filter<T>>(filters: Vec<F>) -> Self {
        let filters = filters
            .into_iter()
            .map(|f| Box::new(f) as Box<dyn Filter<T>>)
            .collect();
        Self { filters }
    }

    pub fn add_filter<F: Filter<T>>(&mut self, filter: F) {
        self.filters.push(Box::new(filter));
    }
}
impl<T> Filter<T> for CombinedAndFilter<T>
where
    T: 'static + Send + Sync + Clone,
{
    fn filter(&self, element: &mut T) -> bool {
        self.filters.iter().all(|filter| filter.filter(element))
    }
}

pub struct HostFilter {
    host: String,
}
impl HostFilter {
    pub fn new(host: String) -> Self {
        Self { host }
    }
}
impl Filter<Advertisement> for HostFilter {
    fn filter(&self, ad: &mut Advertisement) -> bool {
        ad.host == self.host
    }
}

pub struct OwnASFilter {
    asn: u64,
}
impl OwnASFilter {
    pub fn new(asn: u64) -> Self {
        Self { asn }
    }
}
impl Filter<Advertisement> for OwnASFilter {
    fn filter(&self, ad: &mut Advertisement) -> bool {
        if let Some(path) = &ad.path {
            if path.iter().any(|pe| match pe {
                PathElement::ASN(asn) => asn == &self.asn,
                PathElement::ASSet(path) => path.iter().any(|p| *p == self.asn),
            }) {
                return false;
            }
        }
        return true;
    }
}

pub struct IpNetworkBlacklistFilter {
    ip_network: IpNetwork,
}
impl IpNetworkBlacklistFilter {
    pub fn new(ip_network: IpNetwork) -> Self {
        Self { ip_network }
    }
}
impl Filter<Route> for IpNetworkBlacklistFilter {
    fn filter(&self, route: &mut Route) -> bool {
        if self.ip_network.netmask() <= route.prefix.netmask() {
            self.ip_network.contains(route.prefix.network_address())
        } else {
            route.prefix.contains(self.ip_network.network_address())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_debug() {
        let host_filter: Box<dyn Filter<Advertisement>> = Box::new(HostFilter::new("192.168.1.1".to_string()));
        println!("{:?}", host_filter);
    }

    #[test]
    fn test_filter_own_as() {
        let filter = OwnASFilter::new(10);
        let mut ad1 = Advertisement {
            path: Some(vec![
                PathElement::ASN(1),
                PathElement::ASN(2),
                PathElement::ASN(3),
                PathElement::ASSet(vec![100, 101, 102]),
            ]),
            ..Default::default()
        };
        let mut ad2 = Advertisement {
            path: Some(vec![
                PathElement::ASN(1),
                PathElement::ASN(2),
                PathElement::ASN(10),
                PathElement::ASSet(vec![100, 101, 102]),
            ]),
            ..Default::default()
        };
        let mut ad3 = Advertisement {
            path: Some(vec![
                PathElement::ASN(1),
                PathElement::ASN(2),
                PathElement::ASN(3),
                PathElement::ASSet(vec![10, 101, 102]),
            ]),
            ..Default::default()
        };
        assert!(filter.filter(&mut ad1));
        assert!(!filter.filter(&mut ad2));
        assert!(!filter.filter(&mut ad3));
    }

    #[test]
    fn test_filter_host() {
        let filter = HostFilter::new("192.168.1.1".to_string());
        let mut ad1 = Advertisement {
            host: "192.168.1.1".to_string(),
            ..Default::default()
        };
        let mut ad2 = Advertisement {
            host: "192.168.1.2".to_string(),
            ..Default::default()
        };
        assert!(filter.filter(&mut ad1));
        assert!(!filter.filter(&mut ad2));
    }

    #[test]
    fn test_combined_and_filter() {
        let filter1 = HostFilter::new("192.168.1.1".to_string());
        let filter2 = OwnASFilter::new(10);

        let mut combined = CombinedAndFilter::new();
        combined.add_filter(filter1);
        combined.add_filter(filter2);

        // Test case 1: Both filters pass
        let mut ad1 = Advertisement {
            host: "192.168.1.1".to_string(),
            path: Some(vec![
                PathElement::ASN(1),
                PathElement::ASN(2),
                PathElement::ASN(3),
            ]),
            ..Default::default()
        };
        assert!(combined.filter(&mut ad1));

        // Test case 2: First filter passes, second fails
        let mut ad2 = Advertisement {
            host: "192.168.1.1".to_string(),
            path: Some(vec![
                PathElement::ASN(1),
                PathElement::ASN(2),
                PathElement::ASN(10),
            ]),
            ..Default::default()
        };
        assert!(!combined.filter(&mut ad2));

        // Test case 3: First filter fails, second passes
        let mut ad3 = Advertisement {
            host: "192.168.1.2".to_string(),
            path: Some(vec![
                PathElement::ASN(1),
                PathElement::ASN(2),
                PathElement::ASN(3),
            ]),
            ..Default::default()
        };
        assert!(!combined.filter(&mut ad3));

        // Test case 4: Both filters fail
        let mut ad4 = Advertisement {
            host: "192.168.1.2".to_string(),
            path: Some(vec![
                PathElement::ASN(1),
                PathElement::ASN(2),
                PathElement::ASN(10),
            ]),
            ..Default::default()
        };
        assert!(!combined.filter(&mut ad4));
    }

    #[test]
    fn test_combined_or_filter() {
        let filter1 = HostFilter::new("192.168.1.1".to_string());
        let filter2 = HostFilter::new("192.168.1.2".to_string());

        let combined = CombinedOrFilter::from_vec(vec![filter1, filter2]);

        // Test case 1: First filter passes
        let mut ad1 = Advertisement {
            host: "192.168.1.1".to_string(),
            ..Default::default()
        };
        assert!(combined.filter(&mut ad1));

        // Test case 2: Second filter passes
        let mut ad2 = Advertisement {
            host: "192.168.1.2".to_string(),
            ..Default::default()
        };
        assert!(combined.filter(&mut ad2));

        // Test case 3: Both filters fail
        let mut ad3 = Advertisement {
            host: "192.168.1.3".to_string(),
            ..Default::default()
        };
        assert!(!combined.filter(&mut ad3));
    }

    #[test]
    fn test_vec_filter_any() {
        let host_filter = HostFilter::new("192.168.1.1".to_string());
        let vec_filter = VecFilterAny::new(host_filter);
        let mut ads = vec![
            Advertisement {
                host: "192.168.1.2".to_string(),
                ..Default::default()
            },
            Advertisement {
                host: "192.168.1.1".to_string(),
                ..Default::default()
            },
            Advertisement {
                host: "192.168.1.3".to_string(),
                ..Default::default()
            },
        ];
        assert!(vec_filter.filter(&mut ads));

        let mut no_matches = vec![
            Advertisement {
                host: "192.168.1.2".to_string(),
                ..Default::default()
            },
            Advertisement {
                host: "192.168.1.3".to_string(),
                ..Default::default()
            },
        ];
        assert!(!vec_filter.filter(&mut no_matches));
    }

    #[test]
    fn test_vec_filter_all() {
        let host_filter = HostFilter::new("192.168.1.1".to_string());
        let vec_filter = VecFilterAll::new(host_filter);
        let mut ads = vec![
            Advertisement {
                host: "192.168.1.1".to_string(),
                ..Default::default()
            },
            Advertisement {
                host: "192.168.1.1".to_string(),
                ..Default::default()
            },
        ];
        assert!(vec_filter.filter(&mut ads));
        ads.push(Advertisement {
            host: "192.168.1.2".to_string(),
            ..Default::default()
        });
        assert!(!vec_filter.filter(&mut ads));
    }
}
