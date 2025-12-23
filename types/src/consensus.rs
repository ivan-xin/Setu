use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::collections::HashMap;

use crate::event::{EventId, VLCSnapshot};

#[allow(unused_imports)]
use crate::event::VectorClock;

pub type AnchorId = String;
pub type CFId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anchor {
    pub id: AnchorId,
    pub event_ids: Vec<EventId>,
    pub vlc_snapshot: VLCSnapshot,
    pub state_root: String,
    pub previous_anchor: Option<AnchorId>,
    pub depth: u64,
    pub timestamp: u64,
}

impl Anchor {
    pub fn new(
        event_ids: Vec<EventId>,
        vlc_snapshot: VLCSnapshot,
        state_root: String,
        previous_anchor: Option<AnchorId>,
        depth: u64,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let id = Self::compute_id(&event_ids, &vlc_snapshot, &state_root, timestamp);

        Self {
            id,
            event_ids,
            vlc_snapshot,
            state_root,
            previous_anchor,
            depth,
            timestamp,
        }
    }

    fn compute_id(
        event_ids: &[EventId],
        vlc_snapshot: &VLCSnapshot,
        state_root: &str,
        timestamp: u64,
    ) -> AnchorId {
        let mut hasher = Sha256::new();
        for event_id in event_ids {
            hasher.update(event_id.as_bytes());
        }
        hasher.update(vlc_snapshot.logical_time.to_le_bytes());
        hasher.update(state_root.as_bytes());
        hasher.update(timestamp.to_le_bytes());
        hex::encode(hasher.finalize())
    }

    pub fn event_count(&self) -> usize {
        self.event_ids.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CFStatus {
    Proposed,
    Voting,
    Approved,
    Finalized,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub validator_id: String,
    pub cf_id: CFId,
    pub approve: bool,
    pub signature: Vec<u8>,
    pub timestamp: u64,
}

impl Vote {
    pub fn new(validator_id: String, cf_id: CFId, approve: bool) -> Self {
        Self {
            validator_id,
            cf_id,
            approve,
            signature: Vec::new(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }

    pub fn with_signature(mut self, signature: Vec<u8>) -> Self {
        self.signature = signature;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusFrame {
    pub id: CFId,
    pub anchor: Anchor,
    pub proposer: String,
    pub status: CFStatus,
    pub votes: HashMap<String, Vote>,
    pub created_at: u64,
    pub finalized_at: Option<u64>,
}

impl ConsensusFrame {
    pub fn new(anchor: Anchor, proposer: String) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let id = Self::compute_id(&anchor, &proposer, timestamp);

        Self {
            id,
            anchor,
            proposer,
            status: CFStatus::Proposed,
            votes: HashMap::new(),
            created_at: timestamp,
            finalized_at: None,
        }
    }

    fn compute_id(anchor: &Anchor, proposer: &str, timestamp: u64) -> CFId {
        let mut hasher = Sha256::new();
        hasher.update(anchor.id.as_bytes());
        hasher.update(proposer.as_bytes());
        hasher.update(timestamp.to_le_bytes());
        hex::encode(hasher.finalize())
    }

    pub fn add_vote(&mut self, vote: Vote) {
        self.votes.insert(vote.validator_id.clone(), vote);
    }

    pub fn approve_count(&self) -> usize {
        self.votes.values().filter(|v| v.approve).count()
    }

    pub fn reject_count(&self) -> usize {
        self.votes.values().filter(|v| !v.approve).count()
    }

    pub fn check_quorum(&self, total_validators: usize) -> bool {
        let threshold = (total_validators * 2) / 3 + 1;
        self.approve_count() >= threshold
    }

    pub fn finalize(&mut self) {
        self.status = CFStatus::Finalized;
        self.finalized_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        );
    }

    pub fn reject(&mut self) {
        self.status = CFStatus::Rejected;
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConsensusConfig {
    pub vlc_delta_threshold: u64,
    pub min_events_per_cf: usize,
    pub max_events_per_cf: usize,
    pub cf_timeout_ms: u64,
    pub validator_count: usize,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            vlc_delta_threshold: 10,
            min_events_per_cf: 1,
            max_events_per_cf: 1000,
            cf_timeout_ms: 5000,
            validator_count: 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_vlc_snapshot() -> VLCSnapshot {
        VLCSnapshot {
            vector_clock: VectorClock::new(),
            logical_time: 10,
            physical_time: 10000,
        }
    }

    #[test]
    fn test_anchor_creation() {
        let anchor = Anchor::new(
            vec!["event1".to_string(), "event2".to_string()],
            create_vlc_snapshot(),
            "state_root_hash".to_string(),
            None,
            0,
        );
        assert_eq!(anchor.event_count(), 2);
    }

    #[test]
    fn test_cf_voting() {
        let anchor = Anchor::new(
            vec!["event1".to_string()],
            create_vlc_snapshot(),
            "state_root".to_string(),
            None,
            0,
        );
        let mut cf = ConsensusFrame::new(anchor, "validator1".to_string());

        cf.add_vote(Vote::new("validator1".to_string(), cf.id.clone(), true));
        cf.add_vote(Vote::new("validator2".to_string(), cf.id.clone(), true));
        cf.add_vote(Vote::new("validator3".to_string(), cf.id.clone(), true));

        assert_eq!(cf.approve_count(), 3);
        assert_eq!(cf.reject_count(), 0);
        assert!(cf.check_quorum(3));
    }
}
