use std::collections::{HashMap, HashSet, VecDeque};

use memory_core::{Edge, Node, NodeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphExpansionConfig {
    pub max_hops: usize,
    pub max_candidates: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NeighborCandidate {
    pub node_id: NodeId,
    pub edge_strength: f32,
    pub hop_distance: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct GraphExpander {
    config: GraphExpansionConfig,
}

impl GraphExpander {
    #[must_use]
    pub fn new(config: GraphExpansionConfig) -> Self {
        Self { config }
    }

    pub fn expand(&self, seeds: &[NodeId], nodes: &[Node], edges: &[Edge]) -> Vec<NeighborCandidate> {
        if seeds.is_empty() {
            return Vec::new();
        }

        let valid_ids = nodes.iter().map(|node| node.id).collect::<HashSet<_>>();
        let mut adjacency = HashMap::<NodeId, Vec<(NodeId, f32)>>::new();
        for edge in edges {
            if valid_ids.contains(&edge.from_node_id) && valid_ids.contains(&edge.to_node_id) {
                adjacency
                    .entry(edge.from_node_id)
                    .or_default()
                    .push((edge.to_node_id, edge.weight));
                adjacency
                    .entry(edge.to_node_id)
                    .or_default()
                    .push((edge.from_node_id, edge.weight));
            }
        }

        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        for seed in seeds {
            queue.push_back((*seed, 0usize));
            visited.insert(*seed);
        }

        let seed_ids = seeds.iter().copied().collect::<HashSet<_>>();
        let mut candidates = HashMap::<NodeId, NeighborCandidate>::new();
        while let Some((current, hop)) = queue.pop_front() {
            if hop >= self.config.max_hops {
                continue;
            }

            for (neighbor, weight) in adjacency.get(&current).into_iter().flatten() {
                let next_hop = hop + 1;
                if !seed_ids.contains(neighbor) {
                    let entry = candidates.entry(*neighbor).or_insert(NeighborCandidate {
                        node_id: *neighbor,
                        edge_strength: *weight,
                        hop_distance: next_hop,
                    });
                    if *weight > entry.edge_strength || next_hop < entry.hop_distance {
                        entry.edge_strength = entry.edge_strength.max(*weight);
                        entry.hop_distance = entry.hop_distance.min(next_hop);
                    }
                }
                if visited.insert(*neighbor) {
                    queue.push_back((*neighbor, next_hop));
                }
            }
        }

        let mut values = candidates.into_values().collect::<Vec<_>>();
        values.sort_by(|left, right| {
            right
                .edge_strength
                .total_cmp(&left.edge_strength)
                .then_with(|| left.hop_distance.cmp(&right.hop_distance))
        });
        values.truncate(self.config.max_candidates);
        values
    }
}
