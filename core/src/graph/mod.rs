use anyhow::Result;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub struct KnowledgeGraph {
    graph: DiGraph<String, String>,
    node_map: HashMap<String, NodeIndex>,
    _data_dir: std::path::PathBuf,
}

impl KnowledgeGraph {
    pub fn new(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        Ok(Self {
            graph: DiGraph::new(),
            node_map: HashMap::new(),
            _data_dir: data_dir.to_path_buf(),
        })
    }

    pub fn add_edge(&mut self, source: &str, target: &str, predicate: &str) {
        let s_idx = *self
            .node_map
            .entry(source.to_string())
            .or_insert_with(|| self.graph.add_node(source.to_string()));
        let t_idx = *self
            .node_map
            .entry(target.to_string())
            .or_insert_with(|| self.graph.add_node(target.to_string()));
        self.graph.add_edge(s_idx, t_idx, predicate.to_string());
    }

    pub fn bfs_traverse(&self, start: &str, depth: usize) -> Vec<(String, usize)> {
        let mut results = Vec::new();
        let start_idx = match self.node_map.get(start) {
            Some(&idx) => idx,
            None => return results,
        };

        let mut visited = HashSet::new();
        let mut queue = vec![(start_idx, 0)];
        visited.insert(start_idx);

        while let Some((node, d)) = queue.pop() {
            if d > 0 {
                if let Some(name) = self.graph.node_weight(node) {
                    results.push((name.clone(), d));
                }
            }
            if d >= depth {
                continue;
            }
            for neighbor in self.graph.neighbors(node) {
                if visited.insert(neighbor) {
                    queue.push((neighbor, d + 1));
                }
            }
        }

        results
    }

    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }
}
