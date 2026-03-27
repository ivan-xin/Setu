//! Setu Move VM — Sui fork integration for Move smart contract execution.
//!
//! This crate is the home for all Move VM logic per ADR-6.
//! setu-runtime stays pure (G5: depends on types/ only).
//!
//! ## Module Layout (Phase 1)
//!
//! - `address_compat` — Setu ↔ Move address conversions
//! - `gas`            — InstructionCountGasMeter (27 GasMeter methods)
//! - `resolver`       — SetuModuleResolver (module loading from storage)
//! - `object_runtime` — SetuObjectRuntime (NativeContextExtensions)
//! - `natives`        — Native function implementations + registration table
//! - `engine`         — SetuMoveEngine (VM lifecycle, session, execute)

pub mod address_compat;
pub mod engine;
pub mod gas;
pub mod natives;
pub mod object_runtime;
pub mod resolver;
