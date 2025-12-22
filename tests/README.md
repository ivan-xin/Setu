Testing Strategy
================

- Unit: core types (VLC ordering, conflict rules), DAG builder invariants, fold hashing.
- Integration: end-to-end flow Transfer → DAG → Fold → PoCW vote/lock.
- Property tests: concurrency cases for RW/WW conflicts, shard routing determinism.
- TEE adapter: run software path in CI; gated SGX/SEV where available.
- Network: gossip/anti-entropy correctness under message loss and reordering.

