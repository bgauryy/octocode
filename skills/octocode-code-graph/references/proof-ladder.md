# Proof Ladder

Load before converting a graph hypothesis into a finding. Why: file topology cannot prove symbol identity, runtime behavior, or safe deletion.

```text
graph candidate
→ exact-read every decisive import/export edge
→ AST structural search for import kind/code shape
→ LSP definition/references/callers/callees for symbol identity
→ configs, tests, diagnostics, build, or runtime check
→ confirmed | likely | candidate | dismissed
```

## Tool roles

- `localGetFileContent`: use `minify:"none"` with `matchString` or a line range to quote exact imports, exports, registrations, and anchors; exhaust returned continuations.
- `localSearch operation:"structural"`: distinguish code shapes; use `pattern` or YAML `rule`, and prefer `rule: kind: …` when fragment parsing is unreliable.
- `lspGetSemantics`: prove definitions, references, callers/callees, symbols, and diagnostics. Use `includeDeclaration:false` for unused claims.
- Text search: cover configs, strings, scripts, tests, reflection, generated registries, and LSP blind spots; lexical hits do not prove identity.

## Claim gates

- First apply every relevant control from `references/false-positive-controls.md`; unresolved scope/runtime alternates cap confidence at candidate.
- Cycle issue: start with the directed `runtimeCycleEdges` witness, exact-read each reported import, then show a concrete loading or maintenance impact; type-only cycles alone are not runtime-cycle proof.
- Dominator issue: explicit roots plus exact alternate-path/config search; a syntactic dominator is not proof that runtime traffic must pass through the file.
- Redundant edge: condensation `transitiveEdge:true` plus exact binding and side-effect comparison before removal.
- Affected scope: graph dependents/path plus LSP references/callers for the symbol under change.
- Layer violation: exact path, importing symbol, and an authoritative layer rule.
- Dead code/delete: explicit entrypoints and tests policy, exact export/re-export chain, LSP excluding declarations, broad text/config search, then tests/build.
- Coupling/god module: graph breadth plus mixed AST responsibilities and semantic callers/callees; file size alone is insufficient.

If LSP returns unsupported/empty/error, report the lane as unavailable and compensate with AST + exact reads + tests; confidence cannot be `confirmed` for identity-sensitive deletion. Inspect all pagination and graph warnings before a negative claim. Verification is green only when the command exit status is zero. Then load `references/output.md` to keep candidates separate from findings.
