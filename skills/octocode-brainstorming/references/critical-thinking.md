Load when thinking a question through with the user, generating or framing options, contradicting claims, judging verified evidence, or briefing critics or debaters.

# Critical thinking

## Thinking with the user

For [thinking together](../output.md#think-together):

- The user's view first, including what they already ruled out.
- Refine each user idea before adding yours: its strongest form, riskiest assumption, cheapest test.
- Generate your own options in [independent passes](../SKILL.md#steps) before reading search results, so neither the results nor your first ideas anchor the rest; label them as yours.
- To challenge a view, the user's or yours, name one or two specific ways it could be wrong. Demanding many reasons that are hard to find entrenches the first view ([evidence](method-evidence.md#consider-the-opposite)).
- Diverge, then converge: options without judging, then merge, cut, and rank against the user's criteria.
- Say what is still unknown.

## Decompose, then recompose

- Split into sub-questions that together decide it, each answerable alone and naming its settling evidence; mark each known, assumed, or to research. The [research tree](../SKILL.md#steps) (step 4) holds them as plan rows.
- Right sub-answers can compose wrongly: in step 9, check that each part's conditions (scale, version, workload) hold together before recommending the whole.

## Hypotheses before targeted search

After diverging and before targeted search, record for each option the claim, its strongest rival, and "no difference" or a named condition where plausible. For each: refuting check and result (survived, refuted with evidence, or untested). Add hypotheses only when they change what evidence to seek.

- A hypothesis nothing could refute is a framing: sharpen or drop it.
- Seek evidence likely under one hypothesis and unlikely under the others; evidence fitting all is context. Try to refute the leading one first.
- Unknown space: one exploratory pass, then hypotheses. Test generated ideas, yours included, for feasibility: model ideas tend to be judged more novel and less feasible than experts' ([evidence](method-evidence.md#self-evaluation)).
- Diagnosticity is a check on evidence, not a debiasing procedure: do not build a full competing-hypotheses matrix for that; searching against each claim does the debiasing ([evidence](method-evidence.md#competing-hypotheses)).

## Thinking lenses

Pick lenses that can change the decision; each becomes an option, a direction or a question inside one.

| Lens | Question | Typical evidence |
|---|---|---|
| Incentives | Who gains or loses, and how would each respond? | Pricing, licensing, governance, adoption stories |
| Base rates | How often do similar attempts succeed, across reference classes? | Surveys, postmortems, benchmarks, issue history |
| Mechanism | Why would it work; what must be true? | Specs, primary studies, source code |
| Failure cases | Where did it fail, under which conditions? | Incident reports, critiques, negative results |
| Adjacent fields | Who solved the same structure elsewhere? | Other-domain work, labeled analogy |
| Inversion | What would guarantee failure, and does any option do it? | Postmortems, migration-away stories |

## Contradict and prove

Try to break every deciding claim before relying on it, the leading option's first.

1. Search against it: failures, limits, critiques, replications and the rival's best case ([query wording](research.md#write-queries)).
2. Validate counterevidence by the same [source rules](verification.md#validated-sources) as support.
3. Mark the claim:

| Status | Bar |
|---|---|
| `proven` | A validated quote from a primary or independent source states it in matching conditions, and the disconfirming search found nothing of equal or better quality against it. A vendor, method author or advocate needs independent agreement for claims in its favor; its own account of how its product works, or an admission against its interest (a limit, a failure mode), counts as primary for that fact. |
| `contradicted` | Validated counterevidence of equal or better quality cuts against it in matching conditions. |
| `unresolved` | Thin, conflicting, inapplicable or unvalidated evidence, or no disconfirming search yet. |

- Resolve conflicts by source quality and applicability, never by count or eloquence; report the conflict.
- Report `unresolved` claims as gaps; never round one up. A recommendation resting on one gets `low` confidence or the test that settles it.
- A claim that survived a real attempt to refute it is stronger than one that was only supported; say which attempts were made.

## Independence and incentives

- Delphi: independent first positions (each worker or critic commits before seeing another's), then share; keep dissent.
- Multiple workers reading one source add no source independence.
- Resolve disagreement with distinguishing evidence, not a majority vote; debate only when the [gate](#debate-when-and-how) holds.
- Critics never gathered the evidence they check; they check evidence, not agreement.
- Weigh by the author's stake: vendors, method authors, and advocates show a claim exists, not that it is true.
- When the host offers another model, use it for a critic or diverger; the gain is a cheap test, not proven ([evidence](method-evidence.md#model-heterogeneity)).

## Your own judgment is not evidence

Your rating of an option, a worker's self-assessment and a critic's agreement are not evidence: without outside feedback, a model's self-correction of reasoning fails and its ranking of its own ideas is unreliable ([evidence](method-evidence.md#self-evaluation)). Rank and revise options by verified ledger rows. A self-review may improve wording; it never settles a claim.

## Judging without bias

- Compare in both orders; a flip means unresolved, never pick.
- Score one criterion at a time, then combine; never ask "which is better overall".
- Ignore markdown, length, citation count, and tone.
- More judges or debate do not remove shared bias.

## Calibrated confidence

| Confidence | Evidence required |
|---|---|
| `high` | Every deciding claim is `proven` in matching conditions; material counterevidence is resolved. Empirical generalizations need independent support. |
| `medium` | Deciding claims have relevant primary support with a material applicability, independence or counterevidence gap. |
| `low` | A deciding claim is `unresolved`: indirect, promotional or thin evidence; unresolved alternatives; or an untested prescription inferred from other findings. |

A recommendation you composed from proven parts is still an untested prescription, so it is `low` until the next test runs: report each part's status beside it, and name the test that would raise it. State confidence with its reason (the claims behind the level) and the check that would move it. Never invent probabilities.

## Checks for critics and the judge

Source-level checks (opened, quote, date, owner, primary, independence) are the [validated-source checks](verification.md#validated-sources). Claim-level checks:

| Check | Ask |
|---|---|
| Quote, not paraphrase | Does the quote say what the claim says, with nothing added? |
| Comparator | Better than what? A weak baseline is not your alternative. |
| Conditions | Which task, scale, version, information? Results can flip. |
| Population | Human study used for models, or the reverse? Neither transfers. |
| Base rate | Success rate across reference classes, not a few successes? |
| Effect size | How large? Significant can be small. |
| Design | Does the study design support a causal claim, or only an association? |
| Diagnosticity | Likelier under one hypothesis? If not, it decides nothing. |
| Counterevidence | What cuts against it, and which is stronger? |

For costly or irreversible choices, run a premortem: state the failure as certain ("a year on, this failed"), list its causes, and turn each into a check or a disconfirming query. Its reasons are hypotheses, not evidence ([evidence](method-evidence.md#premortem)). Then ask what happens afterward and what contains the harm. A reversible experiment preserves options.

## Debate: when and how

Debate does not reliably beat cheaper single-agent methods ([evidence](method-evidence.md#debate)). Debate only when one contested conclusion decides the recommendation, verified evidence supports both sides, and both agree on a settling test. Otherwise gather the missing evidence or report the gap. Write a falsifiable resolution and its settling evidence, then use the [debater and judge briefs](briefs.md#debater-brief).

Next: [briefs](briefs.md); report per [output](../output.md). Sources for these rules: [method evidence](method-evidence.md).
