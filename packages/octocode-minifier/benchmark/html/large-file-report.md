# HTML (`.html`) — Large-File Benchmark

**Source:** 185 lines / 7,795 bytes — Documentation page with nav, sidebar, API reference section

**Agent rating: 8.5/10 (strong)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 7,795 B | — | — |
| content-view | 5,704 B | **−26.8%** | 10.94 ms |
| applyMinification | 4,496 B | **−42.3%** | 6.64 ms |
| minifyContentSync | 4,496 B | **−42.3%** | 1.66 ms |
| minifyContent (async, type=aggressive) | 4,508 B | **−42.2%** | 35.43 ms |
| symbols | 1,955 B (−74.9%) | — | 2.09 ms |

## Notes

- HTML comments stripped by aggressive strategy
- html-minifier-terser async path for whitespace compression
