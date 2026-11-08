# Rust best practices and gotchas

A working reference for writing fast, clean Rust: memory, threads, CPU efficiency, gotchas, clean code, tooling, and crate management. Each section starts at the basics and builds up. Grounded in the Rust Performance Book, the API guidelines, and supply-chain security docs, with sources at the end.

## Memory management

- Ownership is the whole model: every value has exactly one owner. Assigning or passing by value moves it, and the old binding is dead. Functions that only read should take &T; functions that mutate in place take &mut T.
- Borrow in signatures, own in fields: fn process(text: &str) accepts String, &String, &str and slices at zero cost, while fn process(text: String) forces callers to clone or give up their value.

```rust
fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}
```

- If a returned reference's lifetime does not obviously tie to an input, return an owned value. Fighting the borrow checker with clever lifetimes usually means the API wants ownership.
- Build collections with iterators and one collect() when the size is known. Pushing in a loop grows the buffer by doubling and copying.

```rust
let squares: Vec<u32> = (0..n).map(|x| x * x).collect();
```

- Clone deliberately. A clone() added to silence the borrow checker is a hidden allocation plus copy. First try restructuring: shorter borrow scopes, split borrows of different fields, or indices.
- Cow<'_, str> fits functions that usually borrow but sometimes must own, like a normalization that only allocates when the input actually changes.
- Pre-size with Vec::with_capacity and String::with_capacity when you know the length. Every growth doubles the allocation and copies everything.
- Box<T> is for recursive types, trait objects (Box<dyn Error>), and moving large values off the stack. It is not a default wrapper: plain values are cheaper.
- Rc and Arc mean shared ownership. Rc is single-threaded; Arc is atomic and Send. Use them when ownership is genuinely shared, not as an escape hatch from borrow errors.
- Reference cycles leak. An Rc or Arc graph with back edges never frees. Break cycles with Weak, which does not keep the value alive.
- Reuse allocations on hot loops: hoist the Vec or String outside the loop and clear() each iteration. clear keeps the capacity, so the steady state allocates nothing.

```rust
let mut buf = String::with_capacity(4096);
for line in &lines {
    buf.clear();
    buf.push_str(line);
    process(&buf);
}
```

- Skip the heap for small bounded data: smallvec spills to the heap only when full, arrayvec never touches it, and bumpalo arenas batch-allocate a whole phase and free it at once.
- Zero-copy parsing: keep &str and &[u8] views into one buffer instead of allocating a String per token. The bytes crate does the same for network buffers.
- Global allocator: a last lever, measured on a profile. See [performance-and-memory.md](references/performance-and-memory.md#global-allocator-last-lever).


## Memory gotchas

- mem::forget and Box::leak are safe but leak. So do Rc and Arc cycles. Leaks are not unsafe, just wasted memory, and the compiler does not check for them.
- Large stack arrays ([u8; 1 << 20]) can overflow the stack, especially in threads (2 MiB default stack) and debug builds. Box them or use a Vec.
- Dropping a deep recursive structure, like a long linked list, can overflow the stack inside recursive Drop. Write an iterative Drop or flatten to a Vec.
- RefCell moves borrow checking to runtime: a borrow_mut() while a borrow() is alive panics. Keep RefCell borrows short and never across a call you do not control.
- Rc is not Send, so a type holding Rc cannot move to another thread. The compiler catches it; switch to Arc only where you actually need cross-thread sharing.

## Threads and concurrency

- std::thread::spawn needs 'static data: the closure must own what it uses (move ||). To borrow stack data, use scoped threads, stable since Rust 1.63.

```rust
let data = vec![1, 2, 3];
std::thread::scope(|s| {
    s.spawn(|| println!("{data:?}"));
});
```

- Share by message when you can. Channels (std::sync::mpsc, or crossbeam and flume for more features) keep locks rare and data flowing one way.
- Mutex<T> grants exclusive access, and lock() returns a Result because a panic while holding the guard poisons the mutex. Keep the guard's scope as small as possible and never hold it over heavy work.
- Before reaching for Arc<Mutex<T>>, ask whether each worker can own its data and merge results at the end. Less sharing beats better locking.
- RwLock fits read-heavy data, but writers can starve and readers may still serialize. For a concurrent map, DashMap is usually simpler and faster than RwLock<HashMap>.
- Atomics: Relaxed for plain counters, Acquire and Release to publish data between threads, SeqCst when unsure. A wrong ordering is a real bug that tests almost never catch.
- rayon for CPU-bound work: changing iter() to par_iter() gets you a work-stealing thread pool. tokio is for IO-bound concurrency: many sockets and timers, not faster CPU work.

```rust
use rayon::prelude::*;
let total: u64 = items.par_iter().map(expensive).sum();
```

- CPU-heavy work inside tokio belongs in tokio::task::spawn_blocking (or rayon). Running it on a runtime worker stalls every task on that thread.
- For global state use std::sync::OnceLock or LazyLock instead of static mut or the lazy_static crate.

## Concurrency gotchas

- Never hold a std::sync::MutexGuard across an .await: the future stops being Send and the lock can deadlock the executor. Use tokio::sync::Mutex only when the critical section truly spans an await.
- Lock ordering: if two locks are ever held at once, always acquire them in the same order everywhere. Inconsistent order is the classic deadlock.
- Poisoning: a panic while a Mutex guard is held poisons it, and every later lock() returns Err. Handle the Err, or use parking_lot::Mutex, which has no poisoning and is smaller and faster.
- Detached threads are killed when main exits, mid-work. Join the handles, or use scoped threads, so shutdown is clean.
- A channel send can block or fail once the receiver is gone; a recv loop that never sees disconnect spins or hangs. Handle disconnect explicitly in both directions.

## CPU efficiency

- Measure first, always. Benchmark with criterion or divan; profile with perf plus flamegraph, or samply for a Firefox Profiler view. Optimizing without a profile is guessing.
- Only measure release builds. Debug builds are 10 to 100 times slower and nothing about them transfers to production.

```shell
cargo bench            # criterion harnesses run optimized
cargo run --release
```

- Release profile (LTO, codegen units): see [build-profiles.md](references/build-profiles.md#release). Octocode ships thin LTO, engine codegen units at 1 and runtime at 16.

- For a fixed target machine, compile with target-cpu=native. For serious gains on fixed hardware, add PGO (cargo-pgo) and BOLT.

```shell
RUSTFLAGS="-C target-cpu=native" cargo build --release
```

- Allocations dominate most real profiles. with_capacity, buffer reuse, and removing format! from loops usually beat any clever micro-optimization.
- Iterators are zero cost: map, filter and fold compile to loop-equivalent code and often optimize better, with bounds checks elided and autovectorization. Reach for them first; verify hot paths with cargo asm.
- Indexing (v[i]) panics out of bounds and carries a bounds check. Iterators, slice patterns, and get() avoid both. get_unchecked removes the check on a proven-hot path, and it is unsafe for a reason.
- Prefer static dispatch (generics) on hot paths: dyn Trait adds indirection and blocks inlining. For a closed set of types, an enum dispatches statically.
- Batch IO: wrap files and sockets in BufReader and BufWriter. An unbuffered read of one byte at a time is a syscall per byte.
- Prevent regressions: run criterion benchmarks in CI against a baseline, or use iai and callgrind for instruction counts that do not jitter between runs.

## Gotchas cheat sheet

- Integer overflow: debug builds panic, release builds wrap silently. Use checked_add, saturating_add, or wrapping_add where the semantics matter.
- unwrap() is a panic with extra steps. This repo denies `unwrap_used` and `expect_used` (`packages/octocode-native/Cargo.toml:78-79`): use `?` with a typed error. Where a panic is truly impossible, use `#[expect(clippy::expect_used, reason = "...")]` on the item. Lint rules: [testing-and-tooling.md](references/testing-and-tooling.md#lint-ratchet-octocode-native).
- Slicing past the end panics (&s[..n]); get(..n) returns Option instead.
- NaN != NaN, so f64 has no Eq and no total order. Compare with an epsilon or use ordered-float; never use floats as HashMap keys.
- let _ = some_result discards the error and silences the must_use warning. Handle it or log it; silence here hides real failures.
- SystemTime can jump backwards (NTP adjustments). Measure durations with Instant.
- HashMap's default SipHash is DoS resistant but slow. For trusted keys use a faster hasher: FxHashMap, or hashbrown with foldhash.
- println! in a tight loop flushes line-buffered stdout on every call. Lock it once for bulk output.

```rust
let mut out = std::io::stdout().lock();
writeln!(out, "{line}")?; // in a fn returning io::Result<()>
```

- Paths are not strings: use Path and PathBuf, not String, and OsString when the OS allows non-UTF-8 names.
- The default integer type is i32; literals in generic contexts sometimes need a suffix (1u64) or an annotation.
- Copy types pass by value cheaply; large non-Copy structs are moved with a memcpy. Pass &T for anything big.
- cargo build is the dev profile: unoptimized and slow. Anything you benchmark or ship needs --release.

## Clean code

- Never leak a dependency's error type through your public API. Error types (thiserror, anyhow), `?` and conversion at boundaries: see [idioms.md](references/idioms.md#errors).
- Make invalid states unrepresentable: newtypes, enums for modes instead of booleans, and parse-don't-validate at the edges of the system.

```rust
struct UserId(u64);

fn load_user(id: UserId) -> Result<User, Error> {
    // id is known-valid here; no re-checking
}
```

- Follow the API naming conventions: as_ borrows, to_ converts and may allocate, into_ consumes, from_ constructs. Callers read your API correctly without docs.
- Keep unsafe minimal and encapsulated behind a safe API, with a // SAFETY: comment proving each invariant. Run Miri on anything unsafe.
- Document every public item (the missing_docs lint helps) and put a doctest example on the main entry points. Doctests run in CI, so examples never go stale.
- Small modules, tight visibility: internals stay private or pub(crate). A public item is a deliberate commitment.
- rustfmt formats; clippy gates. Format and lint gates for this repo: see [testing-and-tooling.md](references/testing-and-tooling.md).


## Tooling

- Toolchain, components, the tool belt and CI order: see [testing-and-tooling.md](references/testing-and-tooling.md).

## Managing crates

- Version requirements: the default caret ("1.2" means >=1.2.0, <2.0.0) is right for most cases. Avoid wildcards; pin exactly (=1.2.3) only with a documented reason.
- Commit Cargo.lock for libraries and applications alike, so builds reproduce; build with `--locked`. This repo commits `packages/octocode-native/Cargo.lock`. See [workspace.md](references/workspace.md).
- Once you have two or more related crates, use a workspace: one target directory, one lockfile. Manifest, layout and lints: see [workspace.md](references/workspace.md).

- Features must be additive: enabling a feature can add API, never remove or change behavior. Keep default features minimal and let consumers opt in.
- When consuming, default-features = false plus an explicit feature list trims your tree and build time.

```toml
tokio = { version = "1", default-features = false, features = ["rt-multi-thread", "macros"] }
```

- Set rust-version (your MSRV) in Cargo.toml and test that exact toolchain in CI, so a new language feature never breaks your users by accident.
- Audit the tree regularly: cargo tree to see it, cargo audit for vulnerabilities, cargo deny check in CI for licenses and duplicates, cargo machete for unused entries.
- Watch for typosquats and abandoned crates. The rustdecimal package, one letter off rust_decimal, shipped a CI-targeting payload. Check the publisher, downloads, and repository activity; cargo vet and crev let you reuse others' audits.
- Patch a broken upstream temporarily with [patch.crates-io] pointing at a git revision, and remove the patch when the fix is released.

```toml
[patch.crates-io]
some-crate = { git = "https://github.com/you/some-crate", rev = "abc123" }
```

- Keep build-dependencies and dev-dependencies out of the shipped graph; a heavy proc-macro in build deps still costs every clean build.

## Sources

- [The Rust Performance Book: heap allocations, profiling, build configuration](https://nnethercote.github.io/perf-book/)
- [Pragmatic Rust Guidelines (Microsoft): performance](https://microsoft.github.io/rust-guidelines/guidelines/performance/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Rust best practices (Canonical): error and panic discipline](https://canonical.github.io/rust-best-practices/error-and-panic-discipline.html)
- [Error handling in Rust (Luca Palmieri)](https://lpalmieri.com/posts/error-handling-rust/)
- [The Rust Book: smart pointers and concurrency](https://doc.rust-lang.org/stable/book/)
- [std::sync documentation: Arc, Mutex, atomics](https://doc.rust-lang.org/stable/std/sync/)
- [The Cargo Book: specifying dependencies and workspaces](https://doc.rust-lang.org/stable/cargo/reference/specifying-dependencies.html)
- [Rust Supply Chain Security Guide: manage dependencies](https://rust-secure-code.github.io/rust-supply-chain-security/dependencies.html)
- [Clippy lint list](https://doc.rust-lang.org/stable/clippy/)
- [Some mistakes Rust doesn't catch (fasterthanli.me)](https://fasterthanli.me/articles/some-mistakes-rust-doesnt-catch)
- [Rust design patterns: anti-patterns appendix](https://www.rust-patterns.com/book/33-appendix-c-anti-patterns.html)
- [Async book: sync and async interop](https://rust-lang.github.io/async-book/part-guide/sync.html)