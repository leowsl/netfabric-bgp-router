use crate::components::advertisement::Advertisement;
use crate::components::route::{PathElement, Route};
use ip_network::IpNetwork;
use log::error;

pub trait Filter<T>: 'static + Send + Sync
where
    T: 'static + Send + Sync,
    T: Clone,
{
    fn filter(&self, element: &mut T) -> bool;
    fn filter_type(&self) -> &'static str {
        std::any::type_name::<T>()
    }
}

pub struct FilterHost {
    host: String,
}
impl FilterHost {
    pub fn new(host: String) -> Self {
        Self { host }
    }
}
impl Filter<Advertisement> for FilterHost {
    fn filter(&self, ad: &mut Advertisement) -> bool {
        ad.host == self.host
    }
}

pub struct FilterOwnAS {
    asn: u64,
}
impl FilterOwnAS {
    pub fn new(asn: u64) -> Self {
        Self { asn }
    }
}
impl Filter<Advertisement> for FilterOwnAS {
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

pub struct FilterIpNetworkBlacklist {
    ip_network: IpNetwork,
}
impl FilterIpNetworkBlacklist {
    pub fn new(ip_network: IpNetwork) -> Self {
        Self { ip_network }
    }
}
impl Filter<Route> for FilterIpNetworkBlacklist {
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
    fn test_filter_own_as() {
        let filter = FilterOwnAS::new(10);
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
        let filter = FilterHost::new("192.168.1.1".to_string());
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
}
