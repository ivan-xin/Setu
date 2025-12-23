//! RelationGraph对象 - 社交关系图
//! 
//! 设计理念：
//! - RelationGraph是SBT拥有的资源对象
//! - 一个SBT可以有多个RelationGraph（朋友圈、工作圈等）
//! - RelationGraph存储到其他SBT的关系

use serde::{Deserialize, Serialize};
use crate::object::{Object, ObjectId, Address, generate_object_id};

/// 关系边
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Relation {
    /// 目标SBT的ID
    pub target_sbt: ObjectId,
    
    /// 关系类型
    pub relation_type: String,
    
    /// 关系权重（用于算法）
    pub weight: u32,
    
    /// 创建时间
    pub created_at: u64,
    
    /// 元数据
    pub metadata: std::collections::HashMap<String, String>,
}

/// 关系图数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationGraphData {
    /// 拥有者（SBT的ID）
    pub owner_sbt: ObjectId,
    
    /// 图的类型/名称
    pub graph_type: String,
    
    /// 关系列表
    pub relations: Vec<Relation>,
    
    /// 创建时间
    pub created_at: u64,
    
    /// 更新时间
    pub updated_at: u64,
}

/// RelationGraph类型别名
pub type RelationGraph = Object<RelationGraphData>;

impl RelationGraphData {
    /// 创建新的关系图
    pub fn new(owner_sbt: ObjectId, graph_type: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        Self {
            owner_sbt,
            graph_type,
            relations: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }
    
    /// 添加关系
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
    
    /// 移除关系
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
    
    /// 获取指定类型的所有关系
    pub fn get_relations_by_type(&self, relation_type: &str) -> Vec<&Relation> {
        self.relations
            .iter()
            .filter(|r| r.relation_type == relation_type)
            .collect()
    }
    
    /// 获取到指定目标的关系
    pub fn get_relation(&self, target_sbt: &ObjectId, relation_type: &str) -> Option<&Relation> {
        self.relations
            .iter()
            .find(|r| r.target_sbt == *target_sbt && r.relation_type == relation_type)
    }
    
    /// 更新关系权重
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
    
    /// 获取关系数量
    pub fn relation_count(&self) -> usize {
        self.relations.len()
    }
    
    /// 获取指定类型的关系数量
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
    /// 创建新的关系图对象
    pub fn new(owner_sbt: ObjectId, graph_type: String) -> Self {
        let id = generate_object_id(
            format!("graph:{}:{}", owner_sbt, graph_type).as_bytes()
        );
        let data = RelationGraphData::new(owner_sbt.clone(), graph_type);
        
        // RelationGraph的owner是SBT的ID（以字符串形式）
        Object::new_owned(id, &owner_sbt, data)
    }
}

/// 辅助函数：创建社交关系图
pub fn create_social_graph(owner_sbt: ObjectId) -> RelationGraph {
    RelationGraph::new(owner_sbt, "social".to_string())
}

/// 辅助函数：创建专业关系图
pub fn create_professional_graph(owner_sbt: ObjectId) -> RelationGraph {
    RelationGraph::new(owner_sbt, "professional".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_relation_graph() {
        let owner_sbt = "sbt_alice".to_string();
        let graph = create_social_graph(owner_sbt.clone());
        
        assert_eq!(graph.data.owner_sbt, owner_sbt);
        assert_eq!(graph.data.graph_type, "social");
        assert_eq!(graph.data.relation_count(), 0);
    }
    
    #[test]
    fn test_add_relation() {
        let mut data = RelationGraphData::new("sbt_alice".to_string(), "social".to_string());
        
        data.add_relation("sbt_bob".to_string(), "follows".to_string(), 100);
        data.add_relation("sbt_charlie".to_string(), "trusts".to_string(), 80);
        
        assert_eq!(data.relation_count(), 2);
        assert_eq!(data.relation_count_by_type("follows"), 1);
        assert_eq!(data.relation_count_by_type("trusts"), 1);
    }
    
    #[test]
    fn test_remove_relation() {
        let mut data = RelationGraphData::new("sbt_alice".to_string(), "social".to_string());
        
        data.add_relation("sbt_bob".to_string(), "follows".to_string(), 100);
        data.add_relation("sbt_charlie".to_string(), "trusts".to_string(), 80);
        
        let removed = data.remove_relation(&"sbt_bob".to_string(), "follows");
        assert!(removed);
        assert_eq!(data.relation_count(), 1);
        
        let not_removed = data.remove_relation(&"sbt_bob".to_string(), "follows");
        assert!(!not_removed);
    }
    
    #[test]
    fn test_get_relations_by_type() {
        let mut data = RelationGraphData::new("sbt_alice".to_string(), "social".to_string());
        
        data.add_relation("sbt_bob".to_string(), "follows".to_string(), 100);
        data.add_relation("sbt_charlie".to_string(), "follows".to_string(), 90);
        data.add_relation("sbt_dave".to_string(), "trusts".to_string(), 80);
        
        let follows = data.get_relations_by_type("follows");
        assert_eq!(follows.len(), 2);
        
        let trusts = data.get_relations_by_type("trusts");
        assert_eq!(trusts.len(), 1);
    }
    
    #[test]
    fn test_update_weight() {
        let mut data = RelationGraphData::new("sbt_alice".to_string(), "social".to_string());
        
        data.add_relation("sbt_bob".to_string(), "follows".to_string(), 100);
        
        let updated = data.update_weight(&"sbt_bob".to_string(), "follows", 150);
        assert!(updated);
        
        let relation = data.get_relation(&"sbt_bob".to_string(), "follows").unwrap();
        assert_eq!(relation.weight, 150);
    }
}
