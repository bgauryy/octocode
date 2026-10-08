# Report-Only Defects

Load when an agent-authored smell hides a wrong result instead of dead weight. Why: removing the disguise reveals a bug that needs a fix.

| Error masking signal | Why it is not junk |
|---|---|
| `catch` that logs and continues, no rethrow or user-visible signal | Deleting it surfaces an unhandled failure |
| `except: pass`, empty `catch {}`, `.catch(() => {})` | Production may rely on the swallowed path |
| Fallback for a failed call (`?? 'default'`, hardcoded sample response) | Callers may depend on the fallback shape |
| Success returned without checking the effect landed | Every downstream check trusts it |
| Default on a value the types call non-null (`?.`, `?? <literal>`, `\|\| {}`, `.get(k, default)` on a required key) | Turns a broken invariant into plausible data |
| Stub body (`pass`, `return None`, `return {}`, `throw new Error("not implemented")`) on a reachable path | Callers already receive the placeholder |

Unverified success is the highest severity: reporting success while target bytes, rows, or requests are unchanged defeats every check above it.

Test integrity: `references/test-gaming.md`. Route confirmed findings through `references/test-hygiene.md` only after the bug is fixed.

| Dependency, supply-chain, credential signal | Verify |
|---|---|
| Dependency that does not resolve on the registry | `artifactSearch` with the ecosystem `type` and exact `packageName` returns empty; provider errors do not prove absence |
| Plausible package, recent, near-zero adoption | Compare with the real package it imitates |
| Import absent from every manifest | Phantom dependency, per `references/declaration-hygiene.md` |
| Placeholder key, token, or URL for a real integration | It never ran against a real credential |
| Credential in version control | History removal is a separate task |
| Mutable reference: `uses: x@main` or `@v1`, `FROM image:latest`, `"*"` or `"latest"`, unpinned global installs | Pinning changes resolution; record the intended version with the owner |
| Insecure setting: `verify=False`, `rejectUnauthorized: false`, `NODE_TLS_REJECT_UNAUTHORIZED`, `privileged: true`, `chmod 777`, `permissions: write-all` | A caller may rely on it; the owner decides the secure value |

An unresolvable dependency name is a supply-chain risk, not a typo: the slot is attacker-controllable until a maintainer confirms the intended package.

## Escalation protocol

1. Record file, line, class, and the exact failure disguised.
2. State the correct behavior and whether a caller depends on the current one.
3. Keep it out of every batch, including already-approved batches.
4. Hand blast radius to `octocode-research` and the fix to normal development. Report it even when the user asked only for cleanup.
5. No broader catch, relaxed assertion, or lowered threshold. Keep repository acceptance requirements.
