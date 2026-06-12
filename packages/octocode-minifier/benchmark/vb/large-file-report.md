# Visual Basic (`.vb`) — Large-File Benchmark

**Source:** 539 lines / 21,461 bytes — Customer report module with apostrophe comments and strings

**Agent rating: 3.5/10 (fair)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 21,461 B | — | — |
| content-view | 19,593 B | **−8.7%** | 8.87 ms |
| applyMinification | 19,593 B | **−8.7%** | 7.3 ms |
| minifyContentSync | 19,593 B | **−8.7%** | 8.61 ms |
| minifyContent (async, type=conservative) | 19,593 B | **−8.7%** | 6.31 ms |
| symbols | n/a (not supported) | — | — |

## Notes

- Apostrophe comments stripped with Visual Basic string awareness
- Apostrophe markers inside string literals preserved
