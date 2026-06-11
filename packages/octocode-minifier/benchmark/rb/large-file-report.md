# Ruby (`.rb`) — Large-File Benchmark

**Source:** 201 lines / 6,781 bytes — ActiveRecord User model with validations, scopes, auth

**Agent rating: 9/10 (excellent)**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | 6,781 B | — | — |
| content-view | 3,027 B | **−55.4%** | 2.59 ms |
| applyMinification | 3,027 B | **−55.4%** | 5.77 ms |
| minifyContentSync | 3,027 B | **−55.4%** | 1 ms |
| minifyContent (async, type=conservative) | 3,027 B | **−55.4%** | 0.98 ms |
| symbols | 488 B (−92.8%) | — | 0.25 ms |

## Notes

- Hash comments and inline annotations stripped
- Schema comment header removed
