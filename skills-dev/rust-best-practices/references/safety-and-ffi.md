# Unsafe code, trust boundaries and FFI

Load for unsafe operations, untrusted input or foreign interfaces. The [Reference](https://doc.rust-lang.org/reference/behavior-considered-undefined.html) owns language rules; the [Rustonomicon](https://doc.rust-lang.org/nomicon/) explains unsafe design. Neither supplies a complete formal model for every aliasing question.

## Unsafe obligations

1. Establish why safe Rust or an existing safe API cannot meet the requirement. Unsafe is not a fix for an unexplained borrow error or an unmeasured optimization.
2. Enumerate preconditions: initialization, validity, provenance, alignment, allocation bounds, aliasing, lifetimes, ownership and thread affinity as applicable.
3. Keep the unsafe operation small and explain with `// SAFETY:` why its preconditions hold at this site. An `unsafe fn` needs a public `# Safety` contract; use explicit unsafe blocks inside it. Audit `unsafe impl`, including `Send`/`Sync`, with the same care.
4. Ensure safe callers cannot violate those conditions, including on errors, panics and cancellation. A safe wrapper is sound only if all permitted safe calls uphold the contract.
5. Run targeted tests and supported Miri checks. Add fuzzing, concurrency modeling or platform/FFI tests where the risk requires them. Report unsupported execution paths.

Respect existing lint policy. A safe-only crate may forbid unsafe; an FFI crate needs narrow audited unsafe sites. Useful selected lints include `unsafe_op_in_unsafe_fn` and `clippy::undocumented_unsafe_blocks`; do not replace a safety argument with a lint count.

**Miri is a detector, not a soundness proof.** It exercises specific executions, has incomplete nondeterministic coverage and limited platform/FFI support. Do not promise that a whole FFI crate can run under Miri. Isolate the pure-Rust invariant where possible and retain real platform tests. Use the project's supported nightly/component selection rather than silently changing its toolchain.

## Untrusted input and resources

| Boundary | Required decision |
|---|---|
| Lengths, counts and arithmetic | Validate ranges before allocation/conversion. Use checked arithmetic for rejection; saturation only when saturation is the intended meaning. Overflow behavior depends on compiler/profile settings. |
| Memory, queues and concurrency | Follow the owner's resource policy. Use backpressure, streaming, pagination or explicit refusal; do not invent a fixed cap that silently drops valid work. `try_reserve` helps only for allocations using that fallible path. |
| Text and serialization | Define byte/encoding, depth and extension-field rules. Reject unknown fields only for a closed contract. Keep parser protections unless an equivalent policy replaces them. |
| Regex | Inspect the selected engine's complexity and controls. `regex` searches depend on both pattern and input size; repeated searches can cost more. Backtracking engines need their own work controls. |
| Filesystem | Match the threat model. A canonicalize/check/open sequence can race symlink replacement; use handle-relative/capability access when adversarial mutation is in scope. |
| Network destinations | Enforce destination policy across resolution, redirects and connection; URL parsing alone does not prevent SSRF. |
| Diagnostics | Keep credentials and sensitive data out of errors, logs, snapshots and derived `Debug`. Use established redaction/secret-handling facilities. |

A policy limit must name the refused or unread scope. A bounded read that stops before EOF is not a complete file. Avoid unchecked narrowing casts and do not rely on debug overflow behavior in release.

## FFI contracts

- Choose and document the ABI, representation, ownership transfer, allocator/free pairing, pointer validity and callback/thread lifetime. `repr(C)` does not make Rust-owned containers or arbitrary Rust types a stable foreign API.
- The allocator/library that creates a resource must release it through the matching API. Reconstruct `Box`/`CString` from raw pointers only with the exact ownership preconditions and at most once.
- Keep a borrowed pointer valid for the full foreign use; account for callbacks after the original call and shutdown races. Copying into owned data is often the simpler boundary.
- A Rust panic reaching a non-unwinding ABI aborts; foreign unwinding entering Rust there is undefined behavior. `C-unwind` permits unwinding only with a compatible contract. `catch_unwind` does not catch aborts and is not general foreign-exception recovery.
- When containment is required, catch only within an established boundary, convert to a defined failure and avoid reusing potentially invalid state. Respect the existing panic strategy; `panic = "abort"` terminates the process, including a hosting application.
- Verify exported signatures and the built artifact from a real consumer. For napi-rs-specific details, read [napi](napi.md).

## Dependency execution

A build script or proc macro executes during the build. Inspect new build-time dependencies, native libraries and sources with that in mind. Use the repository's advisory/license/source checks; an unsafe count, popularity ranking or successful advisory scan does not establish safety.

Sources: [Reference: undefined behavior](https://doc.rust-lang.org/reference/behavior-considered-undefined.html), [explicit unsafe blocks](https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html), [Miri limitations](https://github.com/rust-lang/miri/blob/master/README.md), [FFI unwinding](https://doc.rust-lang.org/nomicon/ffi.html#ffi-and-unwinding), [regex untrusted inputs](https://docs.rs/regex/latest/regex/#untrusted-inputs).
