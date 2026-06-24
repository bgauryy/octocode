# Full Multi-Source Research Loop

Load when the question spans surfaces — local workspace + GitHub + npm + commit history + web — and needs chained sub-loops that feed each other, then a synthesis. Use for "how does the ecosystem solve X", "trace this behavior from our code to upstream", "is this approach prior-art / safe to adopt". Mechanics in `loop-protocol.md`; per-surface recipe in `research-loop.md`; tools in `tools.md`.

## Shape: a loop of sub-loops

```
Frame the question
  → plan the surfaces & their order
  → for each surface: run a focused sub-loop (research-loop or code-check-loop)
      → its anchors become the next surface's seeds
  → reconcile across surfaces
  → synthesize, or open a new sub-loop if a gap remains
```

Each surface is a grounded sub-loop with its own budget. The *output anchors* of one (a symbol, package name, commit SHA, repo, error string) are the *seeds* of the next — this hand-off is what makes the chain converge instead of sprawling.

## Ordering (cheap, local-first)

Order surfaces so cheap/authoritative evidence narrows the expensive passes — like running the must-pass check first so later steps build on solid ground:

1. **Local** — what does our own code/tree actually do? Yields exact symbols, strings, versions to carry out.
2. **History** — when/why did it change? Commits and PRs explain intent and point to upstream.
3. **GitHub** — how do real repos implement it; cross-repo comparison. Clone when analysis needs AST/LSP.
4. **npm / packages** — versions, APIs, install reality for a dependency angle.
5. **Web** — only as a LEAD surface (articles, registries, discussions); confirm every web claim against code via Octocode before trusting it.

Run independent surfaces in parallel when they don't seed each other; chain them when they do.

## Reconcile, don't concatenate

After the sub-loops, reconcile: where do surfaces agree (raise confidence), where do they conflict (a real gap — open a targeted sub-loop to resolve), what's still unproven. Agreement across independent paths is the multi-source analogue of best-of-K: convergent evidence is strong, divergent evidence is a finding in itself. Do not hand back a pile of per-surface dumps — produce one synthesized answer with the decisive anchors.

## Budgets & stop

Give each sub-loop its own iteration cap; cap the number of sub-loops too. Stop when the framed question is answered with cross-surface evidence and conflicts are resolved, or when a completeness check ("which surface haven't I run? which claim is still unverified?") comes back clean. Emit a trace: surfaces run, decisive anchors per surface, reconciliation, and open gaps.
