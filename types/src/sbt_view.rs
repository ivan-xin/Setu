//! SBTView - 应用层聚合视图
//! 
//! 设计理念：
//! - SBTView是应用层的概念，不是链上对象
//! - 聚合SBT + 其拥有的所有资源（Coins、RelationGraphs）
//! - 提供便捷的查询接口和计算字段

use serde::{Deserialize, Serialize};
use crate::{
    object::{ObjectId, Address},
    sbt::SBT,
    coin::Coin,
    relation::RelationGraph,
};

/// SBT聚合视图 - 代表一个完整的数字身份及其资源
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SBTView {
    /// SBT对象（身份）
    pub sbt: SBT,
    
    /// 拥有的所有Coin对象
    pub coins: Vec<Coin>,
    
    /// 拥有的所有RelationGraph对象
    pub graphs: Vec<RelationGraph>,
    
    // ========== 计算字段 ==========
    
    /// 总余额（所有Coin的sum）
    pub total_balance: u64,
    
    /// Coin数量
    pub coin_count: usize,
    
    /// 关系图数量
    pub graph_count: usize,
    
    /// 总关系数
    pub total_relations: usize,
}

impl SBTView {
    /// 从SBT和资源构建视图
    pub fn new(sbt: SBT, coins: Vec<Coin>, graphs: Vec<RelationGraph>) -> Self {
        let total_balance = coins.iter().map(|c| c.value()).sum();
        let coin_count = coins.len();
        let graph_count = graphs.len();
        let total_relations = graphs.iter()
            .map(|g| g.data.relation_count())
            .sum();
        
        Self {
            sbt,
            coins,
            graphs,
            total_balance,
            coin_count,
            graph_count,
            total_relations,
        }
    }
    
    /// 获取SBT的ID
    pub fn sbt_id(&self) -> &ObjectId {
        &self.sbt.metadata.id
    }
    
    /// 获取Address
    pub fn address(&self) -> &Address {
        &self.sbt.data.owner
    }
    
    /// 获取显示名称
    pub fn display_name(&self) -> Option<&String> {
        self.sbt.data.display_name.as_ref()
    }
    
    /// 获取信誉分数
    pub fn reputation(&self) -> u64 {
        self.sbt.data.reputation_score
    }
    
    /// 获取指定类型的关系图
    pub fn get_graph_by_type(&self, graph_type: &str) -> Option<&RelationGraph> {
        self.graphs
            .iter()
            .find(|g| g.data.graph_type == graph_type)
    }
    
    /// 获取指定类型的所有关系图
    pub fn get_graphs_by_type(&self, graph_type: &str) -> Vec<&RelationGraph> {
        self.graphs
            .iter()
            .filter(|g| g.data.graph_type == graph_type)
            .collect()
    }
    
    /// 获取社交关系图
    pub fn social_graph(&self) -> Option<&RelationGraph> {
        self.get_graph_by_type("social")
    }
    
    /// 获取专业关系图
    pub fn professional_graph(&self) -> Option<&RelationGraph> {
        self.get_graph_by_type("professional")
    }
    
    /// 按关系类型统计关系数量
    pub fn count_relations_by_type(&self, relation_type: &str) -> usize {
        self.graphs
            .iter()
            .map(|g| g.data.relation_count_by_type(relation_type))
            .sum()
    }
    
    /// 获取所有关注的SBT IDs
    pub fn following_sbts(&self) -> Vec<ObjectId> {
        self.graphs
            .iter()
            .flat_map(|g| g.data.get_relations_by_type("follows"))
            .map(|r| r.target_sbt.clone())
            .collect()
    }
    
    /// 检查是否有足够余额
    pub fn has_balance(&self, amount: u64) -> bool {
        self.total_balance >= amount
    }
    
    /// 获取可用于支付的Coin列表（按余额排序）
    pub fn get_payable_coins(&self, amount: u64) -> Option<Vec<&Coin>> {
        let mut sorted_coins: Vec<&Coin> = self.coins.iter().collect();
        sorted_coins.sort_by(|a, b| b.value().cmp(&a.value()));
        
        let mut total = 0;
        let mut selected = Vec::new();
        
        for coin in sorted_coins {
            if total >= amount {
                break;
            }
            selected.push(coin);
            total += coin.value();
        }
        
        if total >= amount {
            Some(selected)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sbt::create_personal_sbt, coin::create_coin, relation::create_social_graph};
    
    #[test]
    fn test_sbt_view_creation() {
        let sbt = create_personal_sbt("alice".to_string());
        let sbt_id = sbt.metadata.id.clone();
        
        let coins = vec![
            create_coin(sbt_id.clone(), 1000),
            create_coin(sbt_id.clone(), 500),
        ];
        
        let graphs = vec![
            create_social_graph(sbt_id.clone()),
        ];
        
        let view = SBTView::new(sbt, coins, graphs);
        
        assert_eq!(view.total_balance, 1500);
        assert_eq!(view.coin_count, 2);
        assert_eq!(view.graph_count, 1);
        assert_eq!(view.address(), &"alice".to_string());
    }
    
    #[test]
    fn test_has_balance() {
        let sbt = create_personal_sbt("alice".to_string());
        let sbt_id = sbt.metadata.id.clone();
        
        let coins = vec![
            create_coin(sbt_id.clone(), 1000),
        ];
        
        let view = SBTView::new(sbt, coins, vec![]);
        
        assert!(view.has_balance(500));
        assert!(view.has_balance(1000));
        assert!(!view.has_balance(1001));
    }
    
    #[test]
    fn test_get_payable_coins() {
        let sbt = create_personal_sbt("alice".to_string());
        let sbt_id = sbt.metadata.id.clone();
        
        let coins = vec![
            create_coin(sbt_id.clone(), 1000),
            create_coin(sbt_id.clone(), 500),
            create_coin(sbt_id.clone(), 300),
        ];
        
        let view = SBTView::new(sbt, coins, vec![]);
        
        // 需要800，应该返回[1000]
        let payable = view.get_payable_coins(800);
        assert!(payable.is_some());
        let coins = payable.unwrap();
        assert_eq!(coins.len(), 1);
        assert_eq!(coins[0].value(), 1000);
        
        // 需要1200，应该返回[1000, 500]
        let payable = view.get_payable_coins(1200);
        assert!(payable.is_some());
        let coins = payable.unwrap();
        assert_eq!(coins.len(), 2);
        
        // 需要2000，余额不足
        let payable = view.get_payable_coins(2000);
        assert!(payable.is_none());
    }
    
    #[test]
    fn test_get_graphs() {
        let sbt = create_personal_sbt("alice".to_string());
        let sbt_id = sbt.metadata.id.clone();
        
        let mut social_graph = create_social_graph(sbt_id.clone());
        social_graph.data.add_relation("sbt_bob".to_string(), "follows".to_string(), 100);
        
        let graphs = vec![social_graph];
        let view = SBTView::new(sbt, vec![], graphs);
        
        assert_eq!(view.total_relations, 1);
        assert!(view.social_graph().is_some());
        assert_eq!(view.count_relations_by_type("follows"), 1);
    }
}
