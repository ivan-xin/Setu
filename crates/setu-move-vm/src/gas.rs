//! InstructionCountGasMeter — Phase 1 minimal gas metering.
//!
//! Counts each instruction as 1 gas unit. Prevents infinite loops
//! and excessive recursion without economic precision (Phase 3+).
//!
//! v3.8 R8-2: Based on POC-6 audit against Sui mainnet-v1.66.2.
//! All 27 GasMeter trait methods verified to compile.

use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    gas_algebra::{InternalGas, NumArgs, NumBytes},
    language_storage::ModuleId,
    vm_status::StatusCode,
};
use move_vm_types::{
    gas::{GasMeter, SimpleInstruction},
    views::{TypeView, ValueView},
};

/// Instruction-counting gas meter for Phase 1.
///
/// Each bytecode instruction costs 1 gas unit (except calls = 10).
/// No `gas_used()` in GasMeter trait — use `instructions_executed()`.
pub struct InstructionCountGasMeter {
    max_instructions: u64,
    instructions_executed: u64,
}

impl InstructionCountGasMeter {
    pub fn new(max_instructions: u64) -> Self {
        Self {
            max_instructions,
            instructions_executed: 0,
        }
    }

    pub fn instructions_executed(&self) -> u64 {
        self.instructions_executed
    }

    fn charge(&mut self, cost: u64) -> PartialVMResult<()> {
        self.instructions_executed += cost;
        if self.instructions_executed > self.max_instructions {
            Err(PartialVMError::new(StatusCode::OUT_OF_GAS))
        } else {
            Ok(())
        }
    }
}

impl GasMeter for InstructionCountGasMeter {
    // (1) Simple instruction: 1 unit
    fn charge_simple_instr(&mut self, _instr: SimpleInstruction) -> PartialVMResult<()> {
        self.charge(1)
    }

    // (2) Pop: free
    fn charge_pop(&mut self, _popped_val: impl ValueView) -> PartialVMResult<()> {
        Ok(())
    }

    // (3) Function call: 10 units
    fn charge_call(
        &mut self,
        _module_id: &ModuleId,
        _func_name: &str,
        _args: impl ExactSizeIterator<Item = impl ValueView>,
        _num_locals: NumArgs,
    ) -> PartialVMResult<()> {
        self.charge(10)
    }

    // (4) Generic function call: 10 units (replaces old charge_call_native)
    fn charge_call_generic(
        &mut self,
        _module_id: &ModuleId,
        _func_name: &str,
        _ty_args: impl ExactSizeIterator<Item = impl TypeView>,
        _args: impl ExactSizeIterator<Item = impl ValueView>,
        _num_locals: NumArgs,
    ) -> PartialVMResult<()> {
        self.charge(10)
    }

    // (5) Load constant: 1 unit
    fn charge_ld_const(&mut self, _size: NumBytes) -> PartialVMResult<()> {
        self.charge(1)
    }

    // (6) Constant after deserialization: free (already charged in ld_const)
    fn charge_ld_const_after_deserialization(
        &mut self,
        _val: impl ValueView,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    // (7) Copy local: 1 unit (Phase 1 flat cost; legacy_abstract_memory_size not available)
    fn charge_copy_loc(&mut self, _val: impl ValueView) -> PartialVMResult<()> {
        self.charge(1)
    }

    // (8) Move local: 1 unit
    fn charge_move_loc(&mut self, _val: impl ValueView) -> PartialVMResult<()> {
        self.charge(1)
    }

    // (9) Store local: 1 unit
    fn charge_store_loc(&mut self, _val: impl ValueView) -> PartialVMResult<()> {
        self.charge(1)
    }

    // (10) Pack struct: max(1, fields)
    fn charge_pack(
        &mut self,
        _is_generic: bool,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.charge(std::cmp::max(1, args.len() as u64))
    }

    // (11) Unpack struct: max(1, fields)
    fn charge_unpack(
        &mut self,
        _is_generic: bool,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.charge(std::cmp::max(1, args.len() as u64))
    }

    // (12) Enum variant switch: 1 unit
    fn charge_variant_switch(&mut self, _val: impl ValueView) -> PartialVMResult<()> {
        self.charge(1)
    }

    // (13) Read reference: 1 unit
    fn charge_read_ref(&mut self, _val: impl ValueView) -> PartialVMResult<()> {
        self.charge(1)
    }

    // (14) Write reference: 1 unit
    fn charge_write_ref(
        &mut self,
        _new_val: impl ValueView,
        _old_val: impl ValueView,
    ) -> PartialVMResult<()> {
        self.charge(1)
    }

    // (15) Equality: 1 unit (Phase 1 flat; legacy_abstract_memory_size not available)
    fn charge_eq(
        &mut self,
        _lhs: impl ValueView,
        _rhs: impl ValueView,
    ) -> PartialVMResult<()> {
        self.charge(1)
    }

    // (16) Inequality: 1 unit
    fn charge_neq(
        &mut self,
        _lhs: impl ValueView,
        _rhs: impl ValueView,
    ) -> PartialVMResult<()> {
        self.charge(1)
    }

    // (17) Vec pack: max(1, elems)
    fn charge_vec_pack<'a>(
        &mut self,
        _ty: impl TypeView + 'a,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.charge(std::cmp::max(1, args.len() as u64))
    }

    // (18) Vec len: 1 unit
    fn charge_vec_len(&mut self, _ty: impl TypeView) -> PartialVMResult<()> {
        self.charge(1)
    }

    // (19) Vec borrow: 1 unit
    fn charge_vec_borrow(
        &mut self,
        _is_mut: bool,
        _ty: impl TypeView,
        _is_success: bool,
    ) -> PartialVMResult<()> {
        self.charge(1)
    }

    // (20) Vec push back: 2 units
    fn charge_vec_push_back(
        &mut self,
        _ty: impl TypeView,
        _val: impl ValueView,
    ) -> PartialVMResult<()> {
        self.charge(2)
    }

    // (21) Vec pop back: 1 unit
    fn charge_vec_pop_back(
        &mut self,
        _ty: impl TypeView,
        _val: Option<impl ValueView>,
    ) -> PartialVMResult<()> {
        self.charge(1)
    }

    // (22) Vec unpack: max(1, num_elements) — 3-param signature (v3.8 R8-2)
    fn charge_vec_unpack(
        &mut self,
        _ty: impl TypeView,
        expect_num_elements: NumArgs,
        _elems: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.charge(std::cmp::max(1, u64::from(expect_num_elements)))
    }

    // (23) Vec swap: 2 units
    fn charge_vec_swap(&mut self, _ty: impl TypeView) -> PartialVMResult<()> {
        self.charge(2)
    }

    // (24) Native function gas: amount reported by native
    // v3.8: u64::from(amount) — not into_inner()
    fn charge_native_function(
        &mut self,
        amount: InternalGas,
        _ret_vals: Option<impl ExactSizeIterator<Item = impl ValueView>>,
    ) -> PartialVMResult<()> {
        self.charge(u64::from(amount))
    }

    // (25) Pre-native check: free
    fn charge_native_function_before_execution(
        &mut self,
        _ty_args: impl ExactSizeIterator<Item = impl TypeView>,
        _args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    // (26) Drop frame: 1 unit
    fn charge_drop_frame(
        &mut self,
        _locals: impl Iterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.charge(1)
    }

    // (27) Remaining gas
    fn remaining_gas(&self) -> InternalGas {
        InternalGas::new(self.max_instructions.saturating_sub(self.instructions_executed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gas_meter_new() {
        let meter = InstructionCountGasMeter::new(1000);
        assert_eq!(meter.instructions_executed(), 0);
        assert_eq!(u64::from(meter.remaining_gas()), 1000);
    }

    #[test]
    fn test_gas_meter_charge() {
        let mut meter = InstructionCountGasMeter::new(100);
        assert!(meter.charge(50).is_ok());
        assert_eq!(meter.instructions_executed(), 50);
        assert_eq!(u64::from(meter.remaining_gas()), 50);
    }

    #[test]
    fn test_gas_meter_out_of_gas() {
        let mut meter = InstructionCountGasMeter::new(10);
        assert!(meter.charge(11).is_err());
    }

    #[test]
    fn test_gas_meter_exact_limit() {
        let mut meter = InstructionCountGasMeter::new(10);
        assert!(meter.charge(10).is_ok());
        assert_eq!(u64::from(meter.remaining_gas()), 0);
        assert!(meter.charge(1).is_err());
    }

    #[test]
    fn test_remaining_gas_arithmetic() {
        let meter = InstructionCountGasMeter::new(0);
        assert_eq!(u64::from(meter.remaining_gas()), 0);
    }
}
