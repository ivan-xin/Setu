use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};

pub type ObjectId = String;
pub type Address = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObjectType {
    OwnedObject,
    SharedObject,
    ImmutableObject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Ownership {
    AddressOwner(OwnerAddress),
    ObjectOwner(ObjectOwnerInfo),
    Shared { initial_shared_version: u64 },
    Immutable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OwnerAddress {
    pub address_hash: [u8; 32],
}

impl OwnerAddress {
    pub fn from_address(address: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(address.as_bytes());
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        Self { address_hash: hash }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjectOwnerInfo {
    pub parent_id_hash: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectMetadata {
    pub id: ObjectId,
    pub version: u64,
    pub digest: String,
    pub object_type: ObjectType,
    pub owner: Option<Address>,
    pub ownership: Ownership,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Object<T: Serialize + Clone> {
    pub metadata: ObjectMetadata,
    pub data: T,
}

impl<T: Serialize + Clone> Object<T> {
    pub fn new_owned(id: ObjectId, owner: &str, data: T) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            metadata: ObjectMetadata {
                id,
                version: 1,
                digest: String::new(),
                object_type: ObjectType::OwnedObject,
                owner: Some(owner.to_string()),
                ownership: Ownership::AddressOwner(OwnerAddress::from_address(owner)),
                created_at: now,
                updated_at: now,
            },
            data,
        }
    }

    pub fn new_shared(id: ObjectId, data: T, initial_version: u64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            metadata: ObjectMetadata {
                id,
                version: initial_version,
                digest: String::new(),
                object_type: ObjectType::SharedObject,
                owner: None,
                ownership: Ownership::Shared {
                    initial_shared_version: initial_version,
                },
                created_at: now,
                updated_at: now,
            },
            data,
        }
    }

    pub fn new_immutable(id: ObjectId, data: T) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            metadata: ObjectMetadata {
                id,
                version: 1,
                digest: String::new(),
                object_type: ObjectType::ImmutableObject,
                owner: None,
                ownership: Ownership::Immutable,
                created_at: now,
                updated_at: now,
            },
            data,
        }
    }

    pub fn id(&self) -> &ObjectId {
        &self.metadata.id
    }

    pub fn version(&self) -> u64 {
        self.metadata.version
    }

    pub fn owner(&self) -> Option<&Address> {
        self.metadata.owner.as_ref()
    }

    pub fn is_owned(&self) -> bool {
        self.metadata.object_type == ObjectType::OwnedObject
    }

    pub fn is_shared(&self) -> bool {
        self.metadata.object_type == ObjectType::SharedObject
    }

    pub fn is_immutable(&self) -> bool {
        self.metadata.object_type == ObjectType::ImmutableObject
    }

    pub fn is_owned_by(&self, address: &str) -> bool {
        self.metadata.owner.as_deref() == Some(address)
    }

    pub fn increment_version(&mut self) {
        self.metadata.version += 1;
        self.metadata.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
    }

    pub fn transfer_to(&mut self, new_owner: &str) {
        if self.is_owned() {
            self.metadata.owner = Some(new_owner.to_string());
            self.metadata.ownership = Ownership::AddressOwner(OwnerAddress::from_address(new_owner));
            self.increment_version();
        }
    }
}

pub fn generate_object_id(seed: &[u8]) -> ObjectId {
    let mut hasher = Sha256::new();
    hasher.update(seed);
    hasher.update(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_le_bytes(),
    );
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestData {
        value: u64,
    }

    #[test]
    fn test_owned_object() {
        let obj = Object::new_owned(
            "obj1".to_string(),
            "alice",
            TestData { value: 100 },
        );
        assert!(obj.is_owned());
        assert_eq!(obj.version(), 1);
    }

    #[test]
    fn test_shared_object() {
        let obj = Object::new_shared(
            "obj2".to_string(),
            TestData { value: 200 },
            1,
        );
        assert!(obj.is_shared());
    }

    #[test]
    fn test_transfer() {
        let mut obj = Object::new_owned(
            "obj3".to_string(),
            "alice",
            TestData { value: 100 },
        );
        obj.transfer_to("bob");
        assert_eq!(obj.version(), 2);
    }
}

// Specific object types for common use cases

/// Account object data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountData {
    pub address: Address,
    pub balance: u64,
    pub nonce: u64,
}

/// Account object
pub type AccountObject = Object<AccountData>;

/// SBT (Soul-Bound Token) data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SBTData {
    pub owner: Address,
    pub token_id: String,
    pub metadata: std::collections::HashMap<String, String>,
}

/// SBT object
pub type SBTObject = Object<SBTData>;

/// Relation graph data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationGraphData {
    pub owner: Address,
    pub relations: Vec<(Address, String)>, // (target_address, relation_type)
}

/// Relation graph object
pub type RelationGraphObject = Object<RelationGraphData>;

/// Helper function to create an account object
pub fn create_account(address: Address, initial_balance: u64) -> AccountObject {
    AccountObject::new_owned(
        generate_object_id(address.as_bytes()),
        &address,
        AccountData {
            address: address.clone(),
            balance: initial_balance,
            nonce: 0,
        },
    )
}

/// Helper function to create an SBT
pub fn create_sbt(owner: Address, token_id: String) -> SBTObject {
    SBTObject::new_owned(
        generate_object_id(token_id.as_bytes()),
        &owner,
        SBTData {
            owner: owner.clone(),
            token_id,
            metadata: std::collections::HashMap::new(),
        },
    )
}

/// Helper function to create a relation graph
pub fn create_relation_graph(owner: Address) -> RelationGraphObject {
    RelationGraphObject::new_owned(
        generate_object_id(owner.as_bytes()),
        &owner,
        RelationGraphData {
            owner: owner.clone(),
            relations: Vec::new(),
        },
    )
}
