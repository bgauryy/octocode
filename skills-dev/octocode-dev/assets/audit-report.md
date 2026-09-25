# Octocode dev audit — <tool|all> — <YYYY-MM-DD>

Scope: <tools> · Mode: audit | audit+fix · Core rev: <sha> (dirty?) · Contract fingerprint: <12 chars> · Config: <non-default settings, disabled tools>

## Summary

| Tool | Contract | Impl | Output | Workflow | Config+Docs | Fixed | Open |
|---|---|---|---|---|---|---|---|
| <tool> | ok / n findings | … | … | … | … | n | n |

## Findings

### <tool> — <lane> — <short title>
- Status: confirmed | candidate · Severity: P1 | P2 | P3 · Owner: core | native | engine | cli | mcp | config | docs
- Evidence: `<file:line>` · repro: `$OCTO <tool> '<json>'`
- Expected vs observed: <one line each>
- Fix: <applied change + files> | deferred — <reason>

## Cleanups applied

- <removed duplicate/dead item> — <file> — <why safe (evidence)>

## Verification

| Check | Command | Result |
|---|---|---|
| Contract sync | `yarn workspace @octocodeai/octocode-native contracts:check` | passed / failed / skipped |
| Rust tests | … | … |
| CLI repro | … | … |
| MCP repro (fresh server) | … | … |

## Pre-existing failures (not caused by this run)

- <test/lint> — baseline evidence
