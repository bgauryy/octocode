Load when a retainer path ends at a reference whose kind decides the fix, or a native program reads freed, uninitialized or out-of-bounds memory.

# References, pointers and sanitizers

## References and pointers

| Kind | Keeps the target alive | Typical failure | How it shows |
|---|---|---|---|
| Strong reference | Yes | Forgotten owner: listener, cache, global map, closure capture, pooled thread-local | Retainer path ends at that owner |
| Weak reference (`WeakRef`, `WeakMap`, Java `WeakReference`, Python `weakref`, Rust `Weak`) | No | Used as a cache: entries vanish at the next GC | Low hit rate that varies with GC timing |
| Soft reference (JVM) | Until memory pressure | Heap stays full, then empties in a burst | Long GC cycles before the clear |
| Phantom / `FinalizationRegistry` / `Cleaner` | No; notifies after collection | Cleanup that runs late or never | Native resources outlive their wrappers |
| Reference counting (`Rc`, `Arc`, `shared_ptr`, Swift ARC, CPython) | While the count is above zero | Cycles never reach zero (CPython's cycle collector reclaims them; `Rc`, `shared_ptr` and ARC do not) | Count stays above zero after the last user is gone; fix with a weak back-reference |
| Raw pointer or borrowed reference | No | Dangling: the target was freed or moved | Use-after-free, garbage values, crashes far from the bug |

Native pointer bugs and the tool that catches each:

| Bug | Tool |
|---|---|
| Use-after-free, double free, heap or stack overflow | AddressSanitizer (`-fsanitize=address -g`), about 2× slower |
| Leak at exit | LeakSanitizer (with ASan on Linux), macOS `leaks --atExit -- ./prog` |
| Uninitialized read | MemorySanitizer (Clang, Linux) or Valgrind Memcheck |
| Undefined behaviour in arithmetic, alignment, casts | UBSan (`-fsanitize=undefined`) |
| Iterator or reference invalidated by container growth | Debug containers (`_GLIBCXX_DEBUG`, libc++ hardening), ASan |
| Unsafe Rust aliasing and provenance | Miri (`cargo +nightly miri test`) |

Sanitizers catch only the paths the run executes; drive them with the failing input and tests or fuzzing that reach the risky code.
