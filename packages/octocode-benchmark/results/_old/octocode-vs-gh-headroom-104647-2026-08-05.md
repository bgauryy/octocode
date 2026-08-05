# Octocode vs `gh` + Headroom — campaign 104647

**Campaign start:** 2026-08-05T10:46:47Z  
**Runs:** 3 isolated passes per arm × 17 questions = 102 answers  
**Headroom:** 0.33.0, `kompress-v2-base`, ONNX backend  
**Character unit:** Unicode code points from preserved, SHA-256-verified artifacts

## Verdict

**Octocode wins this valid rerun.** After independent blind grading and label
reveal, Octocode won 31 of 51 question pairings versus 20 for `gh` + Headroom.
Correctness was effectively tied—508/510 for Octocode and 507/510 for
Headroom—while Octocode used 1,001,831 characters versus 2,484,950: **59.7%
less context**, or a **2.48× smaller** footprint.

The comparison is now measurable. All 102 strict question logs were present;
all 433 referenced artifacts matched their recorded hashes; every Headroom
transform was classified; both arms recorded source exit status; and all six
answer files covered Q1–Q17. The campaign validator reported zero failures.

## Aggregate result

| Arm | Correctness | Research | Workflow | Calls | Failed calls | Chars in | Pair wins |
|---|---:|---:|---:|---:|---:|---:|---:|
| `gh` + Headroom | 507/510 (9.941) | 247/255 (4.843) | 210/255 (4.118) | 151 | 3 | 2,484,950 | 20 |
| Octocode | **508/510 (9.961)** | 247/255 (4.843) | **234/255 (4.588)** | **131** | **2** | **1,001,831** | **31** |

Preferences were correctness-first, with measured characters breaking an
essentially equal-correctness tie. There were no tied pairings. Octocode made
13.2% fewer calls and used 1,483,119 fewer characters.

## Per-pass result

| Pass | Headroom C/R/W | Headroom chars | Octocode C/R/W | Octocode chars | Pair wins | Pass winner |
|---:|---:|---:|---:|---:|---:|---|
| 1 | 170/84/68 | 872,187 | 170/82/**80** | **288,198** | 3–14 | Octocode |
| 2 | 170/81/74 | 381,766 | 170/**84/78** | **352,647** | **10–7** | Headroom |
| 3 | 167/**82**/68 | 1,230,997 | **168**/81/**76** | **360,986** | 7–10 | Octocode |

Median pass footprint was 872,187 characters for Headroom and 352,647 for
Octocode, a 2.47× ratio. Median correctness was 10 in every pass for both arms.

## Question-level majority

| Majority winner | Questions |
|---|---|
| Octocode (11) | Q1, Q3, Q4, Q6, Q7, Q8, Q9, Q11, Q12, Q16, Q17 |
| Headroom (6) | Q2, Q5, Q10, Q13, Q14, Q15 |

Most rows were fully correct on both sides and therefore decided by measured
context. The only Q1–Q16 correctness deduction was Headroom pass 3 Q7, which
reported React's optional-peer metadata but omitted the containing
`peerDependencies.react` value. On Q17 pass 3, both arms correctly explained
memoization, keying, and bypasses but omitted the requested second composed
layer, `createPatchedFetcher` in `patch-fetch.ts`; both received 8/10.

## Headroom compression result

| Pass | Raw `gh` chars | Headroom chars in | Reduction | Calls |
|---:|---:|---:|---:|---:|
| 1 | 891,916 | 872,187 | 2.21% | 47 |
| 2 | 417,203 | 381,766 | 8.49% | 54 |
| 3 | 1,374,243 | 1,230,997 | 10.42% | 50 |
| **Total** | **2,683,362** | **2,484,950** | **7.39%** | **151** |

The 151 calls resolved to 67 `router:noop`, 36
`router:protected:analysis_context`, 23 `router:mixed`, 21
`router:smart_crusher`, and 4 `router:diff` transforms. No call used the invalid
`router:protected:user_message` route, and no model-readiness or disabled-model
diagnostic appeared. This directly resolves the old zero-ratio ambiguity: a
zero reduction is accepted only with a recorded legitimate transform.

## Q1 ambiguity and corrected grading

Q1 asked for the exported function that converts a filesystem route string to
a regular expression and “the internal helper it calls first to create named
parameter groups.” The frozen source supports two internally consistent
readings in `route-regex.ts`:

- `getRouteRegex` → `getParametrizedRoute`: creates the regexp plus a `groups`
  metadata object keyed by parameter names.
- `getNamedRouteRegex` → `getNamedParametrizedRoute`: creates the actual named
  regexp.

The wording did not distinguish name-keyed metadata from ECMAScript named
capture groups. A separate adjudicator, isolated from answers and grades,
classified the prompt as materially ambiguous. The blind grader then regraded
only Q1, awarding full correctness to either consistent pair and preserving
research/workflow deductions. No answer mixed the pairs. This changed the
mapped preference result to 31–20 for Octocode and removed an artificial
correctness penalty.

Future runs should explicitly ask for either “the `groups` metadata keyed by
route parameter name” or “ECMAScript named capture groups.”

## Q14: exact answer and why the old result was false

At frozen SHA `96e40feeae35a35e185ba2dd718253459bda2b30`, Vitest's package fields are:

| Field | Value |
|---|---|
| `dependencies.vite` | **absent** |
| `peerDependencies.vite` | `^6.4.0 || ^7.0.0 || ^8.0.0` |
| `peerDependenciesMeta.vite.optional` | `false` |
| `devDependencies.vite` | `^6.4.0 || ^7.0.0 || ^8.0.0` |

Vite is therefore a **required peer dependency**, not a regular dependency; it
is also a development dependency. The earlier false classification came from
an excerpt that omitted the JSON object boundaries, leaving a nearby `vite`
field to be assigned to the wrong containing object. The hardened benchmark now
requires a deterministic `containing object → field → value/absent` ledger
before grading structured facts. **All six answers classified Q14 correctly in
this rerun.** Headroom won two of its three Q14 character tiebreaks; Octocode won
one.

## What made this rerun valid

1. Both transports preserve complete raw/processed artifacts, Unicode counts,
   SHA-256 hashes, source exit status, and one strict JSONL record per call.
2. Headroom records the exact transform and backend. Preflight exercises valid
   no-op, SmartCrusher JSON, mixed JSON/prose, and long-prose ONNX routes.
3. Failed probes remain in call and character totals as workflow waste.
4. Campaign validation requires 102 logs, six complete answer files, valid
   hashes/artifacts, classified transforms, and clean model diagnostics.
5. Immutable repository SHAs and campaign-boundary PR/issue states were frozen
   before runners started. Every question stayed below the eight-call limit;
   the maximum was six.
6. A clean grader established frozen-ref ground truth before opening the
   identity-redacted, pass-shuffled packet. An earlier grader context that had
   seen a prior-report snippet was rejected and contributed no scores.
7. An independent measurement audit recomputed the artifact census and metrics;
   a final audit checked label mapping and arithmetic after the Q1 correction.

## Limitations

- Headroom elapsed time is not in this campaign's JSONL schema, so no latency
  comparison is claimed.
- Correctness is saturated and differs by one point across 510 available;
  interpret this as an efficiency/workflow result, not a broad quality gap.
- Compression depends on the pinned Headroom/model/runtime combination; another
  version is a different benchmark arm.

## Bottom line

This rerun supports a winner claim: with effectively equal answer correctness,
Octocode won 31–20, used 59.7% fewer context characters, made fewer calls, and
earned the higher workflow score. Headroom's instrumentation is no longer
ambiguous, but its 7.39% transport reduction did not offset the larger `gh`
research footprint.
