# Rust (`.rs`) — Large-File Benchmark

**Source:** 325 lines / 9,961 bytes — Async task runtime — doc comments, unsafe blocks, generics

**Agent rating: 7.5/10 (good)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 9,961 B | — | — |
| content-view | 6,656 B | **−33.2%** | 2.24 ms |
| applyMinification | 6,656 B | **−33.2%** | 3.1 ms |
| minifyContentSync | 6,656 B | **−33.2%** | 2.32 ms |
| minifyContent (async, type=conservative) | 6,656 B | **−33.2%** | 3.59 ms |
| symbols | 1,911 B (−80.8%) | — | 1.19 ms |

## Notes

- //! and /// doc comments stripped by c-style remover
- Aggressive savings from Rustdoc
