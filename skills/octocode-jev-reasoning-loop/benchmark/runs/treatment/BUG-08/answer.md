# BUG-08 Treatment — Vite HMR accept callback skipped for dynamic imports

## THINK

**Observations from source:**
- `fetchUpdate`: `hotModulesMap.get(path)` → silent return if undefined (comment: "common in code-splitting projects")
- `acceptDeps`: registers with `this.ownerPath` as key — ownerPath is set at HMRContext construction
- Static imports: ownerPath resolved at transform time by Vite dev server — matches server module graph
- Dynamic imports: ownerPath resolved at runtime by module runner — URL may differ (e.g., `/@fs/` prefix vs `/src/` form)
- `hmrHandler.ts`: WS messages serialized; if server never sends update, client never sees it

**Three competing causes:**
- H1: URL key mismatch — `ownerPath != update.path` for dynamic imports
- H2: Timing race — update arrives before `hot.accept()` registers
- H3: Server graph walk excludes dynamic import edges

## GATE

**Classification: `disputed_inference`** — all three produce identical symptom (silent skip), evidence exists for H1 and H3 but neither is directly confirmed over the other from available source reads.

Jev call warranted: Yes. `willChangeAction=true` — H1 leads but H3 (server graph exclusion) would require a completely different fix.

## JEV RESULT

```json
{
  "claim_status": "insufficient",
  "claim_status_probability": 0.53,
  "decisive_basis": "B1",
  "decisive_basis_probability": 0.81
}
```

**Claim: insufficient at 0.53.** Jev is not confident enough to assert H1 over H3. Decisive basis B1 (URL key mismatch evidence) is most relevant but insufficient to rule out H3.

**Action:** Jev's `insufficient` verdict means: do NOT assert H1 as root cause without more evidence. Recommend the discriminating check — test with a module that has both static and dynamic importers. If static importer's HMR fires while dynamic importer's doesn't, H3 is ruled out.

## Root cause

**H1 most likely but unconfirmed** (Jev: insufficient 0.53, basis B1 at 0.81). The `fetchUpdate` silent return pattern and the `ownerPath` key construction strongly suggest URL normalization mismatch as the mechanism. However, server module graph walk behavior for dynamic imports requires additional verification.

**Conservative conclusion:** The failure point is in `fetchUpdate`'s hotModulesMap lookup. The fix is defensive regardless of root cause: normalize the path before lookup and add a diagnostic log when `mod` is undefined (instead of silent return).

## Fix (safe regardless of H1 vs H3)

```ts
// fetchUpdate — add diagnostics and normalized fallback
const mod = this.hotModulesMap.get(path)
  ?? this.hotModulesMap.get(normalizePath(path))
if (!mod) {
  this.logger.debug(`[hmr] no module registered for ${path} — possible URL mismatch or dynamic import`)
  return
}
```

## Decision snapshot
- **Before Jev:** H1 likely (0.70), H3 possible (0.25), H2 unlikely (0.05)
- **After Jev:** H1 likely (0.55), H3 possible (0.40), H2 unlikely (0.05)
- **Direction changed:** No — H1 still leads, but confidence dropped; conservative fix recommended
