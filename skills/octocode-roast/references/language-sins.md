# Language-Specific Sins & Search Patterns

Per-language sin tables plus copy-paste detection queries. Pair with the tiered catalog in sin-catalog.md. These are leads; cite only after exact code evidence proves impact and confidence.

---

## Language-Specific Sins

### TypeScript/JavaScript

| Sin | Pattern | Roast |
|-----|---------|-------|
| `any` overuse | `: any` | "TypeScript asked for a divorce." |
| `@ts-ignore` abuse | `@ts-ignore` | "Silencing the type checker. Very mature." |
| Prototype pollution | `obj[userInput] =` | "Prototype pollution vector. `__proto__` says hello." |

### Python

| Sin | Pattern | Roast |
|-----|---------|-------|
| `except: pass` | `except:` with `pass` | "Catching literally everything and doing nothing. Peak nihilism." |
| `import *` | `from x import *` | "`import *` — Who knows what's in scope? Surprise!" |
| Mutable default args | `def fn(x=[])` | "Mutable default argument. Classic Python trap." |

### React

| Sin | Pattern | Roast |
|-----|---------|-------|
| Missing key prop | `map` without `key` | "Missing key prop. React is confused. So am I." |
| State in render | `useState` in conditions | "Conditional hooks. React's rules? More like guidelines." |
| Stale closure | useEffect/useCallback deps | "Stale closure detected. Your state is living in the past." |

### SQL/Database

| Sin | Pattern | Roast |
|-----|---------|-------|
| `SELECT *` | `SELECT *` | "`SELECT *` — Because bandwidth is free, right?" |
| No indexes hint | Large table scans | "Full table scan. Your DBA just felt a disturbance in the force." |
| String concatenation | `"SELECT..." + var` | "SQL injection delivery mechanism activated." |

---

## Search Patterns

Use these as pattern families for `octocode-research`; do not run or document Octocode research commands here. Exclude docs, examples, fixtures, generated files, and tests before ranking unless the user asked to roast those surfaces.

| Category | Patterns |
|---|---|
| Security | `password\s*=`, `api_key\s*=`, `secret\s*=`, `token\s*=`, `eval\(`, `new Function\(`, `innerHTML\s*=`, `dangerouslySetInnerHTML`, `verify\s*=\s*False`, `rejectUnauthorized:\s*false` |
| Architecture and size | parent-directory import climbs, unusually large files, dense directories, high fan-in/fan-out |
| Type safety and error handling | `: any`, `as any`, `@ts-ignore`, non-null assertions, empty catches, `except ... pass`, `panic!`, `unwrap()` |
| Performance and data access | sync file I/O, `SELECT *`, async `forEach`, blocking calls in hot paths |
| Quality, frontend, and residue | `TODO`, `FIXME`, `HACK`, `XXX`, disables, large `z-index`, `!important`, `console.log`, `debugger`, merge markers |

Ask `octocode-research` to upgrade any match into exact evidence before the roast cites it. If the evidence only proves style or taste, demote it to Slop or Misdemeanor.
