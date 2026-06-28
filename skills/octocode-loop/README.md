# Octocode Loop

`octocode-loop` keeps an agent in an evidence loop: Act -> Observe -> Learn -> Repeat. Use it when the question is already clear but one search or one read is not enough to trust the answer.

The skill is about convergence. Each iteration must produce a real Octocode observation, update the hypothesis, and choose the next cheapest useful step.

## How it works

The skill frames one question, chooses the cheapest observation that can change the answer, reads the returned status before details, and carries exact anchors into the next step. Each loop updates the hypothesis until an exact read, AST/LSP check, history fact, build, test, or other deterministic proof closes the question or a clear stop rule is reached.

## Good asks

- "Keep searching until you prove where this behavior comes from."
- "Check whether this pattern holds across the repo."
- "Run a careful evidence loop before concluding this is dead code."
- "Compare a few repos and reconcile the answer."
- "Validate this bug hypothesis with search, reads, LSP, and tests."

## What you get

- The exact question the loop is trying to close.
- Key observations, not a transcript of every tool call.
- Anchors carried forward: paths, lines, symbols, cursors, packages, commits, or PRs.
- A final answer with evidence and the deterministic check that promoted a lead to proof.
- Open gaps when the budget stops before full confidence.

## Use another skill when

- The idea itself needs exploration: use `octocode-brainstorming`.
- The task is broad code work or editing: use `octocode-engineer`.
- The user wants a formal plan or RFC: use `octocode-rfc-generator`.
- The lookup is quick and bounded: use `npx octocode`.

## User value

This skill prevents premature "found it" answers. The user can see what changed the agent's mind, which evidence survived, and why the final confidence is justified.
