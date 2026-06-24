# Octocode Loop

**A clear user guide for evidence-heavy research: ask a clear question, run the smallest useful Octocode call, read the result, learn from it, and repeat until the evidence converges.**

Octocode Loop is an Agent Skill for work that needs iteration, not one-shot guessing. It gives an agent a disciplined `Act -> Observe -> Learn -> Repeat` protocol over Octocode MCP tools or the Octocode CLI, so conclusions come from real tool output with anchors, statuses, and verification checks.

---

## Why use it?

Use this when you want the agent to *show its work* without drowning you in raw tool dumps. The skill turns vague research behavior into a predictable, readable research flow:

| Without this skill | With Octocode Loop |
|--------------------|--------------------|
| The agent may stop after the first plausible hit | It keeps looping until evidence converges or a clear budget is hit |
| Search results become confident claims too quickly | Search results stay leads until a deterministic check proves them |
| You get scattered snippets and tool output | You get an answer, anchors, a compact trace, verification, and open gaps |
| `empty` results are easy to misread as absence | `empty` is handled as data: broaden, change shape, or switch surface |
| Long research gets messy | The loop keeps a small ledger of goal, anchors, hypotheses, and tried calls |

The benefit is trust: you can see why the agent believes the answer, what evidence it used, and what remains unproven.

---

## What users get

When you invoke `octocode-loop`, expect the agent to behave like this:

- It states the question it is trying to close.
- It uses cheap discovery before expensive reads, clones, LSP, or tests.
- It carries exact anchors forward: files, lines, ids, cursors, branches, PRs, packages.
- It explains only the important turns in the loop.
- It ends with a grounded answer, not a transcript.

## What it does

The skill keeps an agent in a tight feedback loop:

1. **Frame** the current question and what observation would end the loop.
2. **Act** with one cheap, scoped Octocode call.
3. **Observe** the returned `status` first: result, `empty`, or `error`.
4. **Learn** by updating hypotheses and carrying exact anchors forward.
5. **Repeat** until the question is answered, the budget is hit, or the loop stops making progress.

The important part is grounding. A search hit is only a lead. A conclusion needs proof: an exact read, an AST or structural match, an LSP operation, history evidence, a build, or a test.

---

## When to use it

Use Octocode Loop when the goal is already clear and the work needs evidence to converge:

- Find where a behavior is implemented and prove the exact file and line.
- Check whether a pattern holds across a repo.
- Validate a bug hypothesis with search, reads, LSP, and tests.
- Compare how multiple repos or packages solve the same problem.
- Trace a behavior from local code to upstream history or external packages.
- Run a careful research sweep where premature "found it" would be risky.

Use a different skill when:

- The idea is still fuzzy or strategic: use `octocode-brainstorming`.
- You need broad engineering implementation or review: use `octocode-engineer`.
- You need a written design decision: use `octocode-rfc-generator`.
- You only want a quick one-off CLI lookup: use `octocode`.

---

## Why it exists

Agents often stop too early: one plausible snippet appears, the answer feels solved, and the weak claim becomes the final answer. Octocode Loop prevents that by making every iteration answer three questions:

- What did the tool actually return?
- What exact anchor can the next step reuse?
- What check would disprove the current hypothesis?

That makes the final answer auditable. You get a short trace of what was tried, what changed the answer, which evidence proved it, and what would be needed to raise confidence further.

---

## How the agent uses Octocode

The skill can use either transport:

| Environment | What the agent uses |
|-------------|---------------------|
| MCP tools are available | Octocode MCP tools such as `ghSearchCode`, `ghGetFileContent`, `ghViewRepoStructure`, `npmSearch`, local search/read, and LSP tools |
| MCP tools are not available | The `octocode` CLI, or `npx octocode` when needed |

The agent should choose one transport for a loop and keep observations comparable. Before raw tool calls, it reads the live schema instead of guessing fields.

---

## The three loop modes

### 1. General research loop

Use this for one repo, one package, or one local tree.

Flow:

```text
Frame -> orient -> search -> read -> prove -> synthesize
```

### 2. Local code checks and findings loop

Use this for bug hunts, dead-code sweeps, refactor-safety checks, pattern audits, and "does this hold everywhere?" questions.

Flow:

```text
Frame claim -> candidate search -> AST/LSP narrow -> prove with build/test/read -> record or discard
```

### 3. Full multi-source research loop

Use this when the answer spans local code, GitHub, npm, history, and possibly the web.

Flow:

```text
Frame -> run focused sub-loops per source -> carry anchors between sources -> reconcile -> synthesize
```

---

## What good output looks like

A good loop result is short but traceable:

- **Answer:** the conclusion in plain language.
- **Evidence:** exact anchors such as files, lines, repo ids, package ids, PR numbers, or commit SHAs.
- **Loop trace:** the key iterations and what each observation changed.
- **Verification:** the deterministic check that promoted the lead into a conclusion.
- **Open gaps:** anything not proven, plus what would be needed to prove it.

Example shape:

```text
Answer: The clone guard is enforced before directory fetch.

Evidence:
- packages/octocode-tools-core/src/...:42 reads ENABLE_CLONE and rejects when false.
- The exact read at that line confirms the branch returns before fetch starts.

Trace:
1. Tree search narrowed the area to provider setup and GitHub directory fetch.
2. Text search found three ENABLE_CLONE references.
3. Exact reads eliminated two config-only references and kept the runtime gate.
4. LSP references confirmed the gate is on the executed path.

Open gaps: No test was run in this pass.
```

---

## Stop rules

The loop stops when one of these is true:

- The framed question is answered with grounded evidence and the main alternate is killed.
- No cheap next step could change the conclusion.
- The iteration, token, or wall-clock budget is hit.
- The last steps made no progress, so repeating would be noise.

Before stopping, the agent should ask: "What is the weakest claim, what is the strongest counter-evidence, and would one more cheap call flip the answer?"

---

## Failure modes it guards against

| Failure mode | What the loop does instead |
|--------------|----------------------------|
| Treating a snippet as proof | Requires an exact read, AST/LSP check, history evidence, build, or test |
| Calling `empty` "not found" too early | Broadens or changes one variable before concluding absence |
| Retrying the same failed query | Changes scope, surface, or query shape |
| Losing track of evidence | Carries anchors forward verbatim |
| Drowning in context | Keeps a compact ledger of goal, anchors, hypotheses, and tried calls |
| Returning a pile of dumps | Reconciles observations into one answer |

---

## Good defaults

- Start with the cheapest query that can change the answer.
- Read `status` before reading details.
- Treat `empty` as an observation, not proof of absence.
- Carry returned paths, lines, cursors, and ids exactly.
- Keep at least two plausible explanations alive until evidence kills one.
- Prefer deterministic checks over self-judgment.
- Report the trace, not raw dumps.
