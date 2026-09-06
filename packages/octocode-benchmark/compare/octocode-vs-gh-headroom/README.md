# Octocode CLI vs `gh` + Headroom

Thirty GitHub research questions in the shared set
[`../github-questions/`](../github-questions/) — one canonical copy, also used by
[`octocode-vs-gh`](../octocode-vs-gh/) and
[`octocode-vs-gh-rtk`](../octocode-vs-gh-rtk/).

| Arm | Allowed surface |
|---|---|
| gh + Headroom | Read-only `gh` operations, every output piped through **Headroom** compression (`../bin/ghc`) |
| Octocode | Matching GitHub research through `npx octocode tools …` |

Headroom is a **transport/compression layer, not an additional research source** —
same role as `rtk` in the gh-rtk matchup. It never adds GitHub reach; it only
shrinks what a `gh` call returns before it enters the agent's context. Baseline arm's
GitHub surface is therefore identical to the plain-`gh` arm; the only difference
is the compressor in front of it.

Both runners receive the same question and budget. Neither gets browser, peer,
or grader-reference access. Each runner may use only its assigned CLI surface;
Octocode clone-to-local tools remain part of the Octocode product surface.

Before the first call, give each runner only its assigned section from
[`RUNNER_TOOL_CONTEXT.md`](../../skills/octocode-benchmark/references/RUNNER_TOOL_CONTEXT.md). The primer is
fixed setup context; research-time help and schema calls are measured.

---

## Why this matchup exists

`SCORING.md`'s tiebreaker is **chars in/out at equal correctness**. Headroom
targets exactly that number. The question this benchmark answers: *does
compressing `gh` output preserve enough for Octocode-grade answers while cutting
the context cost* — and by how much, versus raw `gh` and versus `rtk`.

Measured on this repo's toolchain (Headroom 0.33.0, kompress-v2-base):

| gh output (real) | raw chars | chars in (compressed) | char reduction | router |
|---|--:|--:|--:|---|
| `api …/git/trees/…?recursive=1` (flask) | 66,289 | 52,137 | **21%** | SmartCrusher (JSON) |
| `issue list --json …` (cli/cli, 30) | 6,267 | 3,326 | **47%** | mixed |

(Reduction is char-based — the unit `SCORING.md` grades. Headroom's own
token-based `ratio` reads lower, for example 0.13 on the tree; report chars, not tokens.)

Structured JSON (`api`, `--json`, tree/issue/PR lists) compresses losslessly —
SmartCrusher folds repeated keys into one schema header and keeps every value.
Prose (issue/PR bodies, diffs) routes to the neural Kompress path and is lossy.

---

## Prerequisites (verified)

```bash
# gh (read-only, already authenticated)
gh --version                     # tested: 2.76.2

# Headroom CLI + library, isolated venv, pinned interpreter
uv tool install --python 3.13 "headroom-ai[all]"
headroom --version               # tested: 0.33.0

# The interpreter the shim must use (Headroom's own venv, NOT system python):
export HR_PY="$HOME/.local/share/uv/tools/headroom-ai/bin/python"
"$HR_PY" -c "import headroom; print('ok')"   # must print ok
```

> **Pin the versions.** Record `headroom --version` and the kompress model
> (`kompress-v2-base`) in the run report. A different Headroom or model version
> is a different baseline arm — do not compare across versions.

---

## How to measure Headroom + gh **properly** (read this before running)

Three facts decide whether your numbers are real. Get any one wrong and baseline arm's
"chars in" is fiction.

### 1. There is NO `headroom compress` CLI — you MUST use the library shim

Headroom exposes `proxy`, `wrap`, `mcp`, etc. — but **no subcommand that
compresses stdin**, and **no `headroom wrap gh`**. The only correct way to
compress a single `gh` command's output is the library, via the checked-in shim
[`../bin/hr_compress.py`](../bin/hr_compress.py), driven by [`../bin/ghc`](../bin/ghc).

### 2. The compression config is required — classify transforms, not ratios

The library **protects** a lone user message by default (it assumes it's the
prompt), so a naive `compress([{ "role":"user","content": gh_output }])` returns
**ratio 0.0 — nothing compressed.** The shim sets the two flags that opt gh
output in (both required):

```python
CompressConfig(compress_user_messages=True, protect_recent=0)
```

`ratio=0.000` is not itself an error: short/unsupported content can legitimately
route through `router:noop`. The checked-in instrumenter records the exact
`transforms` list. A call is invalid when transforms are missing or contain
`router:protected:user_message`; a valid `router:noop` remains a measured
passthrough call.

Before any runner starts, run the deterministic readiness gate. It verifies a
valid no-op, SmartCrusher on structured JSON, and Kompress on long prose. It
fails if the model is unavailable or ML compression is disabled:

```bash
export HR_PY="$HOME/.local/share/uv/tools/headroom-ai/bin/python"
"$HR_PY" ../bin/preflight.py --warmup
```

### 3. Count chars from the shim's log, NEVER from the agent

Models miscount their own context. `../bin/ghc` writes one **measured** JSONL
record per call (`raw_chars`, `out_chars`, `ratio`, `transforms`, hashes, and
artifact paths) to `$GHC_LOG`. Baseline arm's
"chars in" for a question is `sum(out_chars)` over that question's calls —
`../bin/sumlog.py` totals it. Do not let the runner self-report the number.

Both raw and compressed outputs are preserved under `$GHC_ARTIFACT_DIR` and
counted as Unicode code points. Strict aggregation re-reads and hashes them, so
desktop/orchestration truncation cannot silently corrupt the score.

### Reversibility policy — one-shot, no CCR retrieve loop (keeps chars-in honest)

The shim compresses **once** and does not run the proxy or expose
`headroom_retrieve`. Nothing re-expands later, so the chars it emits are exactly
the chars that enter context. **Do not** enable the proxy/MCP retrieve loop for
this arm — a retrieve call can pull originals back and silently inflate the
real context cost that "chars in" is supposed to capture.

### Determinism

Verified byte-identical across repeated runs on this setup for both the
SmartCrusher and mixed/Kompress paths (greedy decode). Still: run **≥3 passes**
per arm per question and report median correctness + char spread — a single pass
is a snapshot, and the neural path can drift across machines/model revisions.

---

## Baseline arm — how to run each `gh` call

Use `../bin/ghc` **by explicit path** for every GitHub read (there is a shell alias
`ghh=git help` on some machines — that is why the wrapper is named `ghc` and is
invoked as `../bin/ghc`, never bare on `$PATH`).

```bash
cd compare/octocode-vs-gh-headroom
export HR_PY="$HOME/.local/share/uv/tools/headroom-ai/bin/python"
export GHC_LOG="$PWD/tmp/Q1.jsonl"       # one log file per question (tmp/ is gitignored; ghc creates it)
export GHC_ARTIFACT_DIR="$PWD/tmp/Q1-artifacts"

../bin/ghc search code --repo vercel/next.js "getRouteRegex" --limit 20
../bin/ghc api 'repos/vercel/next.js/git/trees/canary?recursive=1'
../bin/ghc api 'repos/vercel/next.js/contents/PATH?ref=canary' \
      -H "Accept: application/vnd.github.raw"      # raw, not base64 (smaller)
```

Allowed gh + Headroom families (read-only, mutations are rejected by the wrapper with
exit 2): `search {code,repos,prs,issues,commits}`, `repo view`, `pr view|diff`,
`issue view`, and `api` limited to GET on `/contents` or `/git/trees`. Same
policy as the gh and gh-rtk arms.

Footprint tips (compression is on top of, not instead of, tight queries):

- **File content:** fetch raw (`-H "Accept: application/vnd.github.raw"`), not
  `--jq .content` base64 (~1.33× larger before compression).
- **Prefer snippet-bearing `gh search code`** when its hit already answers the
  question — avoid a full-file fetch entirely.
- Keep `--json` field lists minimal; SmartCrusher compresses the JSON further,
  but it can't recover fields you asked `gh` to include and never needed.

Total a question's chars-in:

```bash
python3 ../bin/sumlog.py tmp/Q1.jsonl --strict \
  --diagnostics tmp/Q1-diagnostics.log
# calls=4  raw_chars=91240  chars_in=58810  reduction=35.5%
```

## Octocode arm (instrumented transport)

```bash
export OCTO_LOG="$PWD/tmp/Q1-octocode.jsonl"
export OCTO_ARTIFACT_DIR="$PWD/tmp/Q1-octocode-artifacts"
../bin/octoc <tool> --queries '<json>'
```

`../bin/octoc` runs exactly `npx octocode tools …`; it only captures the complete
combined output, Unicode-character count, hash, exit code, elapsed time, and
artifact path. Use the log total, never a runner's reported count.

---

## Run the benchmark (per-question protocol)

There is no runner harness — benchmarks are run **by hand**. See the top-level
[`README.md`](../../README.md), [`INSTRUCTIONS.md`](../../skills/octocode-benchmark/references/INSTRUCTIONS.md),
[`JUDGING.md`](../../skills/octocode-benchmark/references/JUDGING.md), and [`SCORING.md`](../../skills/octocode-benchmark/references/SCORING.md). For each
of the 30 questions:

1. **Seal the packet.** Give baseline arm and Octocode arm the *same* question text and budget,
   differing only in allowed surface (above). No scope, hints, or reference —
   those bias the run.
2. **Isolate the arms.** Run each arm in a fresh context (separate subagent /
   session). The runner does only GitHub research; it must not see the other
   arm's transcript or any reference answer.
3. **Baseline arm:** every GitHub read goes through `../bin/ghc` with per-question log,
   diagnostics, and artifacts. Validate with `sumlog.py --strict`; a zero ratio
   is allowed only when its transform is classified.
4. **Octocode arm:** every Octocode read goes through `../bin/octoc`; use its JSONL and
   artifacts for exact character totals.
5. **Repeat ≥3×** per arm; keep median correctness, report char spread.
6. **Grade blind.** A separate grader scores both answers with arm labels
   stripped/shuffled: Correctness (0–10), Research depth (1–5), Workflow (1–5),
   Chars in/out (measured — from `../bin/sumlog.py` for A, from the recorded
   context for B). Correctness first; ties broken by fewer chars in/out. A
   confidently-wrong answer blocks a win regardless of efficiency.
7. **Report** to [`../../results/`](../../results/) with Headroom + model
   versions pinned.

After all six passes finish, reject any incomplete or corrupt campaign before
grading:

```bash
python3 ../bin/validate_campaign.py tmp/campaign-<id> > tmp/campaign-<id>/metrics.json
```

This requires all question logs (30 questions × 3 passes × 2 arms), all six answer files, valid hashes and
artifacts, classified Headroom transforms, recorded source exit statuses in both
arms, and no model-readiness diagnostics. Failed research calls remain in the
totals and are reported as workflow waste; because their output was captured,
they do not invalidate an otherwise complete pass.

The validator defaults to the current 30-question suite. To re-audit a preserved
historical campaign, pass `--question-count <N>` (for example, `20` or `17` for older sets).

### Quick smoke test (confirm the arm runs before a full campaign)

```bash
cd compare/octocode-vs-gh-headroom
export HR_PY="$HOME/.local/share/uv/tools/headroom-ai/bin/python"
export GHC_LOG="$PWD/tmp/smoke.jsonl"; rm -f "$GHC_LOG"
../bin/ghc api 'repos/pallets/flask/git/trees/main?recursive=1' >/dev/null
python3 ../bin/sumlog.py tmp/smoke.jsonl    # expect ~21% char reduction, chars_in>0
../bin/ghc issue create --repo x/y --title z ; echo "exit=$? (expect 2)"
rm -f tmp/smoke.jsonl
```

Do not infer validity from reduction alone. The run is valid only when
`preflight.py --warmup` passes and every question log passes strict validation.
