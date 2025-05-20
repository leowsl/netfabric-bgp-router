use crate::{
    components::bgp::bgp_config::{ProcessConfig, SessionConfig},
    modules::router::RouterOptions,
};
use uuid::Uuid;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkTopology {
    topology: Topology<NetworkTopologyVertex, NetworkTopologyEdge>,
}
impl NetworkTopology {
    pub fn new() -> Self {
        Self {
            topology: Topology::new(),
        }
    }

    pub fn add_router(
        &mut self,
        router_options: RouterOptions,
        process_config: ProcessConfig,
    ) -> Uuid {
        let vertex_id = Uuid::new_v4();
        let vertex = NetworkTopologyVertex {
            router_options,
            process_config,
        };
        self.topology.add_vertex(vertex_id.clone(), vertex);
        vertex_id
    }

    pub fn add_connection(
        &mut self,
        vertex1_id: Uuid,
        vertex2_id: Uuid,
        session_configs: (SessionConfig, SessionConfig),
        link_buffer_size: u32,
    ) {
        let edge = NetworkTopologyEdge {
            session_configs,
            link_buffer_size,
        };
        self.topology.add_edge((vertex1_id, vertex2_id, edge));
    }

    pub fn get_topology(&self) -> &Topology<NetworkTopologyVertex, NetworkTopologyEdge> {
        &self.topology
    }

    pub fn get_topology_mut(
        &mut self,
    ) -> &mut Topology<NetworkTopologyVertex, NetworkTopologyEdge> {
        &mut self.topology
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkTopologyVertex {
    pub router_options: RouterOptions,
    pub process_config: ProcessConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkTopologyEdge {
    pub session_configs: (SessionConfig, SessionConfig),
    pub link_buffer_size: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Topology<V, E, K = Uuid>
where
    K: Default + PartialEq + Eq,
    E: Default,
    V: Default,
{
    vertices: Vec<(K, V)>,
    edges: Vec<(K, K, E)>,
}

impl<V, E, K> Topology<V, E, K>
where
    K: Default + PartialEq + Eq + Clone,
    E: Default,
    V: Default,
{
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn add_edge(&mut self, edge: (K, K, E)) {
        if !self.vertices.iter().any(|(k, _)| k == &edge.0) {
            self.add_vertex(edge.0.clone(), Default::default());
        }
        if !self.vertices.iter().any(|(k, _)| k == &edge.1) {
            self.add_vertex(edge.1.clone(), Default::default());
        }
        self.edges.push(edge);
    }

    pub fn add_vertex(&mut self, vertex: K, value: V) {
        // Overwrite value if exists
        if let Some(idx) = self.vertices.iter().position(|(k, _)| k == &vertex) {
            self.vertices[idx].1 = value;
            return;
        }
        self.vertices.push((vertex, value));
    }

    pub fn from_edges(edges: Vec<(K, K, E)>) -> Self {
        let mut topology = Self::new();
        for edge in edges {
            topology.add_edge(edge);
        }
        topology
    }

    pub fn contains_vertex(&self, vertex: &K) -> bool {
        self.vertices.iter().any(|(k, _)| k == vertex)
    }

    pub fn iter_vertices(&self) -> impl Iterator<Item = (&K, &V)> {
        self.vertices.iter().map(|(k, v)| (k, v))
    }

    pub fn iter_edges(&self) -> impl Iterator<Item = (&K, &K, &E)> {
        self.edges.iter().map(|(k1, k2, e)| (k1, k2, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_topology_functions() {
        // Test create, add vertex, add edge, iter vertices, iter edges
        let mut topology = Topology::new();
        let v1 = Uuid::new_v4();
        let v2 = Uuid::new_v4();
        topology.add_vertex(v1.clone(), 1.0);
        topology.add_vertex(v2.clone(), 2.0);
        topology.add_vertex(v2.clone(), 1.0);
        topology.add_edge((v1.clone(), v2.clone(), 1.0));

        assert_eq!(
            topology.iter_vertices().collect::<Vec<_>>(),
            vec![(&v1, &1.0), (&v2, &1.0)]
        );

        assert_eq!(
            topology.iter_edges().collect::<Vec<_>>(),
            vec![(&v1, &v2, &1.0)]
        );
    }

    #[test]
    fn create_network_topology() {
        let mut network_topology = NetworkTopology::new();
        let topology = network_topology.get_topology_mut();

        // Create Full Mesh Topology for 4 nodes
        let v1 = Uuid::new_v4();
        topology.add_vertex(v1.clone(), NetworkTopologyVertex::default());
        let v2 = Uuid::new_v4();
        topology.add_vertex(v2.clone(), NetworkTopologyVertex::default());
        let v3 = Uuid::new_v4();
        topology.add_vertex(v2.clone(), NetworkTopologyVertex::default());
        let v4 = Uuid::new_v4();
        topology.add_vertex(v2.clone(), NetworkTopologyVertex::default());

        topology.add_edge((v1.clone(), v2.clone(), NetworkTopologyEdge::default()));
        topology.add_edge((v1.clone(), v3.clone(), NetworkTopologyEdge::default()));
        topology.add_edge((v2.clone(), v4.clone(), NetworkTopologyEdge::default()));
        topology.add_edge((v2.clone(), v3.clone(), NetworkTopologyEdge::default()));
        topology.add_edge((v2.clone(), v4.clone(), NetworkTopologyEdge::default()));
        topology.add_edge((v3.clone(), v4.clone(), NetworkTopologyEdge::default()));

        assert_eq!(topology.iter_vertices().count(), 4);
        assert_eq!(topology.iter_edges().count(), 6);
    }
}
