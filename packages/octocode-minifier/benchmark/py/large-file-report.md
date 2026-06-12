# Python (`.py`) — Large-File Benchmark

**Source:** 341 lines / 10,724 bytes — httpx-style sync/async client with docstrings (adapted)

**Agent rating: 6/10 (fair)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 10,724 B | — | — |
| content-view | 8,936 B | **−16.7%** | 60.88 ms |
| applyMinification | 8,936 B | **−16.7%** | 37.21 ms |
| minifyContentSync | 8,936 B | **−16.7%** | 48.51 ms |
| minifyContent (async, type=conservative) | 8,936 B | **−16.7%** | 16.55 ms |
| symbols | 4,270 B (−60.2%) | — | 2.43 ms |

## Notes

- LIMITATION: Python triple-quoted docstrings ('''...'''  /  """...""") are NOT stripped — only # line comments are removed
- Real-world Python files with heavy docstring usage show very low content-view cuts
- Symbols extraction (−60%) compensates significantly for the agent context use-case
- Fix: implement docstring-position heuristic detection in a future strategy iteration
