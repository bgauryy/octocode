# Language-Specific Sins & Search Patterns

Per-language sin tables plus copy-paste detection queries. Pair with the tiered catalog in sin-catalog.md.

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

Run these against the roast target path. Add `--json` when you need exact anchors for follow-up reads.

```bash
TARGET=./src

# CAPITAL: security
npx octocode search 'password\s*=|api_key\s*=|secret\s*=|token\s*=' "$TARGET" --regex --view discovery
npx octocode search 'eval\(|new Function\(' "$TARGET" --regex --view discovery
npx octocode search 'innerHTML\s*=|dangerouslySetInnerHTML' "$TARGET" --regex --view discovery
npx octocode search 'verify\s*=\s*False|rejectUnauthorized:\s*false' "$TARGET" --regex --view discovery

# Architecture and size
npx octocode search "$TARGET" --tree --depth 2
npx octocode search "from\\s+['\\\"]\\.\\./\\.\\." "$TARGET" --regex --ext ts,tsx,js,jsx --files-only
find "$TARGET" -type f \( -name '*.ts' -o -name '*.tsx' -o -name '*.js' -o -name '*.jsx' \) -print0 | xargs -0 wc -l | sort -rn | head -20

# Type safety and error handling
npx octocode search ': any|as any|@ts-ignore' "$TARGET" --regex --ext ts,tsx --view discovery
npx octocode search '!\.' "$TARGET" --regex --ext ts,tsx --view discovery
npx octocode search 'catch\s*\([^)]*\)\s*\{\s*\}|except\s+[^:]+:\s*pass' "$TARGET" --regex --view discovery
npx octocode search 'panic!|unwrap\(\)' "$TARGET" --regex --ext rs --view discovery

# Performance and data access
npx octocode search 'readFileSync|writeFileSync' "$TARGET" --regex --view discovery
npx octocode search 'SELECT\s+\*' "$TARGET" --regex --view discovery
npx octocode search '\.forEach\(async|await\s+.*\.forEach' "$TARGET" --regex --view discovery

# Quality, frontend, and AI residue
npx octocode search 'TODO|FIXME|HACK|XXX' "$TARGET" --regex --view discovery
npx octocode search 'eslint-disable|ts-ignore|type:\s*ignore' "$TARGET" --regex --view discovery
npx octocode search '!important|z-index\s*:\s*[0-9]{4,}' "$TARGET" --regex --view discovery
npx octocode search 'console\.log|debugger|<<<<<<<|>>>>>>>' "$TARGET" --regex --view discovery
```

For raw MCP `localSearchCode`, read the host schema first and translate the regex strings into its input fields rather than copying CLI flags.
