Load when thinking a question through with the user, framing hypotheses, choosing directions, judging verified claims, or briefing critics or debaters.

# Critical thinking

## Thinking with the user

For [thinking together](../output.md#think-together):

- The user's view first, including what they already ruled out.
- Refine each user idea before adding yours: its strongest form, riskiest assumption, cheapest test.
- Generate your own options per lens before reading search-result lists; label them as yours.
- Diverge, then converge: options without judging, then merge, cut, and rank against the user's criteria.
- Say what is still unknown.

## Decompose, then recompose

- Split into sub-questions that together decide it, each answerable alone and naming its settling evidence; mark each known, assumed, or to research.
- Right sub-answers can compose wrongly: [recompose](phases.md#4-decide).

## Hypotheses before search

Before searching, record the leading answer, strongest rival, and “no difference” or a named condition where plausible. For each: claim, refuting check and result (survived, refuted with evidence, or untested). Add hypotheses only when they change what evidence to seek.

- A hypothesis nothing could refute is a framing: sharpen or drop it.
- Seek evidence likely under one hypothesis and unlikely under the others; evidence fitting all is context. Try to refute the leading one first.
- Unknown space: one exploratory pass, then hypotheses. Test generated ideas, yours included, for feasibility.

## Thinking lenses

Pick lenses that can change the decision; each becomes a direction or a question inside one.

| Lens | Question | Typical evidence |
|---|---|---|
| Incentives | Who gains or loses, and how would each respond? | Pricing, licensing, governance, adoption stories |
| Base rates | How often do similar attempts succeed, across reference classes? | Surveys, postmortems, benchmarks, issue history |
| Mechanism | Why would it work; what must be true? | Specs, primary studies, source code |
| Failure cases | Where did it fail, under which conditions? | Incident reports, critiques, negative results |
| Adjacent fields | Who solved the same structure elsewhere? | Other-domain work, labeled analogy |

## Search like a fact-checker

- Check on other pages who is behind a source: owner, funding, whether it sells what it praises.
- Quote the original study, spec, or release; if unreachable, note `secondary` and lower confidence.
- Corroborate with an independent source, not the same story syndicated.
- Conflicts: report them; resolve by source quality, never by count.
- Citations in AI search answers are leads only.
- Stop a line when varied searches add no deciding evidence; “no evidence found” is valid. Name unresolved gaps instead of padding the source list.

## Independence and incentives

- Delphi: independent first positions (each worker or critic commits before seeing another's), then share; keep dissent.
- Multiple workers reading one source add no source independence.
- Resolve disagreement with distinguishing evidence, not a majority vote; debate only when the [gate](#debate-when-and-how) holds.
- Critics never gathered the evidence they check; they check evidence, not agreement.
- Weigh by the author's stake: vendors, method authors, and advocates show a claim exists, not that it is true.

## Judging without bias

- Compare in both orders; a flip means unresolved, never pick.
- Score one criterion at a time, then combine; never ask "which is better overall".
- Ignore markdown, length, citation count, and tone.
- More judges or debate do not remove shared bias.

## Calibrated confidence

| Confidence | Evidence required |
|---|---|
| `high` | Direct authoritative evidence settles the claim in matching conditions; material counterevidence is resolved. Empirical generalizations need independent support. |
| `medium` | Relevant primary support with a material applicability, independence or counterevidence gap. |
| `low` | Indirect, promotional or thin evidence; unresolved alternatives; or an untested prescription inferred from other findings. |

State confidence with its because (the claim ids behind the level) and the check that would move it.

## Checks for critics and the judge

| Check | Ask |
|---|---|
| Quote, not paraphrase | Does the quote say what the claim says, with nothing added? |
| Exact source | Is the cited URL the verified page (abstract ≠ full text)? |
| Original source | Original study or release, not a page citing it? |
| Comparator | Better than what? A weak baseline is not your alternative. |
| Conditions | Which task, judge, information? Results can flip. |
| Population | Human study used for models, or the reverse? Neither transfers. |
| Independence | Shared paper, authors, or mirror? Count one study once. |
| Base rate | Success rate across reference classes, not a few successes? |
| Effect size | How large? Significant can be small. |
| Design | Does the study design support a causal claim, or only an association? |
| Status and date | Peer-reviewed, preprint, or summary? Dated? |
| Diagnosticity | Likelier under one hypothesis? If not, it decides nothing. |
| Incentive | Does the source gain? Then it needs independent support. |
| Counterevidence | What cuts against it, and which is stronger? Resolve by quality, never votes or eloquence. |

For costly or irreversible choices, ask what would make the proposal fail, what happens afterward, and what contains the harm. A reversible experiment preserves options; a premortem alone proves nothing.

## Debate: when and how

Debate only when one contested conclusion decides the recommendation, verified evidence supports both sides, and both agree on a settling test. Otherwise gather the missing evidence or report the gap. Write a falsifiable resolution and its settling evidence, then use the [debater and judge briefs](briefs.md#debater-brief).

Next: [briefs](briefs.md); report per [output](../output.md).
