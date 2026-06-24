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

```bash
# CAPITAL: Security
localSearchCode pattern="password\s*=|api_key\s*=|secret\s*=|token\s*="
localSearchCode pattern="eval\(|new Function\("
localSearchCode pattern="innerHTML\s*=|dangerouslySetInnerHTML"
localSearchCode pattern="verify\s*=\s*False|rejectUnauthorized:\s*false"

# CAPITAL: Architecture
localSearchCode pattern="import.*from.*\.\/" --follow to detect cycles

# FELONY: Types & Safety
localSearchCode pattern=": any|as any" type="ts"
localSearchCode pattern="!\." type="ts"
localSearchCode pattern="catch\s*\([^)]*\)\s*\{\s*\}"
localSearchCode pattern="\bvar\s+" type="ts,js"

# FELONY: Performance
localSearchCode pattern="readFileSync|writeFileSync" type="ts"
localSearchCode pattern="SELECT \* FROM"
localSearchCode pattern="\.forEach\(async"

# CRIME: Code Quality
localSearchCode pattern="\?\s*[^:]+\?\s*[^:]+:"        # nested ternary
localSearchCode pattern="eslint-disable"
localSearchCode pattern="TODO|FIXME|HACK|XXX"
localSearchCode pattern="sleep\(|setTimeout.*await"

# CRIME: Concurrency
localSearchCode pattern="async.*\{[^}]*\}" --no-catch  # unhandled async

# CRIME: Frontend
localSearchCode pattern="!important" type="css,scss"
localSearchCode pattern="z-index:\s*\d{4,}"
localSearchCode pattern="useEffect\(\s*\(\)\s*=>"

# SLOP: AI Residue
localSearchCode pattern="In today's.*landscape|delve into|rich tapestry|meticulous|robust framework" type="md,ts,js,py"
localSearchCode pattern="I hope this helps|As an AI"

# MISDEMEANOR
localSearchCode pattern="console\.(log|debug|warn|error)"
localSearchCode pattern="<<<<<<<|>>>>>>>"
```
