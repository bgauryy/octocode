# Rust correctness refresher

Load for an initial overview or an ownership/correctness misconception. Use the topic references for the actual change; this page is not a second rulebook.

| Question | Working model | Next reference |
|---|---|---|
| Who owns the value? | A move transfers ownership; a `Copy` value is copied. Borrow for access, own when retaining or producing data. | [Idioms](references/idioms.md) |
| Does this need shared ownership? | `Rc` shares on one thread; `Arc` shares across threads only with the necessary `Send`/`Sync` bounds. Neither prevents cycles or synchronizes inner mutation. | [Types](references/types-and-patterns.md) |
| Can the operation fail normally? | Use `Result`; distinguish absence with `Option`. Panics represent bugs/invariant failures, subject to project policy. | [Idioms](references/idioms.md) |
| Is there shared mutable state? | Prefer independent ownership where practical. Choose locks/channels by the access pattern, with explicit shutdown. | [Concurrency](references/concurrency-and-async.md) |
| Is the boundary untrusted or unsafe? | Validate its contract and prove each unsafe precondition. Compilation alone does not establish soundness. | [Safety](references/safety-and-ffi.md) |
| Is the code slow? | Reproduce with representative input and the deployed configuration, then profile and compare. | [Performance](references/performance-and-memory.md) |

## Correct common misconceptions

| Misconception | Correction |
|---|---|
| Every clone allocates. | `Clone` is type-specific: `Arc::clone` increments a count; cloning a `String` copies its data. Inspect the type and cost. |
| A `Vec` always doubles. | Growth strategy is unspecified. Capacity, initialized length and amortized `push` guarantees are distinct. |
| Iterators or `get()` eliminate all bounds checks. | `get()` checks bounds and returns `Option`; optimization depends on the code. Choose clear traversal and measure hot paths. |
| `Arc<RefCell<T>>` makes `T` thread-safe. | Atomic reference counting does not add synchronization to `T`; the required auto-trait bounds still apply. |
| A detached handle cancels its work. | Dropping a Tokio `JoinHandle` detaches. A started blocking job cannot be aborted. A dropped child-process handle can leave the child running. |
| `SeqCst` fixes a concurrent algorithm. | Ordering is part of a synchronization proof, not a substitute for one. |
| Green Miri tests prove an API sound. | They check particular executions; soundness must hold for every permitted safe caller. |
| Latest stable documentation defines the supported syntax. | The crate's edition, MSRV, target and dependency versions determine what it can use. |
| A fixed cap makes a prefix complete. | Report a limit or provide a continuation; distinguish selected output from unread data. |

Sources: [Vec guarantees](https://doc.rust-lang.org/std/vec/struct.Vec.html#guarantees), [Arc thread safety](https://doc.rust-lang.org/std/sync/struct.Arc.html#thread-safety), [Tokio tasks](https://docs.rs/tokio/latest/tokio/task/), [Miri limitations](https://github.com/rust-lang/miri/blob/master/README.md). Resolve dependency documentation to the locked version before applying an API.
