API (planned)
=============

Scope
-----
- Submit transfers to the DAG builder.
- Query DAG/FoldGraph state (transfer status, fold status, votes).
- Health/metrics for nodes.

Interfaces
----------
- HTTP/JSON for external clients; gRPC for node-to-node if needed.
- Authn/Authz pluggable (API key/JWT); rate limits at ingress.
- Idempotent submission keyed by transfer id.

Open questions
--------------
- Pagination/filtering for DAG/Fold queries.
- Exposure of shard routing hints to clients.

