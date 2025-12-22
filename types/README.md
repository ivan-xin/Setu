Types Overview (planned)
========================

Core entities
-------------
- `Transfer`: payload + resource/object keys, VLC, power, signer; unique transfer id.
- `Object`: domain object with versioning (e.g., balances, generic object_id).
- `VLC`: vector logical clock / DOBC representation and comparison rules.
- `DAGNode`: transfer plus causal metadata (depth, indegree/outdegree, root/leaf flags).
- `Fold`: batch of DAG nodes grouped by depth window; `FoldHash` via Merkle.
- `Vote`: PoCW vote referencing `FoldHash` and voter identity.
- `ConflictOutcome`: deterministic decision (winner/loser) with reasons.

Traits (examples)
-----------------
- `CausalComparable`: compare VLCs, determine happens-before or concurrency.
- `ConflictDetect`: define RW/WW detection over resource/object keys.
- `FoldHasher`: compute fold hash from included transfers/nodes.

Notes
-----
- Keep types `no_std` friendly where practical; isolate crypto/backends behind features.
- Derive serde where applicable for network/storage.

