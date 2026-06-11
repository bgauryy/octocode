# CSS (`.css`) — Large-File Benchmark

**Source:** 363 lines / 10,231 bytes — Design-system tokens + components (buttons, cards, forms)

**Agent rating: 7/10 (good)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 10,231 B | — | — |
| content-view | 8,312 B | **−18.8%** | 6.03 ms |
| applyMinification | 7,082 B | **−30.8%** | 5.05 ms |
| minifyContentSync | 7,082 B | **−30.8%** | 21.45 ms |
| minifyContent (async, type=aggressive) | 7,003 B | **−31.6%** | 48.37 ms |
| symbols | 3,460 B (−66.2%) | — | 0.64 ms |

## Notes

- Aggressive strategy: CleanCSS async path
- Variable declarations compress well
