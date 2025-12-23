// ========== Core Modules ==========
pub mod event;
pub mod consensus;
pub mod node;
pub mod object;

// ========== New Object Model ==========
pub mod coin;        // 新增：Coin对象
pub mod sbt;         // 重构：SBT对象
pub mod relation;    // 重构：RelationGraph对象
pub mod sbt_view;    // 新增：SBT聚合视图

// ========== Deprecated (向后兼容) ==========
// TODO: 如果需要Account向后兼容，实现简化的account模块
// #[deprecated(note = "Account concept removed. Use SBT as identity instead.")]
// pub mod account;

// Export commonly used types
pub use event::{Event, EventId, EventStatus, EventType, Transfer};
pub use consensus::{Anchor, AnchorId, ConsensusFrame, CFId, CFStatus};
pub use node::*;

// Re-export VLC types from setu-vlc
pub use setu_vlc::{VectorClock, VLCSnapshot};

// ========== New Object Model Exports ==========
pub use object::{Object, ObjectId, Address, ObjectType, ObjectMetadata, Ownership};

// Coin相关
pub use coin::{Coin, Balance, create_coin};

// SBT相关
pub use sbt::{SBT, SBTData, Credential, create_sbt, create_personal_sbt, create_organization_sbt};

// RelationGraph相关  
pub use relation::{
    RelationGraph, RelationGraphData, Relation,
    create_social_graph, create_professional_graph,
};

// 聚合视图
pub use sbt_view::SBTView;

// ========== Deprecated Types (向后兼容) - 暂时注释 ==========
// TODO: 如果需要向后兼容，实现简化的account模块
// #[deprecated(note = "Use SBT instead of Account")]
// pub use account::{AccountData, create_account};
//
// #[deprecated(note = "Use SBT directly")]
// pub type AccountObject = account::Account;
//
// #[deprecated(note = "Use SBT directly")]
// pub type SBTObject = SBT;
//
// #[deprecated(note = "Use RelationGraph directly")]
// pub type RelationGraphObject = RelationGraph;

// Error types
pub type SetuResult<T> = Result<T, SetuError>;

#[derive(Debug, thiserror::Error)]
pub enum SetuError {
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Invalid data: {0}")]
    InvalidData(String),
    
    #[error("Invalid transfer: {0}")]
    InvalidTransfer(String),
    
    #[error("Other error: {0}")]
    Other(String),
}
// pub use account::*;
// pub use sbt::*;
// pub use relation::*;
