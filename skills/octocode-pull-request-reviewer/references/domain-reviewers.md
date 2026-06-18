# Domain Reviewers Reference

## Domain Detection & Priority Matrix

| Domain | Detect | HIGH Priority | MED Priority | Skip |
|--------|--------|---------------|--------------|------|
| **Bug** | Runtime errors, logic flaws, data corruption, resource leaks, race conditions, type violations, API misuse | Crashes, data corruption, security breach, null access in hot path | Edge-case errors, uncertain race conditions | Try/catch without cleanup need, compiler-caught issues |
| **Architecture** | Pattern violations, tight coupling, circular deps, mixed concerns, leaky abstractions | Breaking public API, circular deps causing bugs | Significant pattern deviations, tech debt increase | Single-file organization, framework-standard patterns |
| **Performance** | O(n²) where O(n) possible, blocking ops, missing cache, unbatched ops, memory leaks | O(n²) on large datasets, memory leaks, blocking main thread | Moderate inefficiency in frequent paths | Negligible impact, theoretical improvements |
| **Code Quality** | Naming violations, convention breaks, visible typos, magic numbers, TODO in new code | Typos in public API/endpoints | Internal naming issues, DRY violations, convention deviations | Personal style, linter-handled formatting |
| **Duplicate Code** | Missed opportunities to leverage existing code, utilities, established patterns | Missing use of critical utilities that could prevent bugs | Code duplication violating DRY across files | Intentional duplication for clarity |
| **Error Handling** | Poor error messages, unclear logs, swallowed exceptions, missing debug context | Swallowed exceptions hiding critical failures | Unclear error messages, missing log context | Internal service calls in trusted environments |
| **Flow Impact** | How changes alter execution flows, data paths, system behavior. Use `ghSearchCode` / `lspGetSemantics(type="callers"/"callHierarchy")` to trace. | Changes that break callers, alter critical paths, change data flow semantics | Flow changes requiring updates in dependent code, altered return values/types | Internal refactors with same external behavior |

---

## Global Exclusions (NEVER Suggest)

- Compiler/TypeScript/Linter errors (tooling catches these)
- Unchanged code (no '+' prefix)
- Test implementation details (unless broken)
- Generated/vendor files
- Speculative "what if" scenarios
- Issues already raised in existing PR comments
