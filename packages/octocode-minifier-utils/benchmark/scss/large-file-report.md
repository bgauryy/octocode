# SCSS (`.scss`) — Large-File Benchmark

**Source:** 291 lines / 7,034 bytes — Design system: variables, mixins, button/card components

**Agent rating: 8.5/10 (strong)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 7,034 B | — | — |
| content-view | 5,050 B | **−28.2%** | 3.39 ms |
| applyMinification | 4,176 B | **−40.6%** | 2.47 ms |
| minifyContentSync | 4,176 B | **−40.6%** | 2.73 ms |
| minifyContent (async, type=aggressive) | 2,083 B | **−70.4%** | 13.83 ms |
| symbols | 3,117 B (−55.7%) | — | 0.21 ms |

## Notes

- CleanCSS async path — aggressive SCSS minification
- // and /* */ comments stripped
