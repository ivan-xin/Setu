Network (planned)
=================

Goals
-----
- Gossip for Transfer/Fold/Vote with backpressure.
- Anti-entropy sync to repair missing DAG segments or folds.
- Shard-aware routing to limit traffic.

Considerations
--------------
- Message signatures and replay protection.
- Batching and compression.
- Peer scoring / eviction to avoid byzantine spam.
- Pluggable transports (QUIC/TCP); secure channels (TLS/Noise).

