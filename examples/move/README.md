# Move Example Contracts

Example Move contracts for the Setu network. Each demonstrates different aspects of the setu-framework.

## Contracts

| Contract | Demonstrates |
|----------|-------------|
| [counter](counter/) | Owned objects, mutation via entry functions, transfer |
| [custom_token](custom_token/) | Custom fungible token (Coin\<GOLD\>), TreasuryCap, mint/burn |
| [nft](nft/) | Unique NFT objects, freeze (immutable), burn (delete) |
| [lightning](lightning/) | Payment channel (Sui contract adaptation), Balance lifecycle, parallel-vector patterns |

## Prerequisites

These contracts depend on `setu-framework` (the Setu stdlib at address `0x1`).

## Building

Move contracts are compiled and published as bytecode via the `/api/v1/move/publish` endpoint.
See the [Move Developer Guide](../../docs/guide/move-developer-guide.md) for compilation and deployment instructions.

## Framework API Reference

The contracts above use these stdlib modules:

- `setu::object` — UID lifecycle: `new()`, `delete()`, `uid_to_address()`
- `setu::transfer` — Ownership: `transfer()`, `freeze_object()`, `share_object()` (Phase 5+)
- `setu::tx_context` — Context: `sender()`, `tx_hash()`, `epoch()`, `derive_id()`
- `setu::coin` — Fungible tokens: `mint()`, `burn()`, `split()`, `join()`, `transfer()`
- `setu::balance` — Balance arithmetic: `value()`, `split()`, `join()`, `zero()`
- `setu::setu` — Native SETU token type identifier

## Limitations (Phase 0-4)

- **No shared objects** — `share_object()` aborts with `E_SHARED_NOT_SUPPORTED`. All objects must be owned.
- **Immutable packages** — Published modules cannot be upgraded (ADR-4).
- **No on-chain events** — Move `emit()` is not available yet.
