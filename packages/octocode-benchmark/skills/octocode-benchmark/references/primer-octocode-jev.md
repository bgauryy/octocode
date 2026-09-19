# Octocode+Jev arm primer

Inject together with [`primer-octocode.md`](primer-octocode.md) as the `octojev` runner's
fixed primer. Everything in the Octocode primer applies, with these arm-specific changes.
This primer is fixed setup — it is **not** counted; use it instead of paying for schema
discovery. Never add question-specific advice here.

## Invocation (this arm runs the local build)

Every research call is:

```bash
cd <octocode-repo-root> && node packages/octocode/out/octocode.js tools <tool> --queries '<json>'
```

`--queries` takes one JSON object or an array (batch); `--input <file>` reads the JSON from
a file (prefer it for large packets); `--scheme --json --compact` prints a tool's schema
(that call is measured). Every query object requires a `reasoning` string. Record the local
build version and repo SHA in the run report.

**`ghCloneRepo` is excluded in this campaign** (known auth defect: the runtime sends
`Bearer` on git endpoints, which GitHub rejects for keyring tokens — see repo
`.octocode/GOTCHAS.md`). Do not attempt it; answer remote questions with the remote tools.

## Additional tools — Jev (typed probabilistic judgment)

Jev calls need the provider key in the same shell (each Bash call is a fresh shell):

```bash
export OCTOCODE_JEV_KEY=$(grep '^OCTOCODE_JEV_KEY=' ~/.octocode/.env | cut -d= -f2-)
```

| Tool | Use it for — and when NOT to |
|---|---|
| `jevScout` | Rank 2–12 candidates with ONE batched typed call so you read only the winners. `source.items` ranks pre-fetched rows (search hits, PR/issue rows: `{id, content, source}`); `source.local` ranks local files by anchor regex. Response: per-candidate `action` (`read`/`gray_read`/`skip`), `reads[]`, and billed `usage`. Never for a single known target; skip when a lexical check settles the shortlist. Verdicts are provisional — reopen anchors before asserting; never report absence from a `skip`. |
| `jevReasoning` | Gate a genuine judgment fork (routes: `hunch_check`, `hypothesis_triage`, `decision_review`, `disputed_inference`, `hallucination_gate`). `blocked`/`needsEvidence` is a correct result: retrieve what it names. Never a substitute for a lookup or read. |
| `ask-file` driver | Answer 2+ independent yes/no questions about content you already fetched, without reading it yourself: pipe the content in; get per-question `p_yes` + direction + billed `usage`. Path: `skills/octocode-jev-reasoning-loop/scripts/ask-file.mjs`. |

## Fire on observed state — never on self-debate

- A search/history call returns **4+ candidate rows or files you have not read** →
  `jevScout` (items mode over the returned rows) → fetch only `read`-marked candidates.
- **Fetched content + 2+ independent yes/no questions** about it → `ask-file --stdin`.
- Under 4 candidates, or one targeted search/read settles it → correctly make **no** Jev
  call (deterministic evidence outranks judgment).

## Query forms

```bash
export OCTOCODE_JEV_KEY=$(grep '^OCTOCODE_JEV_KEY=' ~/.octocode/.env | cut -d= -f2-); \
node packages/octocode/out/octocode.js tools jevScout --queries '{"reasoning":"rank rows before fetching","claim":"<what a read-worthy candidate would contain>","source":{"items":[{"id":"r1","content":"<row title/snippet>","source":"<label>"},{"id":"r2","content":"…","source":"…"}]}}'

# "which file DEFINES the capability" (vs mentions/imports it): add the implements taxonomy
… tools jevScout --queries '{"reasoning":"…","claim":"…","source":{…},"dimensions":[{"key":"implements","role":"primary","taxonomy":"implements"}]}'

export OCTOCODE_JEV_KEY=…; <emit fetched content> | node skills/octocode-jev-reasoning-loop/scripts/ask-file.mjs --stdin --source "OWNER/REPO:PATH" --questions "q1 || q2 || q3"
```

## Usage accounting (this arm self-reports Jev spend)

Every Jev response prints `usage{input_tokens,output_tokens}`. Sum them across the whole
run and per question; report `jevCalls`, `jevInputTokens`, `jevOutputTokens` in each answer
section. Zero calls on a question is a valid, reportable outcome — state which trigger
condition never occurred.
