Load when a Rust program's RSS or heap grows, or an allocation profile must be separated from resident memory.

# Rust memory profiling

## Memory: allocations are not RSS

On a supported Linux host with Valgrind installed:

```bash
valgrind --tool=massif --massif-out-file="$out/massif.out" target/release/<binary> <args>
ms_print "$out/massif.out" > "$out/massif.txt"
```

Use Massif's allocation trees near the peak; use an installed massif visualizer for the time axis. Massif measures heap usage, with bookkeeping/alignment; stacks are optional, and it changes timing substantially. An allocator/custom arena may hide individual object allocations. For lower overhead, use an installed allocation profiler compatible with the actual allocator ([per platform](memory-allocators.md)), or Instruments Allocations on macOS. Record unsupported platforms/allocators explicitly.

Name which number grew ([churn, retention or peak](memory-profiling.md#churn-retention-or-peak): total allocated bytes, outstanding allocations or resident pages) and prove a leak with that section's slope check before claiming one. Rust ownership does not preclude retained caches, cycles in `Arc`, forgotten allocations, or unbounded queues. Inspect ownership and release paths at the measured allocation stacks; do not add manual freeing without establishing the owner.

Sources: [Massif](https://valgrind.org/docs/manual/ms-manual.html).
