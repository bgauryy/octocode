# Report-Only Defects

Load when an agent-authored smell hides a wrong result rather than dead weight. These exit this skill as findings, never as an excision batch: removing the disguise reveals a real bug, and that is a fix.

## Error masking

| Signal | Why it is not junk |
|--------|-------------------|
| `catch` block that logs and continues, with no rethrow and no user-visible signal | Deleting it surfaces a failure the caller never handled |
| `except: pass`, empty `catch {}`, `.catch(() => {})` | The swallowed path may already be relied on in production |
| Fallback value substituted for a failed call (`?? 'default'`, hardcoded sample response) | Callers may depend on the fallback shape |
| Success returned without verifying the effect landed | Every downstream check trusts this return |
| Safe navigation or a default on a value the types call non-null (`?.`, `?? <literal>`, `\|\| {}`, `.get(k, default)` on a required key) | The default turns a broken invariant into plausible data |
| Stub body standing in for logic (`pass`, `return None`, `return {}`, `throw new Error("not implemented")`) on a reachable path | Callers already receive the placeholder as a result |

The unverified-success row is the highest-severity item here: a routine reporting success while the target bytes, rows, or requests are unchanged defeats every check above it.

## Test integrity

Edited assertions, special-cased inputs, weak oracles, environment detection, grader patching, and CI weakening: load `references/test-gaming.md`. Route confirmed findings through `references/test-hygiene.md` only after the underlying bug is fixed.

## Dependency, supply-chain, and credential defects

| Signal | Verification required |
|--------|----------------------|
| Dependency that does not resolve on the registry | `artifactSearch` with the dependency ecosystem `type` and exact `packageName` returns empty; provider errors do not establish absence |
| Plausible-looking package published recently with near-zero adoption | Name compared against the real package it imitates |
| Import of a package absent from any manifest | Phantom dependency confirmed via `references/declaration-hygiene.md` |
| Placeholder key, token, or URL standing in for a real integration | The integration has never run against a real credential |
| Credential committed to version control | Rotate first; removal from history is a separate task |
| Mutable reference: `uses: x@main` or `@v1`, `FROM image:latest`, `"*"` or `"latest"` versions, unpinned global installs | Pinning changes resolution; record the intended version with the owner |
| Insecure setting: `verify=False`, `rejectUnauthorized: false`, `NODE_TLS_REJECT_UNAUTHORIZED`, `privileged: true`, `chmod 777`, `permissions: write-all` | A caller or deployment may rely on it; the owner decides the secure value |

An unresolvable dependency name is a supply-chain risk, not a typo — treat the slot as attacker-controllable until a maintainer confirms the intended package.

## Escalation protocol

1. Record file, line, class, and the exact failure the code disguises.
2. State the correct behavior, and whether any caller depends on the current one.
3. Keep it out of every excision batch, including batches the user already approved.
4. Hand blast-radius mapping to `octocode-research` and the fix to normal development work, reporting the finding even when the user asked only for cleanup.

Never satisfy a check by widening the mask: no broadened catch, relaxed assertion, or lowered threshold to hide a failure. Preserve repository acceptance requirements.

Next: for the behavior-preserving tier load `references/agentic-defects.md`; for measured base rates load `references/agentic-defects.md` (Audit order).
