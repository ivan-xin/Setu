Setu Documentation
==================

Overview
--------
Setu is a causally driven distributed ledger (RIB) that advances with a verifiable logical clock (VLC) rather than wall time. Transfers are organized into a DAG in real time, folded into batches (FoldGraph) for commitment, and secured with Proof of Causal Work (PoCW). Network sharding reduces gossip and conflict scope.

Dataflow (high level)
---------------------
- Event intake: accept `Transfer` events.
- DAG builder (real-time): extract causal deps (resource/object keys, VLC), create nodes/edges, maintain depth/root/leaf/degree, detect conflicts (RW/WW) and resolve deterministically (VLC order > power > transfer id).
- FoldGraph (batch): group DAG nodes by depth window ΔH into folds, compute `FoldHash` (Merkle), link folds, persist, and drive PoCW voting.
- PoCW: nodes vote on `FoldHash`; 2/3 majority locks a fold, otherwise re-fold.
- TEE path: `VerifyVLC`, `VerifyPower`, `VerifyObjectVersion`, `Detect/ResolveConflict`, `CalcFoldHash` run inside SGX/SEV/SoftwareTEE.
- Gossip + anti-entropy: disseminate Transfer/Fold/Vote; heal gaps. Storage is per-node; eventual consistency via consensus.
- Sharding: route by resource/object key to limit gossip/conflict domains; cross-shard via receipts/bridging (freeze + receipt → consume on target shard).

Roles
-----
- RIB Node: unified term (can both create transfers and validate). Optional specialization into ingestion/validation if needed.

Persistence (suggested tables)
------------------------------
- `transfers`, `dag_dependencies`, `dag_nodes`
- `causality_folds`, `fold_transfers`, `fold_graph`
- `pocw_votes`, `pocw_locks`

Next docs to add
----------------
- Protocol details per module (dag-engine, conflict, fold-engine, vlc, gossip, pocw, tee-adapter).
- Shard routing rules and cross-shard receipt format.
- API surfaces (submit/query).

