//! RocksDB implementation of ObjectStore

use crate::object_store::ObjectStore;
use crate::rocks::{SetuDB, ColumnFamily};
use setu_types::{
    ObjectId, Address,
    Coin, Profile, Credential, RelationGraph, AccountView,
    SetuResult, SetuError,
};

pub struct RocksObjectStore {
    db: SetuDB,
}

impl RocksObjectStore {
    pub fn new(db: SetuDB) -> Self {
        Self { db }
    }
    
    pub fn open(path: impl AsRef<std::path::Path>) -> SetuResult<Self> {
        let db = SetuDB::open_default(path)
            .map_err(|e| SetuError::StorageError(format!("Failed to open database: {}", e)))?;
        Ok(Self::new(db))
    }
    
    fn add_to_index(&self, cf: ColumnFamily, key: &Address, object_id: &ObjectId) -> SetuResult<()> {
        let mut ids: Vec<ObjectId> = self.db.get(cf, key)
            .map_err(|e| SetuError::StorageError(e.to_string()))?
            .unwrap_or_default();
        if !ids.contains(object_id) {
            ids.push(*object_id);
            self.db.put(cf, key, &ids).map_err(|e| SetuError::StorageError(e.to_string()))?;
        }
        Ok(())
    }
    
    fn remove_from_index(&self, cf: ColumnFamily, key: &Address, object_id: &ObjectId) -> SetuResult<()> {
        let mut ids: Vec<ObjectId> = self.db.get(cf, key)
            .map_err(|e| SetuError::StorageError(e.to_string()))?
            .unwrap_or_default();
        let original_len = ids.len();
        ids.retain(|id| id != object_id);
        if ids.len() < original_len {
            if ids.is_empty() {
                self.db.delete(cf, key).map_err(|e| SetuError::StorageError(e.to_string()))?;
            } else {
                self.db.put(cf, key, &ids).map_err(|e| SetuError::StorageError(e.to_string()))?;
            }
        }
        Ok(())
    }
    
    fn get_index(&self, cf: ColumnFamily, key: &Address) -> SetuResult<Vec<ObjectId>> {
        self.db.get(cf, key)
            .map_err(|e| SetuError::StorageError(e.to_string()))
            .map(|opt| opt.unwrap_or_default())
    }
}

impl ObjectStore for RocksObjectStore {
    fn store_coin(&self, coin: &Coin) -> SetuResult<ObjectId> {
        let id = coin.metadata.id;
        self.db.put(ColumnFamily::Coins, &id, coin).map_err(|e| SetuError::StorageError(e.to_string()))?;
        if let Some(owner) = &coin.metadata.owner {
            self.add_to_index(ColumnFamily::CoinsByOwner, owner, &id)?;
        }
        Ok(id)
    }
    
    fn get_coin(&self, id: &ObjectId) -> SetuResult<Option<Coin>> {
        self.db.get(ColumnFamily::Coins, id).map_err(|e| SetuError::StorageError(e.to_string()))
    }
    
    fn get_coins_by_owner(&self, owner: &Address) -> SetuResult<Vec<Coin>> {
        let ids = self.get_index(ColumnFamily::CoinsByOwner, owner)?;
        let coins: Vec<Option<Coin>> = self.db.multi_get(ColumnFamily::Coins, &ids)
            .map_err(|e| SetuError::StorageError(e.to_string()))?;
        Ok(coins.into_iter().flatten().collect())
    }
    
    fn update_coin(&self, coin: &Coin) -> SetuResult<()> {
        if let Some(old_coin) = self.get_coin(&coin.metadata.id)? {
            if old_coin.metadata.owner != coin.metadata.owner {
                if let Some(old_owner) = &old_coin.metadata.owner {
                    self.remove_from_index(ColumnFamily::CoinsByOwner, old_owner, &coin.metadata.id)?;
                }
                if let Some(new_owner) = &coin.metadata.owner {
                    self.add_to_index(ColumnFamily::CoinsByOwner, new_owner, &coin.metadata.id)?;
                }
            }
        }
        self.db.put(ColumnFamily::Coins, &coin.metadata.id, coin).map_err(|e| SetuError::StorageError(e.to_string()))
    }
    
    fn delete_coin(&self, id: &ObjectId) -> SetuResult<()> {
        if let Some(coin) = self.get_coin(id)? {
            if let Some(owner) = &coin.metadata.owner {
                self.remove_from_index(ColumnFamily::CoinsByOwner, owner, id)?;
            }
        }
        self.db.delete(ColumnFamily::Coins, id).map_err(|e| SetuError::StorageError(e.to_string()))
    }
    
    fn store_profile(&self, profile: &Profile) -> SetuResult<ObjectId> {
        let id = profile.metadata.id;
        self.db.put(ColumnFamily::Profiles, &id, profile).map_err(|e| SetuError::StorageError(e.to_string()))?;
        self.db.put(ColumnFamily::ProfileByAddress, &profile.data.owner, &id).map_err(|e| SetuError::StorageError(e.to_string()))?;
        Ok(id)
    }
    
    fn get_profile(&self, id: &ObjectId) -> SetuResult<Option<Profile>> {
        self.db.get(ColumnFamily::Profiles, id).map_err(|e| SetuError::StorageError(e.to_string()))
    }
    
    fn get_profile_by_address(&self, address: &Address) -> SetuResult<Option<Profile>> {
        let profile_id: Option<ObjectId> = self.db.get(ColumnFamily::ProfileByAddress, address)
            .map_err(|e| SetuError::StorageError(e.to_string()))?;
        match profile_id {
            Some(id) => self.get_profile(&id),
            None => Ok(None),
        }
    }
    
    fn update_profile(&self, profile: &Profile) -> SetuResult<()> {
        if let Some(old_profile) = self.get_profile(&profile.metadata.id)? {
            if old_profile.data.owner != profile.data.owner {
                self.db.delete(ColumnFamily::ProfileByAddress, &old_profile.data.owner)
                    .map_err(|e| SetuError::StorageError(e.to_string()))?;
            }
        }
        self.store_profile(profile)?;
        Ok(())
    }
    
    fn delete_profile(&self, id: &ObjectId) -> SetuResult<()> {
        if let Some(profile) = self.get_profile(id)? {
            self.db.delete(ColumnFamily::ProfileByAddress, &profile.data.owner)
                .map_err(|e| SetuError::StorageError(e.to_string()))?;
        }
        self.db.delete(ColumnFamily::Profiles, id).map_err(|e| SetuError::StorageError(e.to_string()))
    }
    
    fn store_credential(&self, credential: &Credential) -> SetuResult<ObjectId> {
        let id = credential.metadata.id;
        self.db.put(ColumnFamily::Credentials, &id, credential).map_err(|e| SetuError::StorageError(e.to_string()))?;
        self.add_to_index(ColumnFamily::CredentialsByHolder, &credential.data.holder, &id)?;
        self.add_to_index(ColumnFamily::CredentialsByIssuer, &credential.data.issuer, &id)?;
        Ok(id)
    }
    
    fn get_credential(&self, id: &ObjectId) -> SetuResult<Option<Credential>> {
        self.db.get(ColumnFamily::Credentials, id).map_err(|e| SetuError::StorageError(e.to_string()))
    }
    
    fn get_credentials_by_holder(&self, holder: &Address) -> SetuResult<Vec<Credential>> {
        let ids = self.get_index(ColumnFamily::CredentialsByHolder, holder)?;
        let creds: Vec<Option<Credential>> = self.db.multi_get(ColumnFamily::Credentials, &ids)
            .map_err(|e| SetuError::StorageError(e.to_string()))?;
        Ok(creds.into_iter().flatten().collect())
    }
    
    fn get_credentials_by_issuer(&self, issuer: &Address) -> SetuResult<Vec<Credential>> {
        let ids = self.get_index(ColumnFamily::CredentialsByIssuer, issuer)?;
        let creds: Vec<Option<Credential>> = self.db.multi_get(ColumnFamily::Credentials, &ids)
            .map_err(|e| SetuError::StorageError(e.to_string()))?;
        Ok(creds.into_iter().flatten().collect())
    }
    
    fn update_credential(&self, credential: &Credential) -> SetuResult<()> {
        if let Some(old_cred) = self.get_credential(&credential.metadata.id)? {
            if old_cred.data.holder != credential.data.holder {
                self.remove_from_index(ColumnFamily::CredentialsByHolder, &old_cred.data.holder, &credential.metadata.id)?;
                self.add_to_index(ColumnFamily::CredentialsByHolder, &credential.data.holder, &credential.metadata.id)?;
            }
            if old_cred.data.issuer != credential.data.issuer {
                self.remove_from_index(ColumnFamily::CredentialsByIssuer, &old_cred.data.issuer, &credential.metadata.id)?;
                self.add_to_index(ColumnFamily::CredentialsByIssuer, &credential.data.issuer, &credential.metadata.id)?;
            }
        }
        self.db.put(ColumnFamily::Credentials, &credential.metadata.id, credential).map_err(|e| SetuError::StorageError(e.to_string()))
    }
    
    fn delete_credential(&self, id: &ObjectId) -> SetuResult<()> {
        if let Some(cred) = self.get_credential(id)? {
            self.remove_from_index(ColumnFamily::CredentialsByHolder, &cred.data.holder, id)?;
            self.remove_from_index(ColumnFamily::CredentialsByIssuer, &cred.data.issuer, id)?;
        }
        self.db.delete(ColumnFamily::Credentials, id).map_err(|e| SetuError::StorageError(e.to_string()))
    }
    
    fn store_graph(&self, graph: &RelationGraph) -> SetuResult<ObjectId> {
        let id = graph.metadata.id;
        self.db.put(ColumnFamily::RelationGraphs, &id, graph).map_err(|e| SetuError::StorageError(e.to_string()))?;
        if let Some(owner) = &graph.metadata.owner {
            self.add_to_index(ColumnFamily::GraphsByOwner, owner, &id)?;
        }
        Ok(id)
    }
    
    fn get_graph(&self, id: &ObjectId) -> SetuResult<Option<RelationGraph>> {
        self.db.get(ColumnFamily::RelationGraphs, id).map_err(|e| SetuError::StorageError(e.to_string()))
    }
    
    fn get_graphs_by_owner(&self, owner: &Address) -> SetuResult<Vec<RelationGraph>> {
        let ids = self.get_index(ColumnFamily::GraphsByOwner, owner)?;
        let graphs: Vec<Option<RelationGraph>> = self.db.multi_get(ColumnFamily::RelationGraphs, &ids)
            .map_err(|e| SetuError::StorageError(e.to_string()))?;
        Ok(graphs.into_iter().flatten().collect())
    }
    
    fn update_graph(&self, graph: &RelationGraph) -> SetuResult<()> {
        if let Some(old_graph) = self.get_graph(&graph.metadata.id)? {
            if old_graph.metadata.owner != graph.metadata.owner {
                if let Some(old_owner) = &old_graph.metadata.owner {
                    self.remove_from_index(ColumnFamily::GraphsByOwner, old_owner, &graph.metadata.id)?;
                }
                if let Some(new_owner) = &graph.metadata.owner {
                    self.add_to_index(ColumnFamily::GraphsByOwner, new_owner, &graph.metadata.id)?;
                }
            }
        }
        self.db.put(ColumnFamily::RelationGraphs, &graph.metadata.id, graph).map_err(|e| SetuError::StorageError(e.to_string()))
    }
    
    fn delete_graph(&self, id: &ObjectId) -> SetuResult<()> {
        if let Some(graph) = self.get_graph(id)? {
            if let Some(owner) = &graph.metadata.owner {
                self.remove_from_index(ColumnFamily::GraphsByOwner, owner, id)?;
            }
        }
        self.db.delete(ColumnFamily::RelationGraphs, id).map_err(|e| SetuError::StorageError(e.to_string()))
    }
    
    fn load_account_view(&self, address: &Address) -> SetuResult<AccountView> {
        let profile = self.get_profile_by_address(address)?;
        let credentials = self.get_credentials_by_holder(address)?;
        let coins = self.get_coins_by_owner(address)?;
        let graphs = self.get_graphs_by_owner(address)?;
        Ok(AccountView::new(address.clone(), profile, credentials, coins, graphs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use setu_types::{create_profile, create_kyc_credential, create_social_graph, generate_object_id};
    
    fn setup_test_store() -> (RocksObjectStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = RocksObjectStore::open(temp_dir.path()).unwrap();
        (store, temp_dir)
    }
    
    #[test]
    fn test_coin_crud() {
        let (store, _temp) = setup_test_store();
        let alice = Address::from_str_id("alice");
        let coin = Coin::new(alice, 1000);
        let coin_id = coin.metadata.id;
        store.store_coin(&coin).unwrap();
        let retrieved = store.get_coin(&coin_id).unwrap().unwrap();
        assert_eq!(retrieved.value(), 1000);
        let coins = store.get_coins_by_owner(&alice).unwrap();
        assert_eq!(coins.len(), 1);
        store.delete_coin(&coin_id).unwrap();
        assert!(store.get_coin(&coin_id).unwrap().is_none());
    }
    
    #[test]
    fn test_profile_crud() {
        let (store, _temp) = setup_test_store();
        let alice = Address::from_str_id("alice");
        let mut profile = create_profile(alice);
        profile.data.set_display_name("Alice");
        let profile_id = profile.metadata.id;
        store.store_profile(&profile).unwrap();
        let retrieved = store.get_profile(&profile_id).unwrap().unwrap();
        assert_eq!(retrieved.data.display_name, Some("Alice".to_string()));
        let by_addr = store.get_profile_by_address(&alice).unwrap().unwrap();
        assert_eq!(by_addr.metadata.id, profile_id);
        store.delete_profile(&profile_id).unwrap();
        assert!(store.get_profile(&profile_id).unwrap().is_none());
    }
    
    #[test]
    fn test_credential_crud() {
        let (store, _temp) = setup_test_store();
        let alice = Address::from_str_id("alice");
        let issuer = Address::from_str_id("kyc_provider");
        let cred = create_kyc_credential(alice, issuer, "level_2");
        let cred_id = cred.metadata.id;
        store.store_credential(&cred).unwrap();
        let retrieved = store.get_credential(&cred_id).unwrap().unwrap();
        assert_eq!(retrieved.data.credential_type, "kyc");
        let by_holder = store.get_credentials_by_holder(&alice).unwrap();
        assert_eq!(by_holder.len(), 1);
        let by_issuer = store.get_credentials_by_issuer(&issuer).unwrap();
        assert_eq!(by_issuer.len(), 1);
        store.delete_credential(&cred_id).unwrap();
        assert!(store.get_credential(&cred_id).unwrap().is_none());
    }
    
    #[test]
    fn test_load_account_view() {
        let (store, _temp) = setup_test_store();
        let alice = Address::from_str_id("alice");
        let issuer = Address::from_str_id("kyc_provider");
        let mut profile = create_profile(alice);
        profile.data.set_display_name("Alice");
        store.store_profile(&profile).unwrap();
        let kyc = create_kyc_credential(alice, issuer, "level_2");
        store.store_credential(&kyc).unwrap();
        let coin1 = Coin::new(alice, 1000);
        let coin2 = Coin::new(alice, 500);
        store.store_coin(&coin1).unwrap();
        store.store_coin(&coin2).unwrap();
        let owner_id = generate_object_id(b"alice_owner");
        let graph = create_social_graph(owner_id, alice);
        store.store_graph(&graph).unwrap();
        let view = store.load_account_view(&alice).unwrap();
        assert_eq!(view.display_name(), Some(&"Alice".to_string()));
        assert_eq!(view.total_balance, 1500);
        assert_eq!(view.coin_count, 2);
        assert_eq!(view.valid_credential_count, 1);
        assert_eq!(view.graph_count, 1);
        assert!(view.has_kyc());
    }
}
