Storage
=======

Targets
-------
- Persistent DAG data: transfers, dependencies, node metadata.
- FoldGraph data: folds, fold-transfers mapping, parent/child edges.
- PoCW data: votes, locks.

Backends
--------
- Postgres for relational queries and durability.
- RocksDB (or similar) for high-throughput key/value paths.
- Abstract via traits to allow in-memory/mocked backends for tests.

Notes
-----
- Schema migrations tracked in repo.
- Favor append-only logs where possible; compaction for pruning.

