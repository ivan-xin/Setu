//! Governance types for on-chain proposals and decisions.
//!
//! These types are shared across setu-governance (logic), setu-validator (integration),
//! and potentially setu-solver (future: voting, evidence).

use serde::{Deserialize, Serialize};

// ========== Governance Payload (Event payload) ==========

/// Governance action payload — carried inside EventPayload::Governance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernancePayload {
    pub proposal_id: [u8; 32],
    pub action: GovernanceAction,
}

/// Discriminant for governance sub-actions within a single EventType::Governance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GovernanceAction {
    /// Submit a new proposal
    Propose(ProposalContent),
    /// Record decision and execute effects
    Execute(GovernanceDecision),
}

/// Content of a governance proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalContent {
    /// Proposer address (hex)
    pub proposer: String,
    /// Category of the proposal
    pub proposal_type: ProposalType,
    /// Human-readable title
    pub title: String,
    /// Human-readable description
    pub description: String,
    /// Concrete effect to apply if approved
    pub action: ProposalEffect,
}

/// Category of governance proposal
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProposalType {
    ParameterChange,
    ValidatorSlash,
    DisputeResolution,
    SubnetPolicy,
}

/// Concrete effect of a passed proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProposalEffect {
    UpdateParameter { key: String, value: Vec<u8> },
    SlashValidator { validator_id: String, amount: u64 },
    ResolveDispute { dispute_id: [u8; 32], resolution: Vec<u8> },
}

/// Decision from evaluation (source-agnostic: could be Agent subnet, voting, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceDecision {
    pub approved: bool,
    pub reasoning: String,
    pub conditions: Vec<String>,
}

// ========== Governance Proposal (SMT state object) ==========

/// Stored in GOVERNANCE subnet SMT as JSON (non-Coin object → G6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceProposal {
    pub proposal_id: [u8; 32],
    pub content: ProposalContent,
    pub status: ProposalStatus,
    pub decision: Option<GovernanceDecision>,
    /// Timestamp from ExecutionContext (G1: deterministic)
    pub created_at: u64,
    pub decided_at: Option<u64>,
}

/// Lifecycle status of a governance proposal
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProposalStatus {
    /// Submitted, awaiting evaluation
    Pending,
    /// Approved, effects applied
    Approved,
    /// Rejected
    Rejected,
    /// Execution of effects failed
    Failed,
}

// ========== Tests ==========

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_content() -> ProposalContent {
        ProposalContent {
            proposer: "alice".into(),
            proposal_type: ProposalType::ParameterChange,
            title: "Increase block size".into(),
            description: "Set max_block_size to 2MB".into(),
            action: ProposalEffect::UpdateParameter {
                key: "max_block_size".into(),
                value: vec![0, 0, 0, 2],
            },
        }
    }

    fn sample_decision(approved: bool) -> GovernanceDecision {
        GovernanceDecision {
            approved,
            reasoning: "Looks good".into(),
            conditions: vec![],
        }
    }

    #[test]
    fn test_governance_payload_serde() {
        let payload = GovernancePayload {
            proposal_id: [1u8; 32],
            action: GovernanceAction::Propose(sample_content()),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let decoded: GovernancePayload = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.proposal_id, [1u8; 32]);
    }

    #[test]
    fn test_governance_proposal_serde() {
        let proposal = GovernanceProposal {
            proposal_id: [2u8; 32],
            content: sample_content(),
            status: ProposalStatus::Pending,
            decision: None,
            created_at: 1000,
            decided_at: None,
        };
        let json = serde_json::to_string(&proposal).unwrap();
        let decoded: GovernanceProposal = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.status, ProposalStatus::Pending);
        assert_eq!(decoded.proposal_id, [2u8; 32]);
    }

    #[test]
    fn test_governance_decision_execute_serde() {
        let payload = GovernancePayload {
            proposal_id: [3u8; 32],
            action: GovernanceAction::Execute(sample_decision(true)),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let decoded: GovernancePayload = serde_json::from_str(&json).unwrap();
        match decoded.action {
            GovernanceAction::Execute(d) => assert!(d.approved),
            _ => panic!("Expected Execute"),
        }
    }
}
