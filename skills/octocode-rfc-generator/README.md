# Octocode RFC Generator

`octocode-rfc-generator` writes evidence-backed RFCs, design docs, architecture proposals, migration plans, implementation plans, and technical decision briefs before coding.

Use it when the cost of being wrong is higher than the cost of writing the decision down.

## How it works

The skill gathers current-state evidence first, then turns the decision into alternatives with explicit tradeoffs. It recommends one path, documents risks and rollback options, orders implementation steps by dependency, and leaves open questions visible instead of hiding weak assumptions inside the plan.

## Good asks

- "Turn this investigation into an RFC."
- "Compare these implementation approaches."
- "Write a migration plan with rollback notes."
- "Validate this proposal against the current codebase."
- "Create an implementation plan before we touch shared contracts."

## What you get

- A concise decision summary.
- Current-state evidence with file:line or external citations.
- At least two alternatives unless the user asks for a single path.
- A recommendation tied to the evidence.
- Risks, tradeoffs, migration notes, rollback options, and open questions.
- Implementation steps ordered by dependency.

## Use another skill when

- The idea still needs validation: use `octocode-brainstorming`.
- The user wants the code changed now: use `octocode-engineer`.
- The request is a quick research answer, not a document: use `octocode-research`.
- The user wants critique rather than a plan: use `octocode-roast`.

## User value

This skill converts research into a reviewable technical decision. It keeps facts cited, alternatives visible, and the rollout practical enough that another engineer can act on it.
