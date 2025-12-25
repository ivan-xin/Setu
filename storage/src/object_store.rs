//! Object Store trait for Coin/Profile/Credential/RelationGraph storage operations
use setu_types::{
    ObjectId, Address,
    Coin, Profile, Credential, RelationGraph, AccountView,
    SetuResult,
};

/// Object store interface for storing and retrieving Setu objects
/// 
/// Address is the root of ownership. All objects are owned by an Address.
pub trait ObjectStore {
    // ========== Coin operations ==========
    
    /// Store a coin and update indexes
    fn store_coin(&self, coin: &Coin) -> SetuResult<ObjectId>;
    
    /// Get a coin by ID
    fn get_coin(&self, id: &ObjectId) -> SetuResult<Option<Coin>>;
    
    /// Get all coins owned by an address
    fn get_coins_by_owner(&self, owner: &Address) -> SetuResult<Vec<Coin>>;
    
    /// Update a coin (replaces existing)
    fn update_coin(&self, coin: &Coin) -> SetuResult<()>;
    
    /// Delete a coin and clean up indexes
    fn delete_coin(&self, id: &ObjectId) -> SetuResult<()>;
    
    // ========== Profile operations ==========
    
    /// Store a profile and update indexes
    fn store_profile(&self, profile: &Profile) -> SetuResult<ObjectId>;
    
    /// Get a profile by ID
    fn get_profile(&self, id: &ObjectId) -> SetuResult<Option<Profile>>;
    
    /// Get profile by owner address (one address has at most one profile)
    fn get_profile_by_address(&self, address: &Address) -> SetuResult<Option<Profile>>;
    
    /// Update a profile (replaces existing)
    fn update_profile(&self, profile: &Profile) -> SetuResult<()>;
    
    /// Delete a profile and clean up indexes
    fn delete_profile(&self, id: &ObjectId) -> SetuResult<()>;
    
    // ========== Credential operations ==========
    
    /// Store a credential and update indexes
    fn store_credential(&self, credential: &Credential) -> SetuResult<ObjectId>;
    
    /// Get a credential by ID
    fn get_credential(&self, id: &ObjectId) -> SetuResult<Option<Credential>>;
    
    /// Get all credentials for an address (holder)
    fn get_credentials_by_holder(&self, holder: &Address) -> SetuResult<Vec<Credential>>;
    
    /// Get credentials issued by an address
    fn get_credentials_by_issuer(&self, issuer: &Address) -> SetuResult<Vec<Credential>>;
    
    /// Update a credential (replaces existing)
    fn update_credential(&self, credential: &Credential) -> SetuResult<()>;
    
    /// Delete a credential and clean up indexes
    fn delete_credential(&self, id: &ObjectId) -> SetuResult<()>;
    
    // ========== RelationGraph operations ==========
    
    /// Store a relation graph and update indexes
    fn store_graph(&self, graph: &RelationGraph) -> SetuResult<ObjectId>;
    
    /// Get a relation graph by ID
    fn get_graph(&self, id: &ObjectId) -> SetuResult<Option<RelationGraph>>;
    
    /// Get all relation graphs owned by an address
    fn get_graphs_by_owner(&self, owner: &Address) -> SetuResult<Vec<RelationGraph>>;
    
    /// Update a relation graph (replaces existing)
    fn update_graph(&self, graph: &RelationGraph) -> SetuResult<()>;
    
    /// Delete a relation graph and clean up indexes
    fn delete_graph(&self, id: &ObjectId) -> SetuResult<()>;
    
    // ========== Aggregation ==========
    
    /// Load AccountView (aggregates Profile, Credentials, Coins, and Graphs)
    fn load_account_view(&self, address: &Address) -> SetuResult<AccountView>;
}
