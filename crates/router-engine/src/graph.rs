//! Token graph for path discovery.
//!
//! Maintains an adjacency list of all tradeable pairs across registered DEX sources.
//! Supports BFS-based path finding with configurable max hops.

use crate::types::{Path, TokenId};
use std::collections::{HashMap, HashSet, VecDeque};

/// Edge in the token graph representing a tradeable pair on a specific DEX.
#[derive(Debug, Clone)]
pub struct Edge {
    pub target: TokenId,
    pub source: String,
    pub pool_address: String,
    pub fee_bps: u32,
    pub last_updated_ms: u64,
}

/// Token graph: adjacency list representation.
/// Each token maps to a list of edges (other tokens it can be swapped to).
#[derive(Debug, Default)]
pub struct TokenGraph {
    adjacency: HashMap<String, Vec<Edge>>,
    /// Set of all known tokens
    tokens: HashSet<String>,
}

impl TokenGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a bidirectional edge (trading pair) to the graph.
    pub fn add_pair(
        &mut self,
        token_a: &TokenId,
        token_b: &TokenId,
        source: &str,
        pool_address: &str,
        fee_bps: u32,
    ) {
        let key_a = token_a.canonical();
        let key_b = token_b.canonical();
        let now = chrono::Utc::now().timestamp_millis() as u64;

        self.tokens.insert(key_a.clone());
        self.tokens.insert(key_b.clone());

        // A -> B
        self.adjacency.entry(key_a.clone()).or_default().push(Edge {
            target: token_b.clone(),
            source: source.to_string(),
            pool_address: pool_address.to_string(),
            fee_bps,
            last_updated_ms: now,
        });

        // B -> A
        self.adjacency.entry(key_b).or_default().push(Edge {
            target: token_a.clone(),
            source: source.to_string(),
            pool_address: pool_address.to_string(),
            fee_bps,
            last_updated_ms: now,
        });
    }

    /// Remove all edges from a specific source (e.g., when re-syncing a DEX adapter).
    pub fn remove_source(&mut self, source: &str) {
        for edges in self.adjacency.values_mut() {
            edges.retain(|e| e.source != source);
        }
    }

    /// Get all neighbors of a token.
    pub fn neighbors(&self, token: &TokenId) -> &[Edge] {
        self.adjacency
            .get(&token.canonical())
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Total number of unique tokens in the graph.
    pub fn token_count(&self) -> usize {
        self.tokens.len()
    }

    /// Total number of edges (directed) in the graph.
    pub fn edge_count(&self) -> usize {
        self.adjacency.values().map(|v| v.len()).sum()
    }

    /// BFS: find all paths from `start` to `end` with at most `max_hops` hops.
    ///
    /// Returns paths sorted by hop count (shortest first).
    /// To avoid combinatorial explosion, limits total paths returned.
    pub fn find_paths(
        &self,
        start: &TokenId,
        end: &TokenId,
        max_hops: usize,
        max_paths: usize,
    ) -> Vec<Path> {
        let start_key = start.canonical();
        let end_key = end.canonical();

        if start_key == end_key {
            return vec![];
        }

        let mut results: Vec<Path> = Vec::new();

        // BFS state: (current_token_key, token_path, source_path, pool_path, visited_pools)
        let mut queue: VecDeque<(
            String,
            Vec<TokenId>,
            Vec<String>,
            Vec<String>,
            HashSet<String>,
        )> = VecDeque::new();

        queue.push_back((
            start_key.clone(),
            vec![start.clone()],
            vec![],
            vec![],
            HashSet::new(),
        ));

        while let Some((current_key, token_path, source_path, pool_path, visited_pools)) =
            queue.pop_front()
        {
            if results.len() >= max_paths {
                break;
            }

            if pool_path.len() >= max_hops {
                continue;
            }

            let edges = match self.adjacency.get(&current_key) {
                Some(e) => e,
                None => continue,
            };

            for edge in edges {
                let target_key = edge.target.canonical();

                // Don't reuse the same pool in one path
                if visited_pools.contains(&edge.pool_address) {
                    continue;
                }

                // Found the destination
                if target_key == end_key {
                    let mut new_tokens = token_path.clone();
                    new_tokens.push(edge.target.clone());

                    let mut new_sources = source_path.clone();
                    new_sources.push(edge.source.clone());

                    let mut new_pools = pool_path.clone();
                    new_pools.push(edge.pool_address.clone());

                    results.push(Path {
                        hops: new_sources.len(),
                        tokens: new_tokens,
                        sources: new_sources,
                        pool_addresses: new_pools,
                    });

                    if results.len() >= max_paths {
                        break;
                    }
                    continue;
                }

                // Don't revisit tokens already in path (avoid cycles that don't end at target)
                if token_path.iter().any(|t| t.canonical() == target_key) {
                    continue;
                }

                // Extend path
                let mut new_tokens = token_path.clone();
                new_tokens.push(edge.target.clone());

                let mut new_sources = source_path.clone();
                new_sources.push(edge.source.clone());

                let mut new_pools = pool_path.clone();
                new_pools.push(edge.pool_address.clone());

                let mut new_visited = visited_pools.clone();
                new_visited.insert(edge.pool_address.clone());

                queue.push_back((
                    target_key,
                    new_tokens,
                    new_sources,
                    new_pools,
                    new_visited,
                ));
            }
        }

        // Sort by hop count (prefer shorter paths)
        results.sort_by_key(|p| p.hops);
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xlm() -> TokenId {
        TokenId::Native
    }
    fn usdc() -> TokenId {
        TokenId::Classic {
            code: "USDC".into(),
            issuer: "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".into(),
        }
    }
    fn eth() -> TokenId {
        TokenId::Contract {
            address: "CETH_CONTRACT_ADDRESS".into(),
        }
    }

    #[test]
    fn test_direct_path() {
        let mut graph = TokenGraph::new();
        graph.add_pair(&xlm(), &usdc(), "soroswap", "pool_1", 30);

        let paths = graph.find_paths(&xlm(), &usdc(), 4, 100);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].hops, 1);
        assert_eq!(paths[0].tokens.len(), 2);
    }

    #[test]
    fn test_multi_hop_path() {
        let mut graph = TokenGraph::new();
        graph.add_pair(&xlm(), &usdc(), "soroswap", "pool_1", 30);
        graph.add_pair(&usdc(), &eth(), "aquarius", "pool_2", 30);

        let paths = graph.find_paths(&xlm(), &eth(), 4, 100);
        // Should find: XLM -> USDC -> ETH (2 hops)
        assert!(paths.iter().any(|p| p.hops == 2));
    }

    #[test]
    fn test_multiple_paths() {
        let mut graph = TokenGraph::new();
        graph.add_pair(&xlm(), &usdc(), "soroswap", "pool_1", 30);
        graph.add_pair(&xlm(), &usdc(), "aquarius", "pool_2", 20);
        graph.add_pair(&xlm(), &eth(), "soroswap", "pool_3", 30);
        graph.add_pair(&eth(), &usdc(), "aquarius", "pool_4", 30);

        let paths = graph.find_paths(&xlm(), &usdc(), 4, 100);
        // Direct: pool_1, pool_2; Indirect: XLM->ETH->USDC
        assert!(paths.len() >= 3);
    }

    #[test]
    fn test_no_path() {
        let mut graph = TokenGraph::new();
        graph.add_pair(&xlm(), &usdc(), "soroswap", "pool_1", 30);

        let paths = graph.find_paths(&xlm(), &eth(), 4, 100);
        assert!(paths.is_empty());
    }

    #[test]
    fn test_max_hops_limit() {
        let mut graph = TokenGraph::new();
        let a = TokenId::from_str_auto("A:ISSUER");
        let b = TokenId::from_str_auto("B:ISSUER");
        let c = TokenId::from_str_auto("C:ISSUER");
        let d = TokenId::from_str_auto("D:ISSUER");
        let e = TokenId::from_str_auto("E:ISSUER");

        graph.add_pair(&a, &b, "dex", "p1", 30);
        graph.add_pair(&b, &c, "dex", "p2", 30);
        graph.add_pair(&c, &d, "dex", "p3", 30);
        graph.add_pair(&d, &e, "dex", "p4", 30);

        // max_hops=3 should NOT find A->B->C->D->E (4 hops)
        let paths = graph.find_paths(&a, &e, 3, 100);
        assert!(paths.is_empty());

        // max_hops=4 should find it
        let paths = graph.find_paths(&a, &e, 4, 100);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].hops, 4);
    }
}
