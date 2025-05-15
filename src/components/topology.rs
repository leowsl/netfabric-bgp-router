use std::collections::HashMap;
use std::hash::Hash;
use uuid::Uuid;

pub struct Topology<E, V, K = Uuid>
where
    K: Default + PartialEq + Eq + Hash,
    E: Default,
    V: Default,
{
    vertices: HashMap<K, V>,
    edges: Vec<(K, K, E)>,
}

impl<E, V, K> Topology<E, V, K>
where
    K: Default + PartialEq + Eq + Hash + Clone,
    E: Default,
    V: Default,
{
    pub fn new() -> Self {
        Self {
            edges: Vec::new(),
            vertices: HashMap::new(),
        }
    }

    pub fn add_edge(&mut self, edge: (K, K, E)) {
        if !self.vertices.contains_key(&edge.0) {
            self.add_vertex(edge.0.clone(), Default::default());
        }
        if !self.vertices.contains_key(&edge.1) {
            self.add_vertex(edge.1.clone(), Default::default());
        }
        self.edges.push(edge);
    }

    pub fn add_vertex(&mut self, vertex: K, value: V) {
        self.vertices.insert(vertex, value);
    }

    pub fn from_edges(edges: Vec<(K, K, E)>) -> Self {
        let mut topology = Self::new();
        for edge in edges {
            topology.add_edge(edge);
        }
        topology
    }

    pub fn iter_vertices(&self) -> impl Iterator<Item = (&K, &V)> {
        self.vertices.iter()
    }

    pub fn iter_edges(&self) -> impl Iterator<Item = &(K, K, E)> {
        self.edges.iter()
    }
}
