/// Standard vector module — native operations provided by the Move VM.
///
/// This is a minimal subset of move-stdlib's vector module,
/// providing only the essential native operations that the VM supports
/// as bytecode instructions.
module std::vector {
    /// Create an empty vector.
    native public fun empty<Element>(): vector<Element>;

    /// Return the length of the vector.
    native public fun length<Element>(v: &vector<Element>): u64;

    /// Acquire an immutable reference to the `i`th element of the vector.
    /// Aborts if `i` is out of bounds.
    native public fun borrow<Element>(v: &vector<Element>, i: u64): &Element;

    /// Add element `e` to the end of the vector.
    native public fun push_back<Element>(v: &mut vector<Element>, e: Element);

    /// Acquire a mutable reference to the `i`th element of the vector.
    /// Aborts if `i` is out of bounds.
    native public fun borrow_mut<Element>(v: &mut vector<Element>, i: u64): &mut Element;

    /// Pop an element from the end of vector.
    /// Aborts if the vector is empty.
    native public fun pop_back<Element>(v: &mut vector<Element>): Element;

    /// Destroy the vector. Aborts if it is not empty.
    native public fun destroy_empty<Element>(v: vector<Element>);

    /// Swap the `i`th and `j`th elements in the vector.
    native public fun swap<Element>(v: &mut vector<Element>, i: u64, j: u64);
}
