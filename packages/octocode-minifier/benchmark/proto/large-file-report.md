# Protobuf (`.proto`) — Large-File Benchmark

**Source:** 198 lines / 6,808 bytes — gRPC service definition: messages, enums, service RPCs

**Agent rating: 8/10 (strong)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 6,808 B | — | — |
| content-view | 2,954 B | **−56.6%** | 0.98 ms |
| applyMinification | 2,954 B | **−56.6%** | 79.78 ms |
| minifyContentSync | 2,954 B | **−56.6%** | 8.72 ms |
| minifyContent (async, type=conservative) | 2,954 B | **−56.6%** | 1.53 ms |
| symbols | n/a (not supported) | — | — |

## Notes

- C-style and // comments stripped by conservative strategy
- High comment density in gRPC schemas
