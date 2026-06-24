# Local Code Checks & Findings Loop

Load when looping over **workspace code** to surface and verify findings — a bug hunt, dead-code sweep, refactor-safety check, pattern audit, or "does this hold across the repo". Here the ground-truth "compiler" is the set of local checks (search / AST / LSP / build / test); each finding is proven before it's recorded. Mechanics in `loop-protocol.md`; tool names and `status` in `tools.md`.

## The check is the ground truth

A finding proposed from a snippet is a hypothesis. Promote it to a recorded finding only after a deterministic check confirms it — exactly as a closed-loop optimizer trusts the compiler's legality verdict, not the model's guess. The layers below are ordered by *trust*, cheap → authoritative; reach for the most authoritative one the claim can afford, and prefer it over reasoning about the snippet:

1. **Structural/text search** — does the pattern exist, and where? (text or AST/structural match). Cheap candidate pass.
2. **AST** — confirms code *shape* (a call with these args, a function of this form). Patterns must match a complete node; partial patterns silently return zero — use a relational rule for partial/relational matches.
3. **LSP semantics** — confirms symbol *identity* and blast radius (definitions, references, who-calls). Use for "is this the same symbol", "is this reachable", "what breaks if I change it".
4. **Build / test run** — the strongest local verdict: run the targeted test or typecheck and read the actual outcome. Never claim a fix or a safe deletion on intent — run the check and record the result.

## Findings loop

```
Frame the claim → candidate search → narrow (AST/LSP) → prove (LSP/test/build) → record or discard → next claim
```

- Each candidate is one iteration: search → read `status`/result → decide. `empty` after a deliberate broadening pass is itself evidence (e.g. "no other caller") — but confirm absence with a second shape (LSP references, or an exact path read) before trusting it.
- Record each finding with its anchor (path:line), the check that proved it, and the verdict. Discard candidates that fail their check — and note why, so the loop doesn't re-surface them.
- For "does X hold everywhere", iterate the check across the match set; a single counter-example anchor disproves a universal claim.

## Remote code, local checks

To run AST/LSP/test checks on an external repo, bring it local first (clone, or fetch a directory subtree), then point the local tools at the returned absolute `localPath`. See `tools.md` for the remote-as-local bridge.

## Verify-before-conclude

The failure mode this loop exists to prevent: asserting "fixed", "unused", "safe to delete", or "always true" from a search hit alone. The rule — run the check that would falsify the claim; only a passed falsification test earns the conclusion. Keep the alternate ("still referenced", "test would fail") alive until the check kills it.

## Stop

Stop when every framed claim is proven or discarded by a check and the budget per claim is respected. For a sweep, stop when a broadening pass yields no new candidates across two iterations. Emit the findings list (anchor + proving check + verdict) and what remains unproven.
