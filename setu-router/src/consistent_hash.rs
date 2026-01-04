//! Consistent Hash Strategy for deterministic routing
//!
//! Borrowed from colleague's implementation with enhancements.
//! 
//! Benefits:
//! - Same resource always routes to the same solver (cache-friendly)
//! - Minimal redistribution when solvers are added/removed
//! - Virtual nodes ensure even distribution

use blake3::Hasher;
use parking_lot::RwLock;
use std::collections::BTreeMap;
use tracing::{debug, trace};

/// Consistent hash strategy with cached hash ring
///
/// Ensures transactions with the same resources are routed to the same solver,
/// which helps with caching and reduces cross-solver coordination.
pub struct ConsistentHashStrategy {
    /// Number of virtual nodes per solver for better distribution
    virtual_nodes: u32,
    /// Cached hash ring: (solvers_hash, ring)
    /// The ring maps hash values to solver IDs
    ring_cache: RwLock<Option<(u64, BTreeMap<u64, String>)>>,
}

impl ConsistentHashStrategy {
    /// Create a new consistent hash strategy with default 150 virtual nodes
    pub fn new() -> Self {
        Self::with_virtual_nodes(150)
    }

    /// Create with custom virtual node count
    pub fn with_virtual_nodes(virtual_nodes: u32) -> Self {
        debug!(
            virtual_nodes = virtual_nodes,
            "Creating ConsistentHashStrategy"
        );
        Self {
            virtual_nodes,
            ring_cache: RwLock::new(None),
        }
    }

    /// Hash a string key using blake3
    fn hash_key(key: &str) -> u64 {
        let mut hasher = Hasher::new();
        hasher.update(key.as_bytes());
        let hash = hasher.finalize();
        let bytes = hash.as_bytes();
        u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
        ])
    }

    /// Compute a hash of the solver list for cache invalidation
    fn solvers_hash(solver_ids: &[String]) -> u64 {
        let mut hasher = Hasher::new();
        for id in solver_ids {
            hasher.update(id.as_bytes());
        }
        let hash = hasher.finalize();
        let bytes = hash.as_bytes();
        u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
        ])
    }

    /// Build the hash ring from solver IDs
    fn build_ring(&self, solver_ids: &[String]) -> BTreeMap<u64, String> {
        let mut ring = BTreeMap::new();
        
        for solver_id in solver_ids {
            for vn in 0..self.virtual_nodes {
                let key = format!("{}:{}", solver_id, vn);
                let hash = Self::hash_key(&key);
                ring.insert(hash, solver_id.clone());
            }
        }
        
        debug!(
            solver_count = solver_ids.len(),
            ring_size = ring.len(),
            "Built hash ring"
        );
        
        ring
    }

    /// Get or build the hash ring, using cache if available
    fn get_or_build_ring(&self, solver_ids: &[String]) -> BTreeMap<u64, String> {
        let current_hash = Self::solvers_hash(solver_ids);

        // Check if cache is valid
        {
            let cache = self.ring_cache.read();
            if let Some((cached_hash, ring)) = cache.as_ref() {
                if *cached_hash == current_hash {
                    trace!("Using cached hash ring");
                    return ring.clone();
                }
            }
        }

        // Build new ring
        let ring = self.build_ring(solver_ids);

        // Cache the new ring
        *self.ring_cache.write() = Some((current_hash, ring.clone()));
        
        ring
    }

    /// Find solver in the ring for a given hash
    fn find_in_ring(ring: &BTreeMap<u64, String>, hash: u64) -> Option<String> {
        if ring.is_empty() {
            return None;
        }
        
        // Find the first node >= hash, or wrap around to first
        ring.range(hash..)
            .next()
            .or_else(|| ring.iter().next())
            .map(|(_, id)| id.clone())
    }

    /// Select a solver based on routing key
    /// 
    /// # Arguments
    /// * `solver_ids` - List of available solver IDs
    /// * `routing_key` - Key to hash for routing (e.g., resource ID, account)
    /// 
    /// # Returns
    /// Selected solver ID, or None if no solvers available
    pub fn select(&self, solver_ids: &[String], routing_key: &str) -> Option<String> {
        if solver_ids.is_empty() {
            return None;
        }

        if solver_ids.len() == 1 {
            return Some(solver_ids[0].clone());
        }

        let ring = self.get_or_build_ring(solver_ids);
        let hash = Self::hash_key(routing_key);
        
        trace!(
            routing_key = %routing_key,
            hash = %hash,
            "Consistent hash lookup"
        );

        Self::find_in_ring(&ring, hash)
    }

    /// Invalidate the cache (call when solvers change)
    pub fn invalidate_cache(&self) {
        *self.ring_cache.write() = None;
        debug!("Hash ring cache invalidated");
    }

    /// Get the number of virtual nodes
    pub fn virtual_nodes(&self) -> u32 {
        self.virtual_nodes
    }
}

impl Default for ConsistentHashStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ConsistentHashStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsistentHashStrategy")
            .field("virtual_nodes", &self.virtual_nodes)
            .field("cache_valid", &self.ring_cache.read().is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_solvers(count: usize) -> Vec<String> {
        (1..=count)
            .map(|i| format!("solver-{}", i))
            .collect()
    }

    #[test]
    fn test_consistent_hash_deterministic() {
        let strategy = ConsistentHashStrategy::default();
        let solvers = create_test_solvers(6);

        let result1 = strategy.select(&solvers, "account:alice").unwrap();
        let result2 = strategy.select(&solvers, "account:alice").unwrap();

        assert_eq!(result1, result2, "Same key should route to same solver");
    }

    #[test]
    fn test_consistent_hash_distribution() {
        let strategy = ConsistentHashStrategy::default();
        let solvers = create_test_solvers(6);

        let mut distribution = std::collections::HashMap::new();

        for i in 0..1000 {
            let key = format!("resource:{}", i);
            let result = strategy.select(&solvers, &key).unwrap();
            *distribution.entry(result).or_insert(0) += 1;
        }

        // All 6 solvers should receive traffic
        assert_eq!(distribution.len(), 6);

        // Check reasonable distribution (each solver gets roughly 10-30%)
        for count in distribution.values() {
            assert!(
                *count > 50 && *count < 300,
                "count={} is outside expected range",
                count
            );
        }
    }

    #[test]
    fn test_consistent_hash_cache() {
        let strategy = ConsistentHashStrategy::default();
        let solvers = create_test_solvers(6);

        // First call builds cache
        let _ = strategy.select(&solvers, "key1").unwrap();
        
        // Verify cache exists
        assert!(strategy.ring_cache.read().is_some());
        
        // Second call should use cache
        let _ = strategy.select(&solvers, "key2").unwrap();

        // Invalidate and verify
        strategy.invalidate_cache();
        assert!(strategy.ring_cache.read().is_none());
    }

    #[test]
    fn test_empty_solvers() {
        let strategy = ConsistentHashStrategy::default();
        let result = strategy.select(&[], "key");
        assert!(result.is_none());
    }

    #[test]
    fn test_single_solver() {
        let strategy = ConsistentHashStrategy::default();
        let solvers = create_test_solvers(1);

        let result = strategy.select(&solvers, "any_key").unwrap();
        assert_eq!(result, "solver-1");
    }

    #[test]
    fn test_different_keys_may_route_differently() {
        let strategy = ConsistentHashStrategy::default();
        let solvers = create_test_solvers(6);

        let result1 = strategy.select(&solvers, "account:alice").unwrap();
        let result2 = strategy.select(&solvers, "account:bob").unwrap();

        // Different keys may (but not necessarily) route to different solvers
        // Just verify both routes succeed
        assert!(!result1.is_empty());
        assert!(!result2.is_empty());
    }

    #[test]
    fn test_minimal_redistribution() {
        let strategy = ConsistentHashStrategy::default();
        
        // Start with 5 solvers
        let solvers5 = create_test_solvers(5);
        let mut routes_5: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        
        for i in 0..100 {
            let key = format!("key:{}", i);
            let solver = strategy.select(&solvers5, &key).unwrap();
            routes_5.insert(key, solver);
        }
        
        // Add one more solver
        let solvers6 = create_test_solvers(6);
        strategy.invalidate_cache();
        
        let mut changed = 0;
        for i in 0..100 {
            let key = format!("key:{}", i);
            let solver = strategy.select(&solvers6, &key).unwrap();
            if routes_5.get(&key) != Some(&solver) {
                changed += 1;
            }
        }
        
        // With consistent hashing, only ~1/6 of keys should be redistributed
        // Allow some variance: expect less than 40% change
        assert!(
            changed < 40,
            "Too many keys redistributed: {} (expected < 40)",
            changed
        );
    }
}

