# Output Contracts

Load when verifying what each agent produced or when building the final report.

## Directory layout (gitignored except results/)

```
packages/octocode-benchmark/
├── output/<run-name>/                  ← gitignored; raw run artifacts
│   ├── manifest.md                     ← frozen before run starts
│   ├── SUMMARY.md                      ← headline verdict, filled after KPI
│   ├── kpi.json                        ← machine rollup
│   ├── judge-mapping-SEALED.json       ← X/Y ↔ arm; reveal only after all stages
│   ├── <suite>.md                      ← per-suite report (4 required sections)
│   ├── answers/<suite>/<arm>/
│   │   ├── answer_Q01.md
│   │   └── ... answer_Q14.md
│   ├── logs/<suite>/<arm>/
│   │   ├── Q01.jsonl
│   │   └── ... Q14.jsonl
│   └── judging/
│       ├── Q01_candidate_X.md          ← blinded (orchestrator writes)
│       ├── Q01_candidate_Y.md
│       ├── Q01_verdict_1.json          ← stage 1 judge 1 writes
│       ├── Q01_verdict_2.json          ← stage 1 judge 2 writes
│       ├── Q01_verdict_3.json          ← stage 1 judge 3 writes
│       ├── Q01_verdict.json            ← orchestrator aggregates from _1/_2/_3
│       └── Q01_flow.json               ← stage 2 judge writes (sealed logs)
└── results/<suite>.md                  ← tracked; append latest run first
```

## manifest.md — freeze before any arm starts

Required fields:
- UTC timestamp of freeze
- Exact model ID and settings (temperature, step budget, retry policy)
- Resolved SHA for every mutable branch + resolution UTC time
- Tool versions: `gh --version`, `rtk --version` or `octocode --version`, `ast-grep --version`
- Solver count k per arm
- Oracle verification date

## Q<NN>.jsonl — one line per tool call

```json
{"id": 1, "tool": "ghSearchCode", "exitCode": 0, "ms": 340, "rawBytes": 12450, "readBytes": 8200}
```

Include failed calls, retries, and pagination calls. `exitCode != 0` is a failed call — still log it, still counts against `maxToolCalls`.

## Q<NN>_verdict_<N>.json (individual stage 1 judge output, N=1/2/3)

Each of the 3 parallel judges writes to its own numbered file to avoid races.

```json
{
  "q": "Q01",
  "X": {
    "correctness": 8,
    "precision": 9,
    "recall": 7,
    "note": "one-line summary of strengths/gaps"
  },
  "Y": {
    "correctness": 5,
    "precision": 6,
    "recall": 8,
    "note": "..."
  },
  "winner": "X",
  "justification": "one paragraph — which better answers every clause and why"
}
```

## Q<NN>_verdict.json (aggregated, orchestrator writes after all 3 judges complete)

```json
{
  "q": "Q01",
  "X": {"correctness": 7.3, "precision": 8.7, "recall": 7.0},
  "Y": {"correctness": 5.0, "precision": 6.3, "recall": 8.0},
  "winner": "X",
  "judgeStd": 0.6,
  "justification": "majority winner from 3 judges"
}
```

## kpi.json — machine rollup (MUST validate against the schema)

`kpi.json` MUST validate against
[`../../../schemas/kpi.schema.json`](../../../schemas/kpi.schema.json). Do not invent a
shape — the **only** validated exemplar to copy is
[`../../../fixtures/compare-run-example/kpi.json`](../../../fixtures/compare-run-example/kpi.json).
Start from that file and fill in real values.

Top-level required keys (schema `required`):

```jsonc
{
  "run": "<run-name>",
  "date": "2026-08-03",                 // NOTE: "date" (YYYY-MM-DD), not "dateUTC"
  "subjectSha": "<octocode repo SHA>",
  "suiteVersion": 2,
  "questionBankId": "<stable-id>",
  "provenance": {                        // schemaVersion:2, harness, harnessVersion, arms
    "schemaVersion": 2, "harness": "...", "harnessVersion": "...",
    "arms": { "A": { "model": "...", "provider": "...", "modelSettings": {"temperature":0},
                     "runner": "...", "runnerVersion": "...", "configHash": "<sha256>",
                     "toolSurface": "...", "toolVersion": "..." },
              "B": { ... } }
  },
  "agentTotals": { "<suite>": { "A": {"agentTokens": 390029, "toolUses": 126, "wallClockSec": 2660},
                                "B": { ... } } },
  "questions": [                         // one entry per question, gapless q=1..N per suite
    { "suite": "octocode-vs-gh-rtk", "q": 1, "taskId": "...", "topic": "...",
      "difficulty": "hard", "category": "...", "taskStatus": "valid", "contaminated": false,
      "arms": {
        "A": {"status":"completed","correctness":8,"precision":8,"recall":7,"judgeStd":0.5,
              "rawBytes":52000,"readBytes":14200,
              "runnerTokens":14200,"tokenSource":"runner","estTokens":14200,"calls":3,
              "falseConfidence":false,"vr":0.764,"vrpt":764},
        "B": { ... } } }
  ],
  "suiteRollups": {                      // required: scoringVersion, reqFormula, verdict, arms...
    "octocode-vs-gh-rtk": {
      "questionCount": 14, "contaminatedQuestionCount": 0, "invalidQuestionCount": 0,
      "eligibilityRule": "taskStatus=valid && contaminated=false && arm.status=completed",
      "scoringVersion": "vrpt-v3",
      "reqFormula": "median over questions of: 100000 * harmonic_mean(C,P,R)/10 / tokens",
      "reqPrecision": 2,
      "arms": {
        "A": {"eligibleQuestions":14,"timeoutQuestions":0,"invalidQuestions":0,
              "meanCorrectness":7.8,"meanPrecision":8.1,"meanRecall":7.2,
              "totalRawBytes":3877979,"totalReadBytes":1476122,
              "falseConfidenceCount":3,"medianVRPT":764,"meanVRPT":820,"medianVR":0.764,
              "medianVRPTci":[680,848],"meanCorrectnessCi":[7.1,8.5],
              "toolPropertyKPIs":{"readTokensMedian":16573,"readTokensP90":107133,"readTokensCV":1.34,
                                  "rawBytesMedian":360018,"rawBytesP90":705216,"signalRatioMedian":0.27}},
        "B": { ... } },
      "verdict": "DRAFT"
    }
  }
}
```

Field names are load-bearing: `date` (not `dateUTC`), per-question `arms.<arm>` (not
`gh-rtk`/`octocode` literals), `suiteRollups` (not top-level `aggregates`/`vrpt`).
Validate before you claim a run is done (schema is JSON Schema draft 2020-12 and
uses the `date` format, so plain `ajv-cli` needs the draft + formats set up):

```bash
# from repo root (ajv + ajv-formats are installed there)
node -e '
  const fs=require("fs"), Ajv=require("ajv/dist/2020").default, addFormats=require("ajv-formats").default;
  const ajv=new Ajv({strict:false}); addFormats(ajv);
  const dir="packages/octocode-benchmark";
  const ok=ajv.compile(JSON.parse(fs.readFileSync(dir+"/schemas/kpi.schema.json")))(
    JSON.parse(fs.readFileSync(dir+"/output/<run>/kpi.json")));
  console.log("kpi valid:", ok); if(!ok) process.exit(1);
'
```

## <suite>.md — four required sections per run

| Section | Contents |
|---|---|
| 1. Tokens usage | Per-question table: raw/read bytes, calls, turns per arm; runner token totals; B/A ratios at mean and median |
| 2. Questions | Per-question: topic, contamination flag, A correctness, B correctness, toolUsed(B) |
| 3. Scores and flow | Per-question: C/P/R mean of 3 judges + judgeStd; stage-2 sealed-log flow (1–5) where run |
| 4. Guardrails & validity | False-confidence counts, dropped/timed-out questions, control scores, oracle drift found |

Losses and ties must be reported as prominently as wins. Never silently drop contaminated questions.

## results/<suite>.md — tracked ledger

Append latest run first. Format:

```markdown
## Run <run-name> — <date>

| Metric | Arm A | Arm B | B/A |
|---|---|---|---|
| Median VRPT | 6.05 | 5.66 | 0.93 |
| Median VR | 1.00 | 0.80 | — |
| Mean correctness (uncontaminated) | 1.00 | 0.90 | — |
| Median read tokens | 16.6k | 14.5k | 0.87 |
| p90 read tokens | 107k | 24k | 0.23 |

**Verdict:** DRAFT (k=1, oracle status: VERIFIED)
```
