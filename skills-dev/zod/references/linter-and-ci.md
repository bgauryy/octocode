Load when automating Zod checks in lint or CI.

# Linting and CI Rules for Zod

## Overview

Mechanical enforcement catches mistakes before code review. Use ESLint rules, CI checks, and static analysis tools to enforce Zod best practices automatically.

## ESLint: `no-restricted-syntax` Rules

### Choose the parsing contract

Use `safeParse()` for expected validation failures and `parse()` where throwing is intentional. A selector matching every `.parse()` also catches unrelated APIs such as `JSON.parse`; do not apply a blanket method-name ban. Any custom lint must resolve Zod bindings and preserve intentionally throwing call sites. Test it against both Zod and unrelated parsers before enabling it.

### Flag candidate calls, then resolve their owners

A syntax selector can identify a property name, but cannot prove it belongs to Zod. Scope rules to resolved Zod imports/schema bindings; include unrelated methods, aliases and intentional exceptions in lint tests. Do not disable every syntax rule for an entire env file.

| Candidate | Check before reporting | Remediation |
|---|---|---|
| `reportInput: true` | Is real or secret data being reported? | Omit input reporting; synthetic fixture inspection may be intentional |
| `.nativeEnum(...)` | Is this Zod v4? | Prefer `z.enum(...)`; the old function remains exported |
| `z.string().email()` | Is this the resolved Zod constructor? | Prefer `z.email()` for new v4 code |
| `.parse(...)` | Does the caller intend a throwing contract? | Suggest `safeParse` only for expected validation failures handled as results |

These are candidate checks, not a method-name ban. [Parsing](../rules/parse-use-safeParse.md) and [input privacy](../rules/error-input-security.md) own the decisions.

## CI: Schema Snapshot Regression

Use snapshots when consumers need review of the exported contract. They cover representable JSON Schema, not arbitrary refinements or transform behavior; retain behavioral tests. Review the expected snapshot filenames too, so removing a schema cannot leave a stale snapshot unnoticed.

### Setup

```typescript
// scripts/export-schemas.ts
import { z } from "zod"
import { writeFileSync, mkdirSync, readdirSync } from "fs"
import { UserSchema, OrderSchema } from "../src/api/schemas"

const schemas = {
  User: UserSchema,
  Order: OrderSchema,
}

mkdirSync("snapshots", { recursive: true })

const expectedFiles = new Set(Object.keys(schemas).map((name) => `${name}.json`))
const staleFiles = readdirSync("snapshots").filter(
  (file) => file.endsWith(".json") && !expectedFiles.has(file)
)
if (staleFiles.length) {
  throw new Error(`Remove or reassign stale schema snapshots: ${staleFiles.join(", ")}`)
}

for (const [name, schema] of Object.entries(schemas)) {
  const jsonSchema = z.toJSONSchema(schema)
  writeFileSync(
    `snapshots/${name}.json`,
    JSON.stringify(jsonSchema, null, 2) + "\n"
  )
}
```

### CI Check

```yaml
# .github/workflows/schema-check.yml
name: Schema Snapshot Check
on: [pull_request]

jobs:
  schema-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
      - run: npm ci
      - run: npx tsx scripts/export-schemas.ts
      - name: Check for schema drift
        run: |
          if [ -z "$(git status --porcelain --untracked-files=all -- snapshots/)" ]; then
            echo "Schemas unchanged"
          else
            echo "::error::Schema snapshots have changed. Review the diff and update snapshots if intentional."
            git diff snapshots/
            exit 1
          fi
```

### Workflow

1. Developer changes a schema
2. CI runs `export-schemas.ts` and diffs against committed snapshots
3. If tracked, added or removed snapshots differ, CI fails; inspect `git status` and the diff (untracked contents need an explicit read)
4. Developer reviews the diff, runs `npx tsx scripts/export-schemas.ts` locally, commits updated snapshots

## Unused Schema Detection

Find schemas that are defined but never imported.

### With knip

```bash
npx knip --include exports
```

knip detects unused exports including schemas. Configure in `knip.json`:

```jsonc
{
  "entry": ["src/index.ts"],
  "project": ["src/**/*.ts"],
  "ignore": ["**/*.test.ts", "**/*.spec.ts"]
}
```

### With ts-prune

```bash
npx ts-prune | grep -i schema
```

Treat hits as candidates. Check public exports, dynamic loading, generated consumers and tests before removing a schema.

## Circular Dependency Detection

Circular schema imports can fail during module initialization. Inspect the evaluated dependency and initialization order; a cycle alone does not prove a crash.

### With madge

```bash
# Find circular dependencies
npx madge --circular --extensions ts src/

# Generate dependency graph
npx madge --image graph.svg --extensions ts src/
```

### CI Integration

```yaml
- name: Check circular dependencies
  run: |
    npx madge --circular --extensions ts src/
    if [ $? -ne 0 ]; then
      echo "::error::Circular dependencies detected"
      exit 1
    fi
```

## Custom lint messages

Name the resolved Zod API, explain the observed contract mismatch, link the owning rule and propose a fix. Avoid advice based solely on a method's spelling.

## Summary

| Tool | What It Catches | When |
|------|----------------|------|
| Binding-aware ESLint rule | Applicable Zod misuse, with explicit exceptions | On save / pre-commit |
| Schema snapshot CI | Unintended schema changes | On pull request |
| knip / ts-prune | Unused schemas | On pull request / periodic |
| madge | Circular schema dependencies | On pull request |
