# HTML (`.html`) — Large-File Benchmark

**Source:** 185 lines / 7,795 bytes — Documentation page with nav, sidebar, API reference section

**Agent rating: 8.5/10 (strong)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 7,795 B | — | — |
| content-view | 5,704 B | **−26.8%** | 2.54 ms |
| applyMinification | 4,496 B | **−42.3%** | 1.99 ms |
| minifyContentSync | 4,496 B | **−42.3%** | 1.9 ms |
| minifyContent (async, type=aggressive) | 4,508 B | **−42.2%** | 6.76 ms |
| symbols | 1,955 B (−74.9%) | — | 0.67 ms |

## Notes

- HTML comments stripped by aggressive strategy
- html-minifier-terser async path for whitespace compression
