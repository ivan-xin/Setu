//! RelationGraph Object - Social Relationship Graph
//! 
//! Design Philosophy:
//! - RelationGraph is a resource object owned by SBT
//! - One SBT can have multiple RelationGraphs (friend circle, work circle, etc.)
//! - RelationGraph stores relationships to other SBTs

use serde::{Deserialize, Serialize};
use crate::object::{Object, ObjectId, Address, generate_object_id};

/// Relationship edge
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Relation {
    /// Target SBT's ID
    pub target_sbt: ObjectId,
    
    /// Relationship type
    pub relation_type: String,
    
    /// Relationship weight (used for algorithms)
    pub weight: u32,
    
    /// Creation time
    pub created_at: u64,
    
    /// Metadata
    pub metadata: std::collections::HashMap<String, String>,
}

/// Relationship graph data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationGraphData {
    /// Owner (SBT's ID)
    pub owner_sbt: ObjectId,
    
    /// Owner address (for convenience)
    pub owner_address: Address,
    
    /// Graph type/name
    pub graph_type: String,
    
    /// Relationship list
    pub relations: Vec<Relation>,
    
    /// Creation time
    pub created_at: u64,
    
    /// Update time
    pub updated_at: u64,
}

/// RelationGraph type alias
pub type RelationGraph = Object<RelationGraphData>;

impl RelationGraphData {
    /// Create a new relationship graph
    pub fn new(owner_sbt: ObjectId, owner_address: Address, graph_type: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        Self {
            owner_sbt,
            owner_address,
            graph_type,
            relations: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }
    
    /// Add relationship
    pub fn add_relation(&mut self, target_sbt: ObjectId, relation_type: String, weight: u32) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let relation = Relation {
            target_sbt,
            relation_type,
            weight,
            created_at: now,
            metadata: std::collections::HashMap::new(),
        };
        
        self.relations.push(relation);
        self.touch();
    }
    
    /// Remove relationship
    pub fn remove_relation(&mut self, target_sbt: &ObjectId, relation_type: &str) -> bool {
        let initial_len = self.relations.len();
        self.relations.retain(|r| {
            !(r.target_sbt == *target_sbt && r.relation_type == relation_type)
        });
        
        if self.relations.len() < initial_len {
            self.touch();
            true
        } else {
            false
        }
    }
    
    /// Get all relationships of specified type
    pub fn get_relations_by_type(&self, relation_type: &str) -> Vec<&Relation> {
        self.relations
            .iter()
            .filter(|r| r.relation_type == relation_type)
            .collect()
    }
    
    /// Get relationship to specified target
    pub fn get_relation(&self, target_sbt: &ObjectId, relation_type: &str) -> Option<&Relation> {
        self.relations
            .iter()
            .find(|r| r.target_sbt == *target_sbt && r.relation_type == relation_type)
    }
    
    /// Update relationship weight
    pub fn update_weight(&mut self, target_sbt: &ObjectId, relation_type: &str, weight: u32) -> bool {
        if let Some(relation) = self.relations
            .iter_mut()
            .find(|r| r.target_sbt == *target_sbt && r.relation_type == relation_type) 
        {
            relation.weight = weight;
            self.touch();
            true
        } else {
            false
        }
    }
    
    /// Get relationship count
    pub fn relation_count(&self) -> usize {
        self.relations.len()
    }
    
    /// Get relationship count by type
    pub fn relation_count_by_type(&self, relation_type: &str) -> usize {
        self.relations
            .iter()
            .filter(|r| r.relation_type == relation_type)
            .count()
    }
    
    fn touch(&mut self) {
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
    }
}

impl RelationGraph {
    /// Create a new relationship graph object
    pub fn new(owner_sbt: ObjectId, owner_address: Address, graph_type: String) -> Self {
        let id = generate_object_id(
            format!("graph:{}:{}", owner_sbt, graph_type).as_bytes()
        );
        let data = RelationGraphData::new(owner_sbt, owner_address, graph_type);
        
        Object::new_owned(id, owner_address, data)
    }
}

/// Helper function: create social relationship graph
pub fn create_social_graph(owner_sbt: ObjectId, owner_address: Address) -> RelationGraph {
    RelationGraph::new(owner_sbt, owner_address, "social".to_string())
}

/// Helper function: create professional relationship graph
pub fn create_professional_graph(owner_sbt: ObjectId, owner_address: Address) -> RelationGraph {
    RelationGraph::new(owner_sbt, owner_address, "professional".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_relation_graph() {
        let owner_sbt = generate_object_id(b"sbt_alice");
        let owner_address = Address::from_str_id("alice");
        let graph = create_social_graph(owner_sbt, owner_address);
        
        assert_eq!(graph.data.owner_sbt, owner_sbt);
        assert_eq!(graph.data.graph_type, "social");
        assert_eq!(graph.data.relation_count(), 0);
    }
    
    #[test]
    fn test_add_relation() {
        let owner_sbt = generate_object_id(b"sbt_alice");
        let owner_address = Address::from_str_id("alice");
        let mut data = RelationGraphData::new(owner_sbt, owner_address, "social".to_string());
        
        let bob_sbt = generate_object_id(b"sbt_bob");
        let charlie_sbt = generate_object_id(b"sbt_charlie");
        
        data.add_relation(bob_sbt, "follows".to_string(), 100);
        data.add_relation(charlie_sbt, "trusts".to_string(), 80);
        
        assert_eq!(data.relation_count(), 2);
        assert_eq!(data.relation_count_by_type("follows"), 1);
        assert_eq!(data.relation_count_by_type("trusts"), 1);
    }
    
    #[test]
    fn test_remove_relation() {
        let owner_sbt = generate_object_id(b"sbt_alice");
        let owner_address = Address::from_str_id("alice");
        let mut data = RelationGraphData::new(owner_sbt, owner_address, "social".to_string());
        
        let bob_sbt = generate_object_id(b"sbt_bob");
        let charlie_sbt = generate_object_id(b"sbt_charlie");
        
        data.add_relation(bob_sbt, "follows".to_string(), 100);
        data.add_relation(charlie_sbt, "trusts".to_string(), 80);
        
        let removed = data.remove_relation(&bob_sbt, "follows");
        assert!(removed);
        assert_eq!(data.relation_count(), 1);
        
        let not_removed = data.remove_relation(&bob_sbt, "follows");
        assert!(!not_removed);
    }
    
    #[test]
    fn test_get_relations_by_type() {
        let owner_sbt = generate_object_id(b"sbt_alice");
        let owner_address = Address::from_str_id("alice");
        let mut data = RelationGraphData::new(owner_sbt, owner_address, "social".to_string());
        
        let bob_sbt = generate_object_id(b"sbt_bob");
        let charlie_sbt = generate_object_id(b"sbt_charlie");
        let dave_sbt = generate_object_id(b"sbt_dave");
        
        data.add_relation(bob_sbt, "follows".to_string(), 100);
        data.add_relation(charlie_sbt, "follows".to_string(), 90);
        data.add_relation(dave_sbt, "trusts".to_string(), 80);
        
        let follows = data.get_relations_by_type("follows");
        assert_eq!(follows.len(), 2);
        
        let trusts = data.get_relations_by_type("trusts");
        assert_eq!(trusts.len(), 1);
    }
    
    #[test]
    fn test_update_weight() {
        let owner_sbt = generate_object_id(b"sbt_alice");
        let owner_address = Address::from_str_id("alice");
        let mut data = RelationGraphData::new(owner_sbt, owner_address, "social".to_string());
        
        let bob_sbt = generate_object_id(b"sbt_bob");
        data.add_relation(bob_sbt, "follows".to_string(), 100);
        
        let updated = data.update_weight(&bob_sbt, "follows", 150);
        assert!(updated);
        
        let relation = data.get_relation(&bob_sbt, "follows").unwrap();
        assert_eq!(relation.weight, 150);
    }
}
