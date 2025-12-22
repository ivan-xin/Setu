Setu Crates Layout (proposed)
=============================

- `core-types`: Transfer/Object/VLC/DAG/Fold/Vote/Conflict primitives; shared traits.
- `vlc`: vector logical clock + DOBC utilities.
- `dag-engine`: real-time DAG builder (deps, degrees, roots/leaves), conflict detection hook.
- `conflict`: deterministic resolution (VLC order > power > transfer id), RW/WW checks.
- `fold-engine`: depth-window grouping, fold hash (Merkle), fold graph parent/child links, re-fold support.
- `tee-adapter`: TEE abstraction + SoftwareTEE + SGX/SEV adapters; runs verify/resolve/hash logic.
- `gossip`: dissemination of Transfer/Fold/Vote, anti-entropy sync.
- `pocw`: Proof of Causal Work voting/locking (2/3 majority).
- `storage`: storage abstractions and backends (e.g., Postgres/RocksDB).
- `api`: HTTP/gRPC submission and query surfaces.
- `node` (bin): main assembly of the above components.
- `move-adapter` (optional): Move VM integration hook.

