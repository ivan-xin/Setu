use setu_types::{SetuError, SetuResult};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct Account {
    pub address: String,
    pub balance: u64,
    pub nonce: u64,
}

impl Account {
    pub fn new(address: String) -> Self {
        Self {
            address,
            balance: 0,
            nonce: 0,
        }
    }

    pub fn with_balance(address: String, balance: u64) -> Self {
        Self {
            address,
            balance,
            nonce: 0,
        }
    }
}

#[derive(Debug)]
pub struct StateStore {
    accounts: Arc<RwLock<HashMap<String, Account>>>,
    storage: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    version: Arc<RwLock<u64>>,
}

impl StateStore {
    pub fn new() -> Self {
        Self {
            accounts: Arc::new(RwLock::new(HashMap::new())),
            storage: Arc::new(RwLock::new(HashMap::new())),
            version: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn get_account(&self, address: &str) -> Option<Account> {
        let accounts = self.accounts.read().await;
        accounts.get(address).cloned()
    }

    pub async fn get_or_create_account(&self, address: &str) -> Account {
        let mut accounts = self.accounts.write().await;
        accounts
            .entry(address.to_string())
            .or_insert_with(|| Account::new(address.to_string()))
            .clone()
    }

    pub async fn update_account(&self, account: Account) {
        let mut accounts = self.accounts.write().await;
        accounts.insert(account.address.clone(), account);
    }

    pub async fn transfer(
        &self,
        from: &str,
        to: &str,
        amount: u64,
    ) -> SetuResult<()> {
        let mut accounts = self.accounts.write().await;

        let from_account = accounts
            .entry(from.to_string())
            .or_insert_with(|| Account::new(from.to_string()));

        if from_account.balance < amount {
            return Err(SetuError::InvalidTransfer("Insufficient balance".to_string()));
        }

        from_account.balance -= amount;
        from_account.nonce += 1;

        let to_account = accounts
            .entry(to.to_string())
            .or_insert_with(|| Account::new(to.to_string()));

        to_account.balance += amount;

        let mut version = self.version.write().await;
        *version += 1;

        Ok(())
    }

    pub async fn get_balance(&self, address: &str) -> u64 {
        let accounts = self.accounts.read().await;
        accounts
            .get(address)
            .map(|a| a.balance)
            .unwrap_or(0)
    }

    pub async fn set_storage(&self, key: String, value: Vec<u8>) {
        let mut storage = self.storage.write().await;
        storage.insert(key, value);
        
        let mut version = self.version.write().await;
        *version += 1;
    }

    pub async fn get_storage(&self, key: &str) -> Option<Vec<u8>> {
        let storage = self.storage.read().await;
        storage.get(key).cloned()
    }

    pub async fn delete_storage(&self, key: &str) {
        let mut storage = self.storage.write().await;
        storage.remove(key);
        
        let mut version = self.version.write().await;
        *version += 1;
    }

    pub async fn compute_state_root(&self) -> String {
        let accounts = self.accounts.read().await;
        let storage = self.storage.read().await;

        let mut hasher = Sha256::new();

        let mut account_keys: Vec<_> = accounts.keys().collect();
        account_keys.sort();
        for key in account_keys {
            if let Some(account) = accounts.get(key) {
                hasher.update(account.address.as_bytes());
                hasher.update(account.balance.to_le_bytes());
                hasher.update(account.nonce.to_le_bytes());
            }
        }

        let mut storage_keys: Vec<_> = storage.keys().collect();
        storage_keys.sort();
        for key in storage_keys {
            if let Some(value) = storage.get(key) {
                hasher.update(key.as_bytes());
                hasher.update(value);
            }
        }

        hex::encode(hasher.finalize())
    }

    pub async fn version(&self) -> u64 {
        *self.version.read().await
    }

    pub async fn account_count(&self) -> usize {
        self.accounts.read().await.len()
    }
}

impl Clone for StateStore {
    fn clone(&self) -> Self {
        Self {
            accounts: Arc::clone(&self.accounts),
            storage: Arc::clone(&self.storage),
            version: Arc::clone(&self.version),
        }
    }
}

impl Default for StateStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_transfer() {
        let store = StateStore::new();
        
        store.update_account(Account::with_balance("alice".to_string(), 1000)).await;
        store.update_account(Account::with_balance("bob".to_string(), 500)).await;

        store.transfer("alice", "bob", 100).await.unwrap();

        assert_eq!(store.get_balance("alice").await, 900);
        assert_eq!(store.get_balance("bob").await, 600);
    }

    #[tokio::test]
    async fn test_insufficient_balance() {
        let store = StateStore::new();
        store.update_account(Account::with_balance("alice".to_string(), 50)).await;

        let result = store.transfer("alice", "bob", 100).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_state_root() {
        let store = StateStore::new();
        store.update_account(Account::with_balance("alice".to_string(), 1000)).await;
        
        let root1 = store.compute_state_root().await;
        
        store.transfer("alice", "bob", 100).await.unwrap();
        
        let root2 = store.compute_state_root().await;
        
        assert_ne!(root1, root2);
    }
}
