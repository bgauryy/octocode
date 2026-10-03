# Challenge

Load when quality risk needs a second mind without a larger swarm: hidden assumptions, echo chambers, weak verification, or a plan, artifact, or solve that needs attack, blind judgment, or independent retries. Perspective debate on product or tech ideas (not code claims): `octocode-brainstorming`.

## Rules
1. Never pass the first worker's transcript as truth to a duck, interviewer, critic, red team, or voter.
2. The parent adjudicates.
3. One technique at a time, unless independence needs parallel critics.
4. Escalate in order: parent self-check → duck → interview or red team → blind review → verifier with anchors → consensus only if still ambiguous.
5. External facts or code proof: `octocode-research` or a verifier with tools. Blunt code critique: `octocode-roast`. Independent anchor tests exist: run them first.
6. Looping into a harness: measure usefulness with `octocode-eval-benchmark`.
7. "Looks fine" without a restatement (duck) or attacks (red team) is a failed run; re-ask with sharper scope.

| Technique | Packet (fresh worker, no parent chat) | Return | Parent then |
|---|---|---|---|
| Rubber duck (stuck plan) | stress-test the plan, do not solve; short brief plus explanation; minimal or no tools | `restatement` · `assumptions` · `gaps` · 3–7 `questions` · `next` | answers or defers each question before shipping; fixes a load-bearing gap itself or via a verifier with anchors (duck prose is not evidence) |
| Interview (falsify claims) | subject `result` plus ≤8 claimed anchors; no transcript or chain of thought | `questions_asked` · `claim_table` (confirmed, contested, unknown) · `contradictions` · `verdict` · `next` | re-checks contested anchors; agreement without new anchors stays `uncertain` |
| Red team | sealed plan or artifact plus acceptance; do not implement; max N ranked findings | `attacks` · `severity` · `falsifiers` · `keep_or_kill` · `next` | answers top kill-shots or defers them explicitly before shipping |
| Blind review | artifact only (diff, doc, report, packet `result`) plus acceptance checklist; strip identity, rationale, peer chat; no coaching | `criteria` (pass \| fail \| unknown) · `blockers` · `nits` · `verdict` · `next` | re-checks failed criteria on real anchors |

- Duck variants: self-duck (parent writes restatement and assumptions once; upgrade at high risk); duo duck (two lenses, e.g. security and UX).
- Interview: optionally re-interview the subject with only the questions plus original acceptance; require anchors or a concession. Ask what flips the verdict: what falsifies this, which anchor was opened or run, what was skipped, where agents disagree, the smallest counterexample, how the answer changes if X is wrong. Cap 1–2 rounds. No lateral subject-interviewer chat unless the parent relays. No claims yet: gather evidence first.
- Red team variants: devil's advocate (default; argue the plan is wrong, list kill-shots) · premortem (high stakes; postmortem of a failure six months later) · steelman (contested decision; strongest opposing case before rebuttal) · security red team (exploit paths, abuse cases, privilege mistakes). Pair technical attacks with anchors (tests, build). One round unless new kill-shots appear.
- Blind review: if the reviewer asks for rationale, give facts and anchors, not persuasion (a duck hears the explanation; a blind reviewer must not). Citation scrub for research-like output: report and source snippets only; every load-bearing claim needs a concrete source location; fail uncited claims; never invent sources.

## Mimic flow (follow a playbook without the full chat)
- Lend a filtered playbook (checklist, skill excerpt, output contract, tool allowlist and stop rules, one tiny gold example). Fields: `playbook` (path or ≤1 screen), `playbook_owner`, `mimic` (`steps` | `tone` | `output_shape`), plus `goal`, `acceptance`, `return`.
- Never share full transcripts, secrets or private data, foreign system prompts verbatim when rights are unknown, or competing mandatory rules (pick one owner).
- Mimic procedure, not conclusions; the worker still produces fresh evidence. Link loadable skill refs rather than pasting large prompts.
- Check the playbook's acceptance, not whether prose sounds like the other agent. One fact only: put it in `context`, no playbook.

## Consensus
| Variant | When | Do |
|---|---|---|
| Self-consistency | Same hard question; stochastic answers | N independent workers, same sealed packet; parent clusters |
| Round-robin | Need an improvement trail | A proposes → B critiques → C revises; fresh each hop; pass only artifact and critique summary |
| Jury | Binary ship / no-ship | Odd N voters; parent breaks ties with anchors |

- Voters get identical `goal`, `acceptance`, `return`, no lateral chat (scouts have different goals).
- Minority dissent is a finding; re-check anchors on the winning cluster.
- Deep split (no majority, conflicting anchors): stop; interview or red team the split. Never average prose.
- Default N=3; raise only when value pays. Prefer a verifier with anchors to a large N.

Next: merge with the barrier in `references/completion.md`.
