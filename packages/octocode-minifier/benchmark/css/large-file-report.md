# CSS (`.css`) — Large-File Benchmark

**Source:** 363 lines / 10,231 bytes — Design-system tokens + components (buttons, cards, forms)

**Agent rating: 7/10 (good)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 10,231 B | — | — |
| content-view | 8,312 B | **−18.8%** | 8.16 ms |
| applyMinification | 7,082 B | **−30.8%** | 14.68 ms |
| minifyContentSync | 7,082 B | **−30.8%** | 4.46 ms |
| minifyContent (async, type=aggressive) | 7,003 B | **−31.6%** | 58.23 ms |
| symbols | 3,460 B (−66.2%) | — | 2.91 ms |

## Notes

- Aggressive strategy: CleanCSS async path
- Variable declarations compress well
