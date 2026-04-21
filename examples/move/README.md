# Move Example Contracts

Example Move contracts for the Setu network. Each demonstrates different aspects of the setu-framework.

## Contracts

| Contract | Demonstrates |
|----------|-------------|
| [counter](counter/) | Owned objects, mutation via entry functions, transfer |
| [custom_token](custom_token/) | Custom fungible token (Coin\<GOLD\>), TreasuryCap, mint/burn |
| [nft](nft/) | Unique NFT objects, freeze (immutable), burn (delete) |
| [lightning](lightning/) | Payment channel (Sui contract adaptation), Balance lifecycle, parallel-vector patterns |
| [pwoo_counter](pwoo_counter/) | Shared object + PWOO concurrent writes (single hotspot slot) |
| [df_registry](df_registry/) | Dynamic fields end-to-end (`add` / `remove` / `borrow` / `borrow_mut` / `exists_`) |
| [dex_pool](dex_pool/) | DF as a PWOO hotspot mitigation — per-pair liquidity under one shared pool |
| [bad_df](bad_df/) | Negative case: `V: key` rejected at DF native entry |

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
- `setu::dynamic_field` — Per-key child state: `add()`, `remove()`, `borrow()`, `borrow_mut()`, `exists_()` (Phase 8)

## Limitations

- **Immutable packages** — Published modules cannot be upgraded (ADR-4).
- **No on-chain events** — Move `emit()` is a placeholder; surface signals via object fields.
- **DF value ability** — `dynamic_field::*<K, V>` rejects any `V` that has the `key` ability (object-typed DF is not supported; see [docs/feat/dynamic-fields/design.md](../../docs/feat/dynamic-fields/design.md) §1.5).
- **DF pre-declaration** — Every DF access must be listed in `MoveCallPayload.dynamic_field_accesses`; the Move VM aborts `E_DF_NOT_PRELOADED` otherwise. See the handbook §6.6.
