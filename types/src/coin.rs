//! Coin对象 - 可转移的资产
//! 
//! 设计理念：
//! - Coin是独立的对象，可以被SBT拥有
//! - 支持split、merge、transfer等操作
//! - Balance是值类型，不是对象

use serde::{Deserialize, Serialize};
use crate::object::{Object, ObjectId, Address, generate_object_id};

/// Balance是值类型，封装代币数量
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Balance {
    value: u64,
}

impl Balance {
    /// 创建新的Balance
    pub fn new(value: u64) -> Self {
        Self { value }
    }
    
    /// 获取余额值
    pub fn value(&self) -> u64 {
        self.value
    }
    
    /// 提取指定数量，返回提取的Balance
    pub fn withdraw(&mut self, amount: u64) -> Result<Balance, String> {
        if self.value < amount {
            return Err(format!(
                "Insufficient balance: have {}, need {}",
                self.value, amount
            ));
        }
        self.value -= amount;
        Ok(Balance::new(amount))
    }
    
    /// 存入Balance
    pub fn deposit(&mut self, balance: Balance) -> Result<(), String> {
        self.value = self.value.checked_add(balance.value)
            .ok_or("Balance overflow")?;
        Ok(())
    }
    
    /// 销毁Balance（用于merge时）
    pub fn destroy(self) -> u64 {
        self.value
    }
}

/// Coin对象数据 - 代表可转移的代币
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinData {
    pub balance: Balance,
}

/// Coin类型别名
pub type Coin = Object<CoinData>;

impl Coin {
    /// 创建新的Coin对象
    /// 
    /// # 参数
    /// - `owner`: Coin的拥有者（通常是SBT的ObjectId）
    /// - `value`: 初始余额
    pub fn new(owner: Address, value: u64) -> Self {
        let id = generate_object_id(format!("coin:{}:{}", owner, value).as_bytes());
        let data = CoinData {
            balance: Balance::new(value),
        };
        Object::new_owned(id, &owner, data)
    }
    
    /// 获取余额
    pub fn value(&self) -> u64 {
        self.data.balance.value()
    }
    
    /// 拆分出指定数量到新Coin
    /// 
    /// # 参数
    /// - `amount`: 要拆分的数量
    /// - `new_owner`: 新Coin的拥有者
    /// 
    /// # 返回
    /// 返回新创建的Coin对象
    pub fn split(&mut self, amount: u64, new_owner: Address) -> Result<Coin, String> {
        let withdrawn = self.data.balance.withdraw(amount)?;
        let new_coin = Coin::new(new_owner, withdrawn.value());
        self.increment_version();
        Ok(new_coin)
    }
    
    /// 合并另一个Coin到当前Coin
    /// 
    /// # 参数
    /// - `other`: 要合并的Coin对象
    pub fn merge(&mut self, other: Coin) -> Result<(), String> {
        self.data.balance.deposit(other.data.balance)?;
        self.increment_version();
        Ok(())
    }
    
    /// 转移Coin的所有权
    /// 
    /// # 参数
    /// - `new_owner`: 新的拥有者
    pub fn transfer(&mut self, new_owner: Address) {
        self.metadata.owner = Some(new_owner);
        self.increment_version();
    }
}

/// 辅助函数：创建Coin
pub fn create_coin(owner: Address, value: u64) -> Coin {
    Coin::new(owner, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_balance_operations() {
        let mut balance = Balance::new(1000);
        
        // Withdraw
        let withdrawn = balance.withdraw(300).unwrap();
        assert_eq!(balance.value(), 700);
        assert_eq!(withdrawn.value(), 300);
        
        // Deposit
        balance.deposit(withdrawn).unwrap();
        assert_eq!(balance.value(), 1000);
        
        // Insufficient balance
        assert!(balance.withdraw(2000).is_err());
    }
    
    #[test]
    fn test_balance_overflow() {
        let mut balance = Balance::new(u64::MAX - 100);
        let to_add = Balance::new(200);
        assert!(balance.deposit(to_add).is_err());
    }
    
    #[test]
    fn test_coin_creation() {
        let owner = Address::from("sbt_alice");
        let coin = Coin::new(owner.clone(), 1000);
        
        assert_eq!(coin.value(), 1000);
        assert_eq!(coin.metadata.owner.as_ref().unwrap(), &owner);
        assert_eq!(coin.metadata.version, 1); // 初始版本是1
    }
    
    #[test]
    fn test_coin_split() {
        let owner = Address::from("sbt_alice");
        let mut coin = Coin::new(owner.clone(), 1000);
        
        let new_owner = Address::from("sbt_bob");
        let new_coin = coin.split(300, new_owner.clone()).unwrap();
        
        assert_eq!(coin.value(), 700);
        assert_eq!(new_coin.value(), 300);
        assert_eq!(coin.metadata.version, 2); // 操作后版本+1
        assert_eq!(new_coin.metadata.version, 1); // 新创建的对象版本为1
        assert_eq!(new_coin.metadata.owner.as_ref().unwrap(), &new_owner);
    }
    
    #[test]
    fn test_coin_split_insufficient() {
        let owner = Address::from("sbt_alice");
        let mut coin = Coin::new(owner.clone(), 100);
        
        let result = coin.split(200, Address::from("sbt_bob"));
        assert!(result.is_err());
    }
    
    #[test]
    fn test_coin_merge() {
        let owner = Address::from("sbt_alice");
        let mut coin1 = Coin::new(owner.clone(), 1000);
        let coin2 = Coin::new(owner.clone(), 500);
        
        coin1.merge(coin2).unwrap();
        
        assert_eq!(coin1.value(), 1500);
        assert_eq!(coin1.metadata.version, 2); // 操作后版本+1
    }
    
    #[test]
    fn test_coin_transfer() {
        let owner = Address::from("sbt_alice");
        let mut coin = Coin::new(owner.clone(), 1000);
        
        let new_owner = Address::from("sbt_bob");
        coin.transfer(new_owner.clone());
        
        assert_eq!(coin.metadata.owner.as_ref().unwrap(), &new_owner);
        assert_eq!(coin.metadata.version, 2); // 操作后版本+1
    }
}
