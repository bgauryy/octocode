Load when thinking a question through with the user, decomposing it, forming hypotheses or picking lenses for options. The proof bar and confidence are in [proof and confidence](proof-bar.md); independence, bias checks, critic checks and the debate gate in [judging](judging.md).

# Critical thinking

## Thinking with the user

For [thinking together](../output.md#think-together):

- The user's view first, including what they already ruled out.
- Refine each user idea before adding yours: its strongest form, riskiest assumption, cheapest test.
- Generate your own options in [independent passes](../SKILL.md#steps) before reading search results, so neither the results nor your first ideas anchor the rest; label them as yours.
- To challenge a view, the user's or yours, name one or two specific ways it could be wrong. Demanding many reasons that are hard to find entrenches the first view ([evidence](method-evidence-challenge.md#consider-the-opposite)).
- Diverge, then converge: options without judging, then merge, cut, and rank against the user's criteria.
- Say what is still unknown.

## Decompose, then recompose

- Split into sub-questions that together decide it, each answerable alone and naming its settling evidence; mark each known, assumed, or to research. The [research tree](../SKILL.md#steps) (step 4) holds them as plan rows.
- Right sub-answers can compose wrongly: in step 9, check that each part's conditions (scale, version, workload) hold together before recommending the whole.

## Hypotheses before targeted search

After diverging and before targeted search, record for each option the claim, its strongest rival, and "no difference" or a named condition where plausible. For each: refuting check and result (survived, refuted with evidence, or untested). Run the [plan critique](phases.md#plan-and-critique) on them, which drops a hypothesis nothing could refute. Add hypotheses only when they change what evidence to seek.

- Seek evidence likely under one hypothesis and unlikely under the others; evidence fitting all is context. Try to refute the leading one first.
- Unknown space: one exploratory pass, then hypotheses. Test generated ideas, yours included, for feasibility: model ideas tend to be judged more novel and less feasible than experts' ([evidence](method-evidence-judging.md#self-evaluation)).
- Diagnosticity is a check on evidence, not a debiasing procedure: do not build a full competing-hypotheses matrix for that; searching against each claim does the debiasing ([evidence](method-evidence-challenge.md#competing-hypotheses)).

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
