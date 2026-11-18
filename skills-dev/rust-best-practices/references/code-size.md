# Compile cost of generic and generated code

Load when `--timings` shows a crate whose frontend (expansion, type and borrow checking) or codegen dominates, when generated types or derives are heavy, or when futures are large. Each change below keeps behavior and cuts the code the compiler must check or generate. Measure before and after: the numbers come from one 690k-line workspace ([provenance](sources-and-crates.md#refinement-provenance)), and an IR-line saving is a proxy until CPU confirms it.

## Measure first

| Question | Tool |
|---|---|
| Which generic functions produce the most LLVM IR, and in how many copies | [`cargo llvm-lines`](https://github.com/dtolnay/cargo-llvm-lines) on the crate's lib or lib-test |
| How large are futures and other types | nightly `-Zprint-type-sizes` ([type sizes](https://nnethercote.github.io/perf-book/type-sizes.html)) |
| Which compiler pass dominates; how much code macros expand to | nightly `-Ztime-passes` and `-Zmacro-stats` on the one measured crate |
| Did the change pay | rustc CPU (user + sys) of that unit, A/B/A/B ([diagnose](build-profiles.md#diagnose-before-changing-flags)) |

Use a nightly toolchain for the measurement only; do not change the project's pinned toolchain.

## Shrink monomorphized code

Each instantiation of a generic function with new type arguments is compiled again, so a generic helper used with many types multiplies codegen.

| Pattern | Change | Measured here |
|---|---|---|
| A generic container instantiated for many key and value types (a memo or cache) | A non-generic core over type-erased values (`Arc<dyn Any + Send + Sync>`) behind a thin typed façade with the same API and bounds | −212k IR lines in the largest crate and −25k in the crate below it; the largest crate's CPU 37.0 → 35.8 s (−3.2%) |
| A recursive walker generic over its callback | Take `&mut dyn FnMut`; keep the public generic signature as an adapter that calls it | −52k and −22k IR lines |
| An `async fn` wrapper that awaits a future it is given (a budget wait, a timeout) | Make it a plain `fn` returning the future, and `Box::pin` the wrapped operation once. Each async wrapper stores the inner future in its own state, so stacked wrappers multiply the size. | An authentication future 21,432 → 2,288 bytes. Futures of 16 KiB or more: 11 → 0 in one crate, 56 → 30 in another. |
| A `BTreeMap` whose order nothing observes | `HashMap` (check that no reader iterates in order and nothing serializes it) | −14.7k IR lines; `BTreeMap` internals were 540k IR lines in 10,087 instantiations |
| Many closure sorts (`sort_by*`) | `Ord` on the row type and `.sort()`, or shared named comparators | Not applied yet: 118 call sites produced 194 monomorphized sorts, 408k IR lines |

The first four together cut IR by 5.2% in the largest crate and 4.1% in the crate below it. Prefer a concrete or `dyn` interface where the call is not hot; keep a generic where a benchmark shows the specialization matters.

## Generated code

| Change | Because (measured) |
|---|---|
| Deduplicate structurally identical generated types in the generator, giving each shape one name, and update the hand-written callers | 679 of 1,035 enums and 268 of 750 structs were duplicates. The generated file went from 78k to 38k lines, and the crate from 68 to 32 CPU s. |
| Derive by role: emit `Deserialize` only on types an input reaches or hand-written code names, without listing them by hand | 732 → 387 derives: crate CPU 18.0 and 18.3 → 11.0 and 10.5 s (−41%), peak memory 1.56–1.69 → 0.99–1.10 GB |
| Put the few generated types a lower crate needs in a leaf crate that the main generated crate re-exports, so each type keeps one definition | The lower crate started at 0.4 s instead of 17.2 s in `--timings` |
| Generate ahead of time and check the output in, rather than a proc macro expanding on every build | oxc and swc do this to cut compile time ([crate structure](crate-structure.md#clean-crate-architecture)) |

Derives were 70% of that crate's expanded code before this change, and type and borrow checking took 15 of its 20.5 s.

## Macro volume in tests

In the largest lib-test unit, 9,595 `json!` calls expanded to 475k lines, 41% of its 1.16M expanded lines (the library expands to 108k). Its 5,944 test attributes expanded to 354k lines, and its 13,359 `assert_eq!` calls to 158k. Literal JSON belongs in data: a raw string parsed at test time through one `#[track_caller]` helper, or a fixture file. Keep `json!` where values are interpolated, and use table-driven tests for repeated call-and-assert shapes ([testing](testing-and-tooling.md#write-trustworthy-tests)). These volumes are measured; their CPU effect is not yet.

Sources: [Rust Performance Book: compile times](https://nnethercote.github.io/perf-book/compile-times.html), [type sizes](https://nnethercote.github.io/perf-book/type-sizes.html), [cargo-llvm-lines](https://github.com/dtolnay/cargo-llvm-lines).
