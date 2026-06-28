---
name: octocode-research
description: "Use when leading evidence-first technical research across local code, GitHub, npm, PRs, history, artifacts, binaries, OQL research/graph packets, or prior-art landscapes. Use for multi-step investigation, idea validation, root-cause discovery, implementation planning, and decision briefs with exact citations."
---

# Octocode Research

Lead the investigation. Frame the question, widen the search enough to avoid anchoring, then converge on proof. This is the general technical research router: use `octocode-brainstorming` for raw idea/white-space exploration, `octocode-engineer` when code edits or review are likely, and `octocode-loop` when the user asks for repeated Act→Observe→Learn loops.

Octocode transport reference: read `references/octocode.md` when choosing, installing, or explaining Octocode MCP vs CLI usage.

## 1. Transport Probe

Pick the best Octocode surface before researching:

1. **MCP registered**: use `localSearchCode`, `ghSearchCode`, `npmSearch`, `lspGetSemantics`, `oqlSearch`, and peers directly after reading the tool description/input schema. If configuration is needed, read `references/octocode.md`.
2. **CLI available/preferred**: use `npx octocode`.
3. **No Octocode transport**: use `rg`/file reads/web only enough to answer, mark confidence degraded, and tell the user to install/run `npx octocode` only when Octocode is required. When GitHub auth is needed, tell them to run `npx octocode auth login`.

Useful probes:

```bash
npx octocode --version
npx octocode auth status --json
npx octocode context
npx octocode tools
```

## 2. Research Shape

Choose the smallest mode that can answer the question:

| Mode | Use when | Loop |
|------|----------|------|
| **Map** | "what exists?", prior art, package/repo landscape | frame terms -> search all relevant surfaces -> cluster -> cite |
| **Validate** | "is this worth it?", "should we add X?" | light diverge -> local/external research -> cross-pollinate -> advocate/critic -> verdict |
| **Investigate** | bug/root-cause/behavior/code question | orient -> hypothesis map -> search/read -> AST/LSP/history proof -> answer |
| **Plan** | implementation/refactor planning | understand current flow -> blast radius -> alternatives -> safest next step |

Default to **Validate** for ambiguous "research this" requests, and **Investigate** for concrete code behavior.

## 3. Operating Loop

1. State scope in one line: corpus, question, mode, and active/skipped surfaces.
2. Diverge just enough: 2-4 reframings/search terms; more only for idea generation.
3. Make a hypothesis map: `crowded/underserved/blocked/worth` for ideas, or `likely/alternate/disconfirming command` for code.
4. Orient before reading: tree/path/package/repo discovery first, exact slices later.
5. Cross-pollinate: web/product/package/repo/local clues must feed the next surface; keep a claim ledger, not raw dumps; fetch/open formal web sources before citing.
6. Prove claims: snippets/search results are leads; exact content, AST, LSP, PR/commit evidence, binary metadata, papers/specs/docs, or tests are proof.
7. Stress-test: run Advocate vs Critic for contested decisions; keep claims that survive rebuttal.
8. Decide: answer with confidence and the one next action.

Stop and ask when the problem fans into 3+ unrelated spaces, every surface is thin after synonym retries, evidence materially conflicts, or the next step changes public contracts / broad blast radius.

## 4. Tool Routing

Prefer current `search` lanes:

- Local/GitHub text: `search <term> <path|owner/repo> --view discovery`
- Files/tree: `search <path|owner/repo> --tree`; `search <query> <path> --search path`
- Exact reads: `search <file|owner/repo/path> --match-string <s> --content-view exact`
- AST: `search <path> --pattern '<shape>' --lang <lang>` or `--rule '<yaml>'`
- LSP: `search <file> --op references|callers|callees|definition --symbol <name> --line <lineHint>`
- Prior art: `search <keywords> --target repositories`; `search <package> --target packages`
- PR/history: `search <owner/repo#N> --target pullRequests`; `search <owner/repo/path> --target commits`
- Artifacts: `search <file> --target artifacts --inspect|--list|--strings`; `unzip <archive>`
- Remote-as-local: `cache fetch` or `clone`; continue on returned `localPath`
- OQL JSON: `search --scheme --compact` before `search --query '<json>'`
- Raw tools: `tools <name> --scheme` before `tools <name> --queries '<json>'`

Use `--json` whenever another step depends on paths, refs, line numbers, pagination, or continuations.

## 5. Reference Routing

Read [research-flow.md](./references/research-flow.md) when executing a concrete Map, Validate, Investigate, Plan, prior-art, PR/history, dead-code, artifact, or binary workflow. It contains the copy-pasteable flows, stress-test pattern, and output skeleton. When changing this skill, smoke it with `evals/prompts.md` and `scripts/eval-research.mjs --self-test`.

## 6. Output

Quick answer:

```text
Finding: <answer>
Evidence: <file:line / fetched formal URL / PR / artifact fact>
Confidence: confirmed|likely|uncertain
Next: <one action>
```

Decision brief:

```text
TL;DR
Framings considered
Evidence by surface
What survived rebuttal
Verdict
Risks / gaps
Recommended next step
```
