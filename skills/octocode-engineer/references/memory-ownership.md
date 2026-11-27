Load when choosing references, pointers or an ownership model, or comparing how languages handle ownership and concurrency.

# Memory: references and ownership

## References, pointers and ownership

| Concept | Meaning | Typical use |
|---|---|---|
| Pointer | Address of an object or region | Low-level memory access |
| Reference | Language-defined access to an object or value | APIs, object access |
| Raw pointer | Address without automatic lifetime management | Systems programming, FFI |
| Unique ownership | One owner manages the resource | Rust `Box`, C++ `unique_ptr` |
| Shared ownership | Several owners share lifetime responsibility | Rust `Rc`/`Arc`, C++ `shared_ptr` |
| Weak reference | Non-owning reference to managed data | Break ownership cycles |
| Borrowed reference | Temporary access without taking ownership | Rust borrowing |
| Immutable reference | Read-only access under language rules | Safe shared access |
| Mutable reference | Mutation through a reference | Controlled updates (Rust: exclusive) |
| Interior mutability | Mutation through an otherwise shared handle | Rust `Cell`, `RefCell`, `Mutex`, atomics |
| RAII | Release resources when the owning scope or object ends | C++ and Rust resource handling |
| Move semantics | Transfer ownership or resources | Avoid expensive copies |
| Value semantics | Independent values | Easier reasoning and isolation |
| Aliasing | Several references reach the same storage | Mutation and concurrency hazards |
| Lifetime analysis | Ensure references stay valid | Prevent dangling access |
| Pinning | Forbid moving an object | Self-referential structures, async runtimes |

## Ownership and concurrency by language

| Language | Memory management | Concurrency mechanism |
|---|---|---|
| C | Manual `malloc`/`free` | pthreads, C11 atomics |
| C++ | RAII, smart pointers, manual control | `std::thread`, mutexes, atomics |
| Rust | Ownership, borrowing, lifetimes | `Send`, `Sync`, `Arc`, `Mutex`, channels |
| Java | Garbage collection | Threads, virtual threads, concurrent collections |
| C# | Garbage collection, `IDisposable` | Tasks, async/await, locks |
| Go | Garbage collection | Goroutines, channels, mutexes |
| Python | CPython: reference counting plus a cycle collector | Threads, processes, asyncio |
| JavaScript | Garbage collection | Event loop, workers, `SharedArrayBuffer` with `Atomics` |

Garbage collection manages reclamation; it does not prevent data races. Thread-safe reference counting (`Arc`, `shared_ptr`) makes the count safe, not the object it points to.
