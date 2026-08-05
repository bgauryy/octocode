# Octocode vs `gh` + Headroom — campaign 093309

**Campaign start:** 2026-08-05T09:33:09Z  
**Runs:** 3 isolated passes per arm × 17 questions = 102 graded answers  
**Headroom:** 0.33.0, `kompress-v2-base`, ONNX backend  
**Character unit:** Unicode code points from preserved, hashed artifacts

## Verdict

**Octocode wins this campaign.** After blind grading and label reveal, Octocode
won 31 of 51 question pairings versus 20 for `gh` + Headroom. It also achieved
higher aggregate correctness and workflow scores while using substantially less
context: 995,235 characters versus 2,575,378.

Headroom itself reduced the raw `gh` transport from 2,796,002 to 2,575,378
characters, an aggregate **7.9% reduction**. That compression was not enough to
offset the broader/larger `gh` research outputs: the Headroom arm consumed
**2.59×** as many characters and made **1.52×** as many calls as Octocode.

Both arms remained near correctness saturation: median correctness was 10 in
every pass for both arms. The result is therefore strongest as an efficiency and
workflow comparison, not as proof of a broad capability gap.

## Aggregate result

| Arm | Correctness | Research | Workflow | Calls | Failed calls | Chars in | Pair wins |
|---|---:|---:|---:|---:|---:|---:|---:|
| `gh` + Headroom | 476/510 (9.333) | 241/255 (4.725) | 202/255 (3.961) | 169 | 3 | 2,575,378 | 20 |
| Octocode | **488/510 (9.569)** | **250/255 (4.902)** | **230/255 (4.510)** | **111** | **1** | **995,235** | **31** |

Preferences were correctness-first, then authoritative character count when
correctness tied. No pairing was a tie.

## Per-pass result

| Pass | Headroom C/R/W | Headroom chars | Octocode C/R/W | Octocode chars | Pair wins | Pass winner |
|---:|---:|---:|---:|---:|---:|---|
| 1 | 157/80/66 | 1,176,811 | **170/85/78** | **342,667** | 4–13 | Octocode |
| 2 | **163**/79/71 | 671,996 | 158/**83/75** | **352,684** | **10–7** | Headroom |
| 3 | 156/82/65 | 726,571 | **160**/82/**77** | **299,884** | 6–11 | Octocode |

Median pass footprint was 726,571 characters for Headroom and 342,667 for
Octocode (**2.12×**).

## Question-level majority

| Majority winner | Questions |
|---|---|
| Octocode (11) | Q1, Q3, Q4, Q5, Q6, Q7, Q8, Q9, Q11, Q12, Q17 |
| Headroom (6) | Q2, Q10, Q13, Q14, Q15, Q16 |

Recurring discriminators were Q1's named/non-named route-regex distinction,
Q10's Node CJS source boundary, Q12's exact frozen EventEmitter implementation,
and Q17's explicit two-layer/file mapping. Many other pairings were fully
correct on both sides and decided by context size.

## Headroom compression result

| Pass | Raw `gh` chars | Headroom chars in | Reduction | Calls |
|---:|---:|---:|---:|---:|
| 1 | 1,320,084 | 1,176,811 | 10.85% | 61 |
| 2 | 700,682 | 671,996 | 4.09% | 49 |
| 3 | 775,236 | 726,571 | 6.28% | 59 |
| **Total** | **2,796,002** | **2,575,378** | **7.89%** | **169** |

`ratio=0` was not treated as invalid. Every call recorded its exact transform,
so legitimate `router:noop` and `router:protected:analysis_context` passthroughs
were distinguishable from the invalid `router:protected:user_message`
misconfiguration. No final-scored record used the latter transform.

## What changed in the benchmark

The rerun used a hardened measurement boundary:

1. Headroom logs now preserve `transforms`, Unicode characters, bytes, SHA-256
   hashes, raw/compressed artifacts, model backend, and source exit status.
2. Octocode runs use a transparent wrapper around exactly
   `npx octocode tools …`, with the same Unicode/hash/artifact accounting.
3. Readiness preflight exercises four distinct routes: valid short no-op,
   SmartCrusher JSON, mixed JSON containing prose/code, and long prose through
   ONNX Kompress.
4. ONNX readiness is established inside **every** Headroom process before
   routing. Headroom's cache is process-local, and even compact JSON can enter
   the mixed neural path, so a size/JSON heuristic was not safe.
5. Failed `gh` and Octocode probes are captured and counted as workflow waste;
   they can no longer disappear from totals.
6. Campaign validation requires all 102 logs, six answer files, valid artifacts
   and hashes, transform classifications, source exit statuses, and clean model
   diagnostics before grading.
7. Blind packets shuffle X/Y by pass and redact tool identities. The grader
   established frozen-ref ground truth before reading answers.
8. Structured facts now require a pre-answer `field → value/absent` table from
   a complete object or deterministic parser, including the containing object.

Earlier diagnostic/partial runs are quarantined under the campaign's `invalid/`
directory and are excluded from every number above.

## Q14: what was wrong, and why

At frozen SHA `96e40feeae35a35e185ba2dd718253459bda2b30`, the exact manifest facts are:

| Field | Value |
|---|---|
| `peerDependencies.vite` | `^6.4.0 || ^7.0.0 || ^8.0.0` |
| `peerDependenciesMeta.vite.optional` | `false` |
| `dependencies.vite` | **absent** |
| `devDependencies.vite` | `^6.4.0 || ^7.0.0 || ^8.0.0` |

So Vite is a **required peer dependency**, is **not** a regular dependency, and
is also present as a development dependency for Vitest's own development.

The old false answer came from reading a compact/matched excerpt whose omitted
object boundaries placed a later `vite` line near `dependencies`; the model
assigned that line to the wrong containing object. During this rerun, the blind
ground-truth grader independently repeated the same mistake from a matched
excerpt, then corrected it after a forced complete unminified read. That is why
the new rule requires explicit object membership before either answer is seen.
All six final runner answers classified Q14 correctly.

## Validity and limitations

- All repositories were frozen to immutable SHAs at campaign start; mutable
  PR/issue state was captured at the same boundary.
- Every final question stayed within the eight-call budget.
- The final campaign validator completed with zero failures.
- Headroom elapsed time was not recorded in the JSONL schema for this campaign,
  so no timing comparison is claimed.
- Correctness is close to saturated, making preference counts sensitive to a
  few boundary questions and to the character tiebreaker.

## Frozen refs

`vercel/next.js@ab09c1f4`, `sindresorhus/is@7821031c`,
`pallets/flask@6a2f545b`, `pmndrs/zustand@beca84e6` (PR #3531 head
`a9e0896b`), `vuejs/core@2c2b92d5`, `expressjs/express@a3714473`,
`pillarjs/router@bda4af36`, `microsoft/vscode@eeb8aa44`,
`fastify/fastify@39e87e89`, `axios/axios@a339fe12`,
`evanw/esbuild@6ff1d8b0`, `nodejs/node@41afbd30`,
`redis/redis@bf49481a`, `vitest-dev/vitest@96e40fee`,
`honojs/hono@192768fb`, `eslint/eslint@56110356`, and
`eslint/js@710cac77`.

