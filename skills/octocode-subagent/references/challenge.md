# Challenge

Load when quality risk needs a second mind without a larger swarm: hidden assumptions, echo chambers, weak verification, or a plan, artifact, or solve that needs attack, blind judgment, or independent retries.

- Verifier-critic and scout fan-out: `references/decompose.md`. Perspective debate on product or tech ideas (not code claims): `octocode-brainstorming`.

## Hard rules

1. Fresh context for every duck, interviewer, critic, red team, or voter. Never feed the first worker's full transcript as truth.
2. The parent adjudicates. Agreement is not proof.
3. One technique at a time, unless independence needs parallel critics. Do not stack by default.
4. Escalate in order: parent self-check → duck → interview or red team → blind review → verifier with anchors → consensus only if still ambiguous.
5. Need external facts or code proof: `octocode-research` or a verifier with tools. Blunt code critique: `octocode-roast`. Independent anchor tests exist: run them first.
6. Measure usefulness with `octocode-eval-benchmark` when these loop into a harness.
7. "Looks fine" without a restatement (duck) or attacks (red team) is a failed run; re-ask with a sharper scope.

## Rubber duck (stuck plan)

- Packet: goal = stress-test this plan, do not solve the task; context = short brief plus the explanation; return = `restatement` · `assumptions` · `gaps` · 3–7 `questions` · `next`.
- The duck gets minimal or no tools and no parent chat.
- The parent answers each question or marks it deferred before shipping. A load-bearing gap: fix in the parent or spawn a verifier with anchors; duck prose is not evidence.
- Variants: self-duck (parent writes restatement and assumptions once; upgrade when risk is high); duo duck (two lenses, for example security and UX).

## Interview (falsify another agent's claims)

- Packet to a fresh interviewer: the subject's `result` plus ≤8 claimed anchors, no subject transcript or chain of thought; return = `questions_asked` · `claim_table` (confirmed, contested, unknown) · `contradictions` · `verdict` · `next`.
- Optional re-interview of the subject: only the questions plus original acceptance. Require anchors or a concession, not a defense of prose.
- Ask what flips the verdict: what falsifies this; which anchor you opened or ran; what you skipped; where agents disagree; the smallest counterexample; how the answer changes if X is wrong.
- The parent re-checks contested anchors. Agreement without new anchors keeps the claim `uncertain`.
- Cap at 1–2 rounds; steer once, then stop and finish in the parent. No lateral subject-interviewer chat unless the parent relays.
- No claims yet: gather evidence first.

## Mimic flow (follow a playbook without the full chat)

- Lend a filtered playbook (checklist, skill excerpt, output contract, tool allowlist and stop rules, one tiny gold example) so a sealed worker follows a known flow. Fields: `playbook` (path or ≤1 screen), `playbook_owner`, `mimic` (`steps` | `tone` | `output_shape`), plus normal `goal`, `acceptance`, `return`.
- Never share full transcripts, unverified worker prose as facts, secrets or private data, foreign system prompts verbatim when rights are unknown, or competing mandatory rules (pick one owner).
- Mimic procedure, not conclusions; the worker still produces fresh evidence. Prefer linking loadable skill refs over pasting large prompts.
- After return, check the playbook's acceptance, not whether prose sounds like the other agent. One fact only: put it in `context`, no playbook.

## Red team

| Variant | When | Ask the worker to |
|---|---|---|
| Devil's advocate | Default adversarial pass | Argue the plan is wrong; list kill-shots |
| Premortem | High-stakes change | Write the postmortem of a failure six months later |
| Steelman | Contested decision | State the strongest opposing case before rebuttal |
| Red team | Security or abuse focus | Find exploit paths, abuse cases, privilege mistakes |

- Packet: fresh worker, sealed plan or artifact plus acceptance, no author chat; do not implement the task; max N ranked findings; return = `attacks` · `severity` · `falsifiers` · `keep_or_kill` · `next`.
- The parent answers the top kill-shots or defers them explicitly before shipping.
- Pair technical attacks with anchors (tests, build). Cap at one round unless new kill-shots appear.

## Blind review

- Packet: artifact only (diff, doc, report, packet `result`) plus an acceptance checklist; strip identity, rationale, and peer chat; return = `criteria` (pass | fail | unknown) · `blockers` · `nits` · `verdict` · `next`. No coaching the author.
- If the reviewer asks for rationale, give facts and anchors only, not persuasion. A duck hears the explanation; a blind reviewer must not.
- The parent re-checks failed criteria on real anchors.
- Citation scrub (research-like output): give the report and source snippets only; every load-bearing claim needs a concrete source location; fail uncited claims; never invent sources.

## Consensus

| Variant | When | Do |
|---|---|---|
| Self-consistency | Same hard question; stochastic answers | N independent workers, same sealed packet; parent clusters |
| Round-robin | Need an improvement trail | A proposes → B critiques → C revises; fresh each hop; pass only artifact and critique summary |
| Jury | Binary ship / no-ship | Odd N voters; parent breaks ties with anchors |

- Voters get identical `goal`, `acceptance`, `return`, and no lateral chat. Scouts have different goals; voters have the same goal.
- Treat minority dissent as a finding. Re-check anchors on the winning cluster.
- Deep split (no majority, conflicting anchors): stop; interview or red team the split. Never average prose.
- Default N=3; raise only when value pays.
- Consensus without anchors is still a claim; it never replaces missing tests. If a deterministic check exists, run it or use a verifier with anchors instead of a large N.
- Shared mutable writes: `references/shared-work.md` first. One clear specialist task: one worker plus a barrier.

Next: merge with the barrier in `references/completion.md`.
