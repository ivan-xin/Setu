Setu Solver (planned)
=====================

Purpose
-------
- Off-chain or sidecar solvers that generate candidate transfers or compute work tied to PoCW.
- Could house domain-specific logic (e.g., AI task results) whose proofs feed into transfers.

Notes
-----
- Keep interfaces narrow; produce transfers that pass DAG/TEE verification.
- Consider batching outputs to match fold windows.

