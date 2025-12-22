Configuration
=============

Areas
-----
- Networking: gossip peers, shard routing table, timeouts, anti-entropy intervals.
- Consensus: PoCW thresholds (2/3), fold window ΔH, DAG limits (max indegree/outdegree).
- Storage: backend selection (Postgres/RocksDB), connection pools, retention.
- TEE: adapter selection (software/SGX/SEV), attestation requirements.
- API: rate limits, authn/authz, submission size limits.

Format
------
- Prefer TOML with environment overrides.
- Provide defaults plus validation on startup.

