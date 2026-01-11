use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};

// Use independent VLC library
pub use setu_vlc::{VectorClock, VLCSnapshot};

// Placeholder types (to be replaced with actual implementations)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transfer {
    pub from: String,
    pub to: String,
    pub amount: u64,
}

/// Validator registration data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorRegistration {
    pub validator_id: String,
    pub address: String,
    pub port: u16,
    pub public_key: Option<Vec<u8>>,
    pub stake: Option<u64>,
}

/// Solver registration data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolverRegistration {
    pub solver_id: String,
    pub address: String,
    pub port: u16,
    pub capacity: u32,
    pub shard_id: Option<String>,
    pub resources: Vec<String>,
    pub public_key: Option<Vec<u8>>,
}

/// Power consumption data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerConsumption {
    pub user_id: String,
    pub amount: u64,
    pub reason: String,
}

/// Task submission data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSubmission {
    pub task_id: String,
    pub task_type: String,
    pub submitter: String,
    pub payload: Vec<u8>,
}

/// Unregistration data (for both Validator and Solver)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unregistration {
    pub node_id: String,
    pub node_type: String, // "validator" or "solver"
}

/// Subnet registration data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubnetRegistration {
    /// Unique subnet identifier (will be assigned if not provided)
    pub subnet_id: String,
    /// Human-readable name for the subnet
    pub name: String,
    /// Description of the subnet's purpose
    pub description: Option<String>,
    /// Owner/creator of the subnet
    pub owner: String,
    /// Maximum number of users allowed (None = unlimited)
    pub max_users: Option<u64>,
    /// Resource limits for the subnet
    pub resource_limits: Option<SubnetResourceLimits>,
    /// List of Solver IDs assigned to this subnet
    pub assigned_solvers: Vec<String>,
    /// Subnet type (application, organization, etc.)
    pub subnet_type: SubnetType,
    /// Parent subnet ID (None = root subnet)
    pub parent_subnet_id: Option<String>,
}

/// Subnet resource limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubnetResourceLimits {
    /// Maximum transactions per second
    pub max_tps: Option<u64>,
    /// Maximum storage in bytes
    pub max_storage_bytes: Option<u64>,
    /// Maximum compute units per transaction
    pub max_compute_units: Option<u64>,
}

/// Subnet type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubnetType {
    /// Root subnet (subnet-0, created at genesis)
    Root,
    /// Application subnet (for a specific dApp)
    Application,
    /// Organization subnet (for a company/DAO)
    Organization,
    /// Personal subnet (for individual users)
    Personal,
    /// System subnet (for internal operations)
    System,
}

impl Default for SubnetType {
    fn default() -> Self {
        SubnetType::Application
    }
}

/// User registration data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRegistration {
    /// User's unique identifier (address/public key hash)
    pub user_id: String,
    /// User's public key for authentication
    pub public_key: Vec<u8>,
    /// Subnet ID to register in (None = root subnet)
    pub subnet_id: Option<String>,
    /// Optional display name
    pub display_name: Option<String>,
    /// Optional metadata (JSON string)
    pub metadata: Option<String>,
    /// Initial power allocation
    pub initial_power: Option<u64>,
}

pub type EventId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventStatus {
    Pending,
    InWorkQueue,
    Executed,
    Confirmed,
    Finalized,
    Failed,
}

/// Event types supported by the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    /// Genesis event (first event in the DAG)
    Genesis,
    /// System event (internal operations)
    System,
    /// Transfer event (Flux transfer between accounts)
    Transfer,
    /// Validator registration event
    ValidatorRegister,
    /// Validator unregistration event
    ValidatorUnregister,
    /// Solver registration event
    SolverRegister,
    /// Solver unregistration event
    SolverUnregister,
    /// Subnet registration event (create a new subnet)
    SubnetRegister,
    /// User registration event (register a user in a subnet)
    UserRegister,
    /// Power consumption event
    PowerConsume,
    /// Task submission event
    TaskSubmit,
    /// Agent chat event (future)
    AgentChat,
    /// SBT/Relationship event (future)
    Relationship,
}

impl EventType {
    /// Check if this event type requires execution by Solver
    pub fn requires_solver_execution(&self) -> bool {
        matches!(
            self,
            EventType::Transfer
                | EventType::ValidatorRegister
                | EventType::ValidatorUnregister
                | EventType::SolverRegister
                | EventType::SolverUnregister
                | EventType::SubnetRegister
                | EventType::UserRegister
                | EventType::PowerConsume
                | EventType::TaskSubmit
        )
    }
    
    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            EventType::Genesis => "Genesis",
            EventType::System => "System",
            EventType::Transfer => "Transfer",
            EventType::ValidatorRegister => "ValidatorRegister",
            EventType::ValidatorUnregister => "ValidatorUnregister",
            EventType::SolverRegister => "SolverRegister",
            EventType::SolverUnregister => "SolverUnregister",
            EventType::SubnetRegister => "SubnetRegister",
            EventType::UserRegister => "UserRegister",
            EventType::PowerConsume => "PowerConsume",
            EventType::TaskSubmit => "TaskSubmit",
            EventType::AgentChat => "AgentChat",
            EventType::Relationship => "Relationship",
        }
    }
}

/// Event payload - contains the actual data for different event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventPayload {
    None,
    Transfer(Transfer),
    ValidatorRegister(ValidatorRegistration),
    ValidatorUnregister(Unregistration),
    SolverRegister(SolverRegistration),
    SolverUnregister(Unregistration),
    SubnetRegister(SubnetRegistration),
    UserRegister(UserRegistration),
    PowerConsume(PowerConsumption),
    TaskSubmit(TaskSubmission),
}

impl Default for EventPayload {
    fn default() -> Self {
        EventPayload::None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub event_type: EventType,
    pub parent_ids: Vec<EventId>,
    /// Legacy transfer field (for backward compatibility)
    pub transfer: Option<Transfer>,
    /// New unified payload field
    pub payload: EventPayload,
    pub vlc_snapshot: VLCSnapshot,
    pub creator: String,
    pub status: EventStatus,
    pub execution_result: Option<ExecutionResult>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub message: Option<String>,
    pub state_changes: Vec<StateChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    pub key: String,
    pub old_value: Option<Vec<u8>>,
    pub new_value: Option<Vec<u8>>,
}

impl Event {
    pub fn new(
        event_type: EventType,
        parent_ids: Vec<EventId>,
        vlc_snapshot: VLCSnapshot,
        creator: String,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let id = Self::compute_id(&parent_ids, &vlc_snapshot, &creator, timestamp);
        
        Self {
            id,
            event_type,
            parent_ids,
            transfer: None,
            payload: EventPayload::None,
            vlc_snapshot,
            creator,
            status: EventStatus::Pending,
            execution_result: None,
            timestamp,
        }
    }

    pub fn genesis(creator: String, vlc_snapshot: VLCSnapshot) -> Self {
        Self::new(EventType::Genesis, vec![], vlc_snapshot, creator)
    }
    
    /// Create a transfer event
    pub fn transfer(
        transfer: Transfer,
        parent_ids: Vec<EventId>,
        vlc_snapshot: VLCSnapshot,
        creator: String,
    ) -> Self {
        let mut event = Self::new(EventType::Transfer, parent_ids, vlc_snapshot, creator);
        event.transfer = Some(transfer.clone());
        event.payload = EventPayload::Transfer(transfer);
        event
    }
    
    /// Create a validator registration event
    pub fn validator_register(
        registration: ValidatorRegistration,
        parent_ids: Vec<EventId>,
        vlc_snapshot: VLCSnapshot,
        creator: String,
    ) -> Self {
        let mut event = Self::new(EventType::ValidatorRegister, parent_ids, vlc_snapshot, creator);
        event.payload = EventPayload::ValidatorRegister(registration);
        event
    }
    
    /// Create a solver registration event
    pub fn solver_register(
        registration: SolverRegistration,
        parent_ids: Vec<EventId>,
        vlc_snapshot: VLCSnapshot,
        creator: String,
    ) -> Self {
        let mut event = Self::new(EventType::SolverRegister, parent_ids, vlc_snapshot, creator);
        event.payload = EventPayload::SolverRegister(registration);
        event
    }
    
    /// Create a validator unregistration event
    pub fn validator_unregister(
        unregistration: Unregistration,
        parent_ids: Vec<EventId>,
        vlc_snapshot: VLCSnapshot,
        creator: String,
    ) -> Self {
        let mut event = Self::new(EventType::ValidatorUnregister, parent_ids, vlc_snapshot, creator);
        event.payload = EventPayload::ValidatorUnregister(unregistration);
        event
    }
    
    /// Create a solver unregistration event
    pub fn solver_unregister(
        unregistration: Unregistration,
        parent_ids: Vec<EventId>,
        vlc_snapshot: VLCSnapshot,
        creator: String,
    ) -> Self {
        let mut event = Self::new(EventType::SolverUnregister, parent_ids, vlc_snapshot, creator);
        event.payload = EventPayload::SolverUnregister(unregistration);
        event
    }
    
    /// Create a power consumption event
    pub fn power_consume(
        consumption: PowerConsumption,
        parent_ids: Vec<EventId>,
        vlc_snapshot: VLCSnapshot,
        creator: String,
    ) -> Self {
        let mut event = Self::new(EventType::PowerConsume, parent_ids, vlc_snapshot, creator);
        event.payload = EventPayload::PowerConsume(consumption);
        event
    }
    
    /// Create a task submission event
    pub fn task_submit(
        task: TaskSubmission,
        parent_ids: Vec<EventId>,
        vlc_snapshot: VLCSnapshot,
        creator: String,
    ) -> Self {
        let mut event = Self::new(EventType::TaskSubmit, parent_ids, vlc_snapshot, creator);
        event.payload = EventPayload::TaskSubmit(task);
        event
    }
    
    /// Create a subnet registration event
    pub fn subnet_register(
        registration: SubnetRegistration,
        parent_ids: Vec<EventId>,
        vlc_snapshot: VLCSnapshot,
        creator: String,
    ) -> Self {
        let mut event = Self::new(EventType::SubnetRegister, parent_ids, vlc_snapshot, creator);
        event.payload = EventPayload::SubnetRegister(registration);
        event
    }
    
    /// Create a user registration event
    pub fn user_register(
        registration: UserRegistration,
        parent_ids: Vec<EventId>,
        vlc_snapshot: VLCSnapshot,
        creator: String,
    ) -> Self {
        let mut event = Self::new(EventType::UserRegister, parent_ids, vlc_snapshot, creator);
        event.payload = EventPayload::UserRegister(registration);
        event
    }

    fn compute_id(
        parent_ids: &[EventId],
        vlc_snapshot: &VLCSnapshot,
        creator: &str,
        timestamp: u64,
    ) -> EventId {
        let mut hasher = Sha256::new();
        for parent_id in parent_ids {
            hasher.update(parent_id.as_bytes());
        }
        hasher.update(vlc_snapshot.logical_time.to_le_bytes());
        hasher.update(creator.as_bytes());
        hasher.update(timestamp.to_le_bytes());
        hex::encode(hasher.finalize())
    }

    /// Legacy method for backward compatibility
    pub fn with_transfer(mut self, transfer: Transfer) -> Self {
        self.transfer = Some(transfer.clone());
        self.payload = EventPayload::Transfer(transfer);
        self
    }
    
    /// Set payload
    pub fn with_payload(mut self, payload: EventPayload) -> Self {
        self.payload = payload;
        self
    }

    pub fn set_status(&mut self, status: EventStatus) {
        self.status = status;
    }

    pub fn set_execution_result(&mut self, result: ExecutionResult) {
        let success = result.success;
        self.execution_result = Some(result);
        if success {
            self.status = EventStatus::Executed;
        } else {
            self.status = EventStatus::Failed;
        }
    }

    pub fn is_genesis(&self) -> bool {
        self.event_type == EventType::Genesis
    }

    pub fn has_parents(&self) -> bool {
        !self.parent_ids.is_empty()
    }

    pub fn depends_on(&self, event_id: &EventId) -> bool {
        self.parent_ids.contains(event_id)
    }
    
    /// Check if this event is a registration event
    pub fn is_registration(&self) -> bool {
        matches!(
            self.event_type,
            EventType::ValidatorRegister 
                | EventType::SolverRegister
                | EventType::SubnetRegister
                | EventType::UserRegister
        )
    }
    
    /// Check if this event is an unregistration event
    pub fn is_unregistration(&self) -> bool {
        matches!(
            self.event_type,
            EventType::ValidatorUnregister | EventType::SolverUnregister
        )
    }
    
    /// Get resources affected by this event (for dependency tracking)
    pub fn affected_resources(&self) -> Vec<String> {
        match &self.payload {
            EventPayload::Transfer(t) => {
                vec![format!("account:{}", t.from), format!("account:{}", t.to)]
            }
            EventPayload::ValidatorRegister(r) => {
                vec![format!("validator:{}", r.validator_id)]
            }
            EventPayload::SolverRegister(r) => {
                vec![format!("solver:{}", r.solver_id)]
            }
            EventPayload::ValidatorUnregister(u) | EventPayload::SolverUnregister(u) => {
                vec![format!("node:{}", u.node_id)]
            }
            EventPayload::SubnetRegister(s) => {
                vec![format!("subnet:{}", s.subnet_id)]
            }
            EventPayload::UserRegister(u) => {
                let subnet = u.subnet_id.as_deref().unwrap_or("subnet-0");
                vec![
                    format!("user:{}", u.user_id),
                    format!("subnet:{}", subnet),
                ]
            }
            EventPayload::PowerConsume(p) => {
                vec![format!("power:{}", p.user_id)]
            }
            EventPayload::TaskSubmit(t) => {
                vec![format!("task:{}", t.task_id)]
            }
            EventPayload::None => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_vlc_snapshot() -> VLCSnapshot {
        VLCSnapshot {
            vector_clock: VectorClock::new(),
            logical_time: 1,
            physical_time: 1000,
        }
    }

    #[test]
    fn test_event_creation() {
        let event = Event::new(
            EventType::Transfer,
            vec![],
            create_vlc_snapshot(),
            "node1".to_string(),
        );
        assert!(!event.id.is_empty());
        assert_eq!(event.status, EventStatus::Pending);
    }

    #[test]
    fn test_genesis_event() {
        let event = Event::genesis("node1".to_string(), create_vlc_snapshot());
        assert!(event.is_genesis());
        assert!(!event.has_parents());
    }
    
    #[test]
    fn test_transfer_event() {
        let transfer = Transfer {
            from: "alice".to_string(),
            to: "bob".to_string(),
            amount: 100,
        };
        let event = Event::transfer(
            transfer,
            vec![],
            create_vlc_snapshot(),
            "solver1".to_string(),
        );
        assert_eq!(event.event_type, EventType::Transfer);
        assert!(event.transfer.is_some());
        
        let resources = event.affected_resources();
        assert!(resources.contains(&"account:alice".to_string()));
        assert!(resources.contains(&"account:bob".to_string()));
    }
    
    #[test]
    fn test_solver_register_event() {
        let registration = SolverRegistration {
            solver_id: "solver-1".to_string(),
            address: "127.0.0.1".to_string(),
            port: 9001,
            capacity: 100,
            shard_id: Some("shard-0".to_string()),
            resources: vec!["ETH".to_string()],
            public_key: None,
        };
        let event = Event::solver_register(
            registration,
            vec![],
            create_vlc_snapshot(),
            "solver-1".to_string(),
        );
        assert_eq!(event.event_type, EventType::SolverRegister);
        assert!(event.is_registration());
        
        let resources = event.affected_resources();
        assert!(resources.contains(&"solver:solver-1".to_string()));
    }
    
    #[test]
    fn test_event_type_requires_solver() {
        assert!(EventType::Transfer.requires_solver_execution());
        assert!(EventType::SolverRegister.requires_solver_execution());
        assert!(EventType::ValidatorRegister.requires_solver_execution());
        assert!(EventType::SubnetRegister.requires_solver_execution());
        assert!(EventType::UserRegister.requires_solver_execution());
        assert!(!EventType::Genesis.requires_solver_execution());
        assert!(!EventType::System.requires_solver_execution());
    }
    
    #[test]
    fn test_subnet_register_event() {
        let registration = SubnetRegistration {
            subnet_id: "subnet-1".to_string(),
            name: "DeFi App".to_string(),
            description: Some("A DeFi application subnet".to_string()),
            owner: "alice".to_string(),
            max_users: Some(10000),
            resource_limits: Some(SubnetResourceLimits {
                max_tps: Some(1000),
                max_storage_bytes: Some(1024 * 1024 * 1024),
                max_compute_units: Some(100000),
            }),
            assigned_solvers: vec!["solver-1".to_string(), "solver-2".to_string()],
            subnet_type: SubnetType::Application,
            parent_subnet_id: None,
        };
        let event = Event::subnet_register(
            registration,
            vec![],
            create_vlc_snapshot(),
            "alice".to_string(),
        );
        assert_eq!(event.event_type, EventType::SubnetRegister);
        assert!(event.is_registration());
        
        let resources = event.affected_resources();
        assert!(resources.contains(&"subnet:subnet-1".to_string()));
    }
    
    #[test]
    fn test_user_register_event() {
        let registration = UserRegistration {
            user_id: "user-alice-123".to_string(),
            public_key: vec![1, 2, 3, 4],
            subnet_id: Some("subnet-1".to_string()),
            display_name: Some("Alice".to_string()),
            metadata: None,
            initial_power: Some(100),
        };
        let event = Event::user_register(
            registration,
            vec![],
            create_vlc_snapshot(),
            "user-alice-123".to_string(),
        );
        assert_eq!(event.event_type, EventType::UserRegister);
        assert!(event.is_registration());
        
        let resources = event.affected_resources();
        assert!(resources.contains(&"user:user-alice-123".to_string()));
        assert!(resources.contains(&"subnet:subnet-1".to_string()));
    }
    
    #[test]
    fn test_user_register_default_subnet() {
        let registration = UserRegistration {
            user_id: "user-bob-456".to_string(),
            public_key: vec![5, 6, 7, 8],
            subnet_id: None, // Should default to subnet-0
            display_name: None,
            metadata: None,
            initial_power: None,
        };
        let event = Event::user_register(
            registration,
            vec![],
            create_vlc_snapshot(),
            "user-bob-456".to_string(),
        );
        
        let resources = event.affected_resources();
        assert!(resources.contains(&"user:user-bob-456".to_string()));
        assert!(resources.contains(&"subnet:subnet-0".to_string())); // Default to root subnet
    }
}
