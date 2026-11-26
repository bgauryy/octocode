Load when using atomic operations, compare-and-swap or lock-free code, or choosing a memory ordering.

# Concurrency: synchronization, atomics and memory ordering

## Synchronization and atomic operations

| Technique | Application | Common issue |
|---|---|---|
| Atomic load / store | Shared scalar state | Wrong memory ordering |
| CAS (compare-and-swap) | Conditional state transition | ABA problem; spurious failure of weak CAS needs a loop |
| Fetch-and-add | Atomic counters | Contention on one cache line; shard hot counters |
| Memory fence | Enforce ordering constraints | Architecture-dependent semantics |
| Acquire / release | Publish and consume state | A missing happens-before edge |
| Sequential consistency | One global order of atomic operations | Can cost extra fences |
| Double-checked locking | Lazy initialization | Needs correct publication (Java `volatile`, C++ atomics); prefer a once primitive |
| Optimistic locking | Update only if the version is unchanged; fits low-contention updates | Retry storms under contention |
| Pessimistic locking | Prevent competing updates | Blocking and deadlocks |
| Fine-grained locking | Smaller critical sections | Complexity and lock-ordering rules |
| Hazard pointers | Protect objects referenced by lock-free readers | Per-access bookkeeping |
| Epoch-based reclamation | Reclaim retired memory safely | One stalled thread delays all reclamation |

## Memory ordering

| Ordering | Guarantee | Common use |
|---|---|---|
| `relaxed` | Atomicity only, no cross-thread ordering | Independent counters |
| `consume` | Dependency ordering | Deprecated in C++26; compilers treat it as `acquire` |
| `acquire` | Later reads and writes in this thread cannot move before the load | Consume published data |
| `release` | Earlier reads and writes in this thread cannot move after the store | Publish initialized data |
| `acq_rel` | Both, on a read-modify-write | Atomic read-modify-write |
| `seq_cst` | Plus one total order over all `seq_cst` operations | Simplest reasoning; the default |

Acquire and release synchronize only when the acquire load reads the value the release store wrote; choosing the flags does not by itself make arbitrary shared data safe ([cppreference: memory_order](https://en.cppreference.com/cpp/atomic/memory_order)).
