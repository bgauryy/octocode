# Octocode-JEV

**Status:** Original broad design draft; current implementation direction is the [skill-first OJQL / Jev v2 RFC](../.octocode/rfc/jev-v2-protocol/RFC.md).
**Working name:** Octocode-JEV / OJQL
**Repository:** Octocode
**Purpose:** Structured probabilistic decision-making for agents over code and research evidence.

**2026-09-20 synthesis:** After the skill prototype, the user explicitly selected `octocode tools jev`: a pure `{state, questions}` judgment contract with optional source loading with internal model selection and workflows expressed through prompts. The short skill now runs and verifies this CLI entry. The user subsequently selected one pure tool: legacy Jev tools and standalone skill clients are retired. Whole-task efficiency remains unproven. See the [consolidation evaluation](../.octocode/octocode-eval-benchmark/jev-single-tool-2026-09-20/REPORT.md). The sections below preserve the original design space; they are not a commitment to ship a new package, cache, inheritance, workflow engine or full DSL. See the [POC evidence](../.octocode/rfc/ojql-feasibility/poc/RESULTS.md), [review disposition](../.octocode/rfc/ojql-feasibility/redteam-2026-09-20/REVIEW.md) and [current acceptance gates](../.octocode/rfc/jev-v2-protocol/RFC.md#acceptance-and-implementation-sequence).

---

# 1. Summary

Octocode-JEV adds a new reasoning primitive to Octocode:

> Let an agent describe a set of fuzzy conditions, evaluate every atomic condition through JEV, and deterministically compose the results into a decision.

Instead of requiring an agent to reason entirely internally:

```text
read files
→ think
→ guess whether condition A is true
→ think
→ guess whether B is true
→ decide
```

the agent can explicitly define:

```text
A = "Does this code contain a race condition?"
B = "Is the problematic path reachable?"
C = "Does the proposed fix address the root cause?"

decision = A && B && C
```

Each atomic condition becomes a JEV judgment.

Octocode handles:

* context resolution;
* file/location retrieval;
* JEV batching;
* probability thresholds;
* deterministic condition evaluation;
* context reuse;
* result aggregation.

Conceptually:

```text
Agent
  ↓
OJQL query
  ↓
Octocode resolves evidence
  ↓
JEV evaluates atomic judgments
  ↓
Octocode evaluates deterministic conditions
  ↓
Decision + probabilities + context
  ↓
Agent continues reasoning
```

The central idea is:

> **A probabilistic `if` statement for agents.**

---

# 2. Motivation

Agents constantly make fuzzy conditional decisions.

Examples:

```text
Does this file likely contain the bug?

Is this function actually responsible?

Is the suspected path reachable?

Does this PR introduce a breaking change?

Is the evidence strong enough to continue investigating this hypothesis?

Does this fix address the root cause?

Should I inspect another file?
```

Today these decisions normally happen inside the model's internal reasoning.

That creates several problems.

## 2.1 Hidden judgments

The agent may conclude:

```text
"This probably looks like a race."
```

but the harness does not receive:

```text
probability = 0.82
```

The judgment is embedded inside prose reasoning.

---

## 2.2 Compound reasoning

Agents often ask themselves questions containing several unrelated conditions:

```text
Is this code buggy, reachable, insecure, and related to the report?
```

This makes the judgment hard to inspect and hard to reuse.

JEV works better when judgments are atomic.

Octocode-JEV encourages decomposition:

```text
has_bug
reachable
security_relevant
matches_report
```

and then deterministically combines them.

---

## 2.3 Hallucinated certainty

An agent may jump from:

```text
"This looks suspicious."
```

to:

```text
"This is the root cause."
```

without an explicit confidence boundary.

Octocode-JEV introduces structured probabilities and configurable thresholds.

---

## 2.4 Repeated context consumption

Agents often repeatedly read or resend the same files and observations.

Octocode-JEV can retain a context snapshot and allow later queries to inherit it.

---

## 2.5 Expensive unnecessary retrieval

Sometimes the agent already knows enough to make a useful preliminary judgment.

It should not always fetch another file.

Octocode-JEV therefore distinguishes between:

```text
what the agent already knows
```

and:

```text
what evidence is available if more certainty is needed
```

---

# 3. Core Principle

The system separates five responsibilities:

```text
Octocode
    retrieves evidence

JEV
    evaluates fuzzy atomic judgments

OJQL
    describes judgments and their relationships

CEL / expression engine
    evaluates deterministic logic

Agent
    owns the higher-level research strategy
```

Or:

```text
Octocode = evidence retrieval

JEV = probabilistic judgment

OJQL = judgment composition

Agent = reasoning loop
```

---

# 4. The Fundamental Abstraction

Each check contains:

```text
question
+
context
+
JEV answer
+
pass predicate
```

Example:

```yaml
checks:
  has_race:
    ask: Can this implementation race during concurrent writes?
    pass: noul >= 0.75
```

JEV may return:

```json
{
  "noul": 0.87
}
```

Octocode evaluates:

```text
0.87 >= 0.75
```

and produces:

```json
{
  "passed": true
}
```

The final decision deals with deterministic booleans:

```text
has_race && reachable && missing_guard
```

This separation is important.

Probabilistic AI judgment:

```text
noul = 0.87
```

is not the same thing as:

```text
true
```

The application decides what probability is sufficient.

---

# 5. Important Rule: Question != HTTP Request

One atomic condition should map to one JEV question.

It should **not** necessarily map to one JEV API request.

Example:

```yaml
checks:
  race:
    ask: Can concurrent writes race?

  stale_read:
    ask: Can this code return stale data?

  missing_lock:
    ask: Is synchronization insufficient?
```

If all questions use the same state, Octocode should send:

```text
one JEV request
+
three independent questions
```

rather than:

```text
three JEV HTTP requests
```

Therefore:

```text
1 condition
=
1 logical JEV question
```

but:

```text
1 condition
≠
1 network request
```

The execution planner decides how questions should be batched.

---

# 6. Package Architecture

Octocode-JEV should be its own package inside the Octocode workspace.

Initially it should remain private while the API and DSL stabilize.

Example:

```text
packages/
  octocode-jev/
    src/
      types.ts
      schema.ts
      parser.ts
      validator.ts
      planner.ts
      context.ts
      evaluator.ts
      jev-client.ts
      execute.ts

    tests/

    ARCHITECTURE.md
    package.json
```

Suggested package name:

```text
@octocodeai/octocode-jev
```

Initially:

```json
{
  "name": "@octocodeai/octocode-jev",
  "private": true
}
```

After dogfooding, it may become public.

---

# 7. Responsibility Boundary

`octocode-jev` should **not** know how GitHub, local files, LSP, or Octocode search work.

The package should accept resolved context.

Octocode itself owns:

```text
GitHub retrieval
local filesystem
ripgrep
LSP
repository navigation
authentication
permissions
provider APIs
```

Architecture:

```text
CLI / MCP
    │
    ▼
octocode-tools-core
    │
    ├── GitHub/local/search/LSP context resolution
    │
    ▼
@octocodeai/octocode-jev
    │
    ├── validate
    ├── plan
    ├── batch
    ├── call JEV
    ├── evaluate
    └── aggregate
            │
            ▼
           JEV
```

---

# 8. Core vs Octocode Extension

The core language should understand:

```text
named context values
checks
JEV question types
predicates
decision expressions
```

It should not fundamentally understand:

```text
github://
local://
symbol://
PR://
```

Those are Octocode context-provider extensions.

Core:

```yaml
context:
  code: "resolved source content"
```

Octocode extension:

```yaml
context:
  locations:
    code:
      uri: github://owner/repo@commit/src/cache.rs
```

Octocode resolves the URI before passing the state to the JEV runtime.

---

# 9. Context Model

Context has three categories:

```text
known
locations
inherited context
```

## `known`

What the agent already knows.

Example:

```yaml
context:
  known:
    bug_report: >
      Concurrent requests occasionally return the previous cached value.

    hypothesis: >
      Invalidation becomes visible before replacement finishes.
```

This may come from:

* previous agent reasoning;
* user input;
* previous search results;
* previous Octocode queries;
* earlier research.

Important:

> `known` is agent-provided knowledge, not automatically verified evidence.

---

# 10. Locations

Locations describe evidence that Octocode *can* resolve.

Example:

```yaml
context:
  locations:
    implementation:
      uri: local://src/cache.rs

    tests:
      uri: local://tests/cache_test.rs
```

A location means:

```text
evidence is available here
```

not:

```text
fetch it immediately
```

This allows lazy evidence retrieval.

---

# 11. Known Context vs Evidence

The distinction must be preserved internally.

Example JEV state:

```json
{
  "known_context": {
    "hypothesis": "The bug is probably caused by invalidation ordering."
  },

  "evidence": {
    "implementation": "..."
  }
}
```

Do not flatten both into one anonymous blob.

This avoids turning:

```text
agent hypothesis
```

into:

```text
repository fact
```

---

# 12. Context Inheritance

A successful query may return a context reference:

```json
{
  "context_ref": "ctx_abc123"
}
```

A later query can use:

```yaml
context:
  inherit: ctx_abc123
```

and add new information:

```yaml
context:
  inherit: ctx_abc123

  known:
    new_observation: >
      The bug happens only after immediate retry.
```

This creates a new immutable context snapshot.

Conceptually:

```text
ctx_A
  │
  └── + additional context
          │
          ▼
        ctx_B
```

Do not mutate old contexts.

---

# 13. Why Context References Matter

Without context references:

```text
agent sends report
agent sends file
agent sends hypothesis
agent gets result

agent asks next question
agent sends report again
agent sends file again
agent sends hypothesis again
```

With context references:

```text
query 1
  ↓
ctx_123

query 2:
  inherit ctx_123
  ask another question
```

This reduces harness/agent token consumption and simplifies research loops.

---

# 14. Evidence Policies

Each check can define how evidence should be handled.

Three modes:

```text
none
auto
required
```

---

# 15. `evidence: none`

Use only known context.

Example:

```yaml
checks:
  hypothesis_plausible:
    ask: Is the current hypothesis consistent with the observations?

    uses:
      - known.bug_report
      - known.hypothesis

    evidence: none
```

No file resolution occurs.

Useful for cheap preliminary reasoning.

---

# 16. `evidence: required`

Always resolve the referenced evidence.

Example:

```yaml
checks:
  implementation_has_race:
    ask: Can the implementation race during concurrent writes?

    uses:
      - locations.implementation

    evidence: required
```

Flow:

```text
resolve file
   ↓
build JEV state
   ↓
ask JEV
```

---

# 17. `evidence: auto`

This is the most interesting mode.

Example:

```yaml
checks:
  race:
    ask: Is a race condition likely to explain the observed behavior?

    uses:
      - known.report
      - known.hypothesis
      - locations.implementation

    evidence: auto
```

The system may first evaluate whether the existing known context is sufficient.

Conceptually:

```text
known context
    │
    ▼
cheap JEV evaluation
    │
    ├── enough context
    │       ↓
    │   use result
    │
    └── insufficient context
            ↓
        resolve evidence
            ↓
          JEV again
            ↓
        refined result
```

This implements:

> Judge from what I already know; inspect code only when needed.

---

# 18. Auto Context Gate

Internally an auto check can generate a companion judgment.

User writes:

```yaml
checks:
  race:
    ask: Is a race condition likely to explain this bug?
    evidence: auto
```

Octocode can internally evaluate:

```text
race

race__context_sufficient
```

Where the second question asks whether the supplied known context is sufficient to evaluate the original question without resolving more evidence.

If context is sufficient:

```text
use preliminary answer
```

Otherwise:

```text
resolve referenced locations
repeat original judgment using evidence
```

---

# 19. Auto Optimization

Do not use the context gate when no meaningful known context exists.

Bad:

```text
empty known context
→ JEV context gate
→ obviously insufficient
→ fetch file
→ JEV
```

Better:

```text
no known context
→ fetch required file directly
→ JEV
```

The planner should optimize this automatically.

---

# 20. Check Context Selection

Checks should explicitly state what they use.

Example:

```yaml
checks:
  implementation_bug:
    ask: Does the implementation contain a race?

    uses:
      - known.report
      - locations.implementation
```

Another check:

```yaml
checks:
  test_supports_bug:
    ask: Do the tests support the concurrency hypothesis?

    uses:
      - locations.implementation
      - locations.tests
```

This allows Octocode to:

* minimize context;
* reduce noise;
* avoid unnecessary reads;
* deduplicate evidence resolution;
* optimize JEV batching.

---

# 21. JEV Question Types

OJQL maps directly to JEV primitives:

```text
Noul
Choice
Score
```

---

# 22. Noul

Default type.

Example:

```yaml
checks:
  has_bug:
    ask: Does the implementation contain the suspected bug?
    pass: noul >= 0.75
```

Equivalent explicit syntax:

```yaml
checks:
  has_bug:
    type: noul
    ask: Does the implementation contain the suspected bug?
    pass: noul >= 0.75
```

---

# 23. Choice

Example:

```yaml
checks:
  root_cause:
    type: choice

    ask: Which explanation best matches the observed behavior?

    options:
      race:
        Concurrent access produces inconsistent state.

      invalidation:
        Cache invalidation is incorrect.

      test_issue:
        The failure primarily comes from test behavior.

      other:
        Another explanation is better supported.

    pass: >
      choice == "race" &&
      confidence >= 0.65
```

---

# 24. Score

Example:

```yaml
checks:
  severity:
    type: score

    ask: How severe is this issue if reachable?

    levels:
      - Minor
      - Moderate
      - Significant
      - Critical

    pass: >
      score >= 2 &&
      confidence >= 0.7
```

---

# 25. Pass Predicates

Every probabilistic result must remain separate from its deterministic interpretation.

Example:

```text
noul = 0.74
```

One workflow may define:

```text
pass >= 0.60
```

Another may require:

```text
pass >= 0.90
```

Therefore checks support:

```yaml
pass: noul >= 0.75
```

---

# 26. Defaults

Common thresholds may be defined globally.

Example:

```yaml
defaults:
  noul_pass: noul >= 0.70
  context_sufficient: noul >= 0.75
```

Then a simple check can be:

```yaml
checks:
  race:
    ask: Can this implementation race?
```

---

# 27. Decision Expressions

Checks are composed through a deterministic expression.

Examples:

```text
condition_1 && condition_2
```

```text
condition_1 || condition_2
```

```text
condition_1 || condition_2 && (condition_3 || condition_4)
```

More practical example:

```yaml
decision: >
  has_race &&
  reachable &&
  (
    tests_support_bug ||
    root_cause_is_race
  )
```

---

# 28. Expression Engine

Do not create a custom expression parser unless necessary.

Recommended:

```text
CEL
```

Use CEL for:

```text
&&
||
!
()
>=
<=
==
params.foo
confidence
probabilities["race"]
```

OJQL owns the structure.

CEL owns expression semantics.

---

# 29. Logical Short-Circuit vs Network Execution

This is an important distinction.

Expression:

```text
A || B
```

logically short-circuits.

But Octocode should not necessarily execute:

```text
ask A

if false:
    ask B
```

If A and B share the same state, it is often better to send both in one JEV batch.

Therefore:

```text
logical short-circuit
≠
network short-circuit
```

The planner optimizes JEV execution independently from expression semantics.

---

# 30. Batching

Suppose:

```text
A uses [implementation]
B uses [implementation]
C uses [implementation, tests]
D uses [implementation, tests]
```

Planner produces:

```text
Batch 1
state:
  implementation

questions:
  A
  B
```

and:

```text
Batch 2
state:
  implementation
  tests

questions:
  C
  D
```

This keeps questions atomic while making execution efficient.

---

# 31. Staged / Dependent Judgments

Some judgments genuinely depend on the answer from an earlier judgment.

Example:

```text
Which file is most suspicious?
      ↓
fetch chosen file
      ↓
Does this file contain the bug?
```

That requires sequential execution.

Do not make this part of OJQL v0.1.

v0.1 should focus on:

```text
parallel independent judgments
+
deterministic composition
```

Future versions may support:

```yaml
stages:
  - choose_candidates

  - when: candidate_is_interesting
    resolve: ...

  - inspect_candidate
```

---

# 32. Return Context

Caller should control how much context is returned.

Possible modes:

```yaml
return:
  context: none
```

```yaml
return:
  context: manifest
```

```yaml
return:
  context: used
```

---

# 33. `context: none`

Return only decision and checks.

Best default for minimal output.

---

# 34. `context: manifest`

Return provenance and references without returning full file contents.

Example:

```json
{
  "context": {
    "known_used": [
      "report",
      "hypothesis"
    ],

    "locations_used": [
      {
        "id": "implementation",
        "uri": "local://src/cache.rs",
        "resolved": true
      }
    ]
  }
}
```

This should probably be the default agent-facing context mode.

---

# 35. `context: used`

Return actual known context and resolved evidence.

Example:

```json
{
  "context": {
    "known": {
      "hypothesis": "..."
    },

    "evidence": {
      "implementation": "..."
    }
  }
}
```

Useful when the agent needs the evidence immediately afterward.

Potentially expensive in agent context, so opt-in.

---

# 36. Result Model

Example:

```json
{
  "decision": true,

  "checks": {
    "race": {
      "passed": true,

      "answer": {
        "type": "noul",
        "noul": 0.91
      },

      "evaluation": {
        "source": "resolved_evidence",
        "refined": true
      }
    },

    "reachable": {
      "passed": true,

      "answer": {
        "type": "noul",
        "noul": 0.84
      },

      "evaluation": {
        "source": "known_context",
        "refined": false
      }
    }
  },

  "context_ref": "ctx_123",

  "context": {
    "known_used": [
      "report",
      "hypothesis"
    ],

    "locations_used": [
      {
        "id": "implementation",
        "uri": "local://src/cache.rs",
        "resolved": true
      }
    ]
  },

  "execution": {
    "jev_requests": 2,
    "questions": 4,
    "locations_resolved": 1
  }
}
```

---

# 37. `refined`

`refined: true` indicates:

```text
initial cheap judgment
       ↓
context was insufficient
       ↓
evidence was fetched
       ↓
question was evaluated again
       ↓
second answer became final answer
```

This is useful telemetry for understanding the research process.

---

# 38. Context Staleness

Cached context cannot assume mutable local files remain unchanged.

For local files store:

```text
path
content hash
resolved timestamp
```

Before reuse:

```text
hash unchanged
    → reuse evidence

hash changed
    → invalidate
    → resolve again
```

GitHub evidence should ideally be pinned to commits:

```text
github://owner/repo@commit/path
```

so evidence is immutable and reproducible.

---

# 39. Provenance

Internally retain provenance:

```json
{
  "known_context": {
    "hypothesis": {
      "value": "...",
      "origin": "agent"
    }
  },

  "evidence": {
    "implementation": {
      "uri": "local://src/cache.rs",
      "hash": "sha256:...",
      "content": "..."
    }
  }
}
```

Not all metadata needs to be passed to JEV.

But it should exist in Octocode.

---

# 40. Proposed Query Schema

Simplified shape:

```yaml
version: "0.1"

context:
  inherit: optional-context-id

  known:
    key: value

  locations:
    name:
      uri: ...
      description: ...

params:
  threshold: 0.75

defaults:
  noul_pass: noul >= 0.70
  context_sufficient: noul >= 0.75

checks:
  check_name:
    type: noul | choice | score

    ask: ...

    uses:
      - known.foo
      - locations.bar

    evidence:
      mode: none | auto | required

    pass: ...

decision: ...

return:
  context: none | manifest | used
```

---

# 41. Example: Bug Investigation

```yaml
version: "0.1"

context:
  known:
    report: >
      Concurrent requests occasionally return an old cached value.

    hypothesis: >
      Invalidation becomes visible before replacement finishes.

  locations:
    implementation:
      uri: github://owner/repository@abc123/src/cache.rs

    tests:
      uri: github://owner/repository@abc123/tests/cache.rs

defaults:
  noul_pass: noul >= 0.70
  context_sufficient: noul >= 0.75

checks:
  hypothesis_plausible:
    ask: >
      Is the proposed race hypothesis consistent with the
      currently known behavior?

    uses:
      - known.report
      - known.hypothesis

    evidence: none

  implementation_supports_hypothesis:
    ask: >
      Does the implementation support the hypothesis that invalidation
      becomes visible before replacement completes?

    uses:
      - known.hypothesis
      - locations.implementation

    evidence: auto

    pass: noul >= 0.75

  tests_cover_race:
    ask: >
      Do the tests meaningfully cover concurrent invalidation
      and replacement?

    uses:
      - locations.tests
      - locations.implementation

    evidence: required

decision: >
  hypothesis_plausible &&
  implementation_supports_hypothesis &&
  !tests_cover_race

return:
  context: manifest
```

---

# 42. Runtime Flow

```text
                       AGENT
                         │
                         ▼
                  OJQL QUERY
                         │
                         ▼
               ┌─────────────────┐
               │ Parse + Validate│
               └────────┬────────┘
                        │
                        ▼
               ┌─────────────────┐
               │ Context Expander│
               │                 │
               │ inherit context │
               │ merge known     │
               │ index locations │
               └────────┬────────┘
                        │
                        ▼
                ┌───────────────┐
                │ Query Planner │
                └───────┬───────┘
                        │
             ┌──────────┼───────────┐
             │          │           │
             ▼          ▼           ▼
          none         auto      required
             │          │           │
             │          ▼           ▼
             │      cheap JEV      resolve
             │      evaluation     evidence
             │          │           │
             │      sufficient?     │
             │       /      \       │
             │     yes       no     │
             │      │         │     │
             │      │      resolve  │
             │      │      evidence │
             │      │         │     │
             │      │       JEV #2  │
             │      │         │     │
             └──────┴─────────┴─────┘
                        │
                        ▼
                final JEV answers
                        │
                        ▼
                 pass predicates
                        │
                        ▼
                  CEL decision
                        │
                        ▼
               context snapshot
                        │
                        ▼
                structured result
                        │
                        ▼
                      AGENT
```

---

# 43. Execution Planner

The planner is one of the most important components.

Responsibilities:

```text
determine required context

determine which locations need resolution

deduplicate file resolution

group questions by equivalent JEV state

perform cheap known-context checks

decide which auto checks need refinement

respect JEV token limits

split oversized batches

execute JEV batches

combine results
```

---

# 44. Internal AST

Do not let YAML become the runtime model.

Compile into an internal AST.

Example:

```text
Program
├── Context
│   ├── inherit
│   ├── known
│   └── locations
│
├── Params
│
├── Defaults
│
├── Checks
│   ├── Check
│   │   ├── id
│   │   ├── questionType
│   │   ├── question
│   │   ├── uses
│   │   ├── evidencePolicy
│   │   ├── options / levels
│   │   └── passExpression
│   └── ...
│
├── DecisionExpression
│
└── ReturnPolicy
```

Pipeline:

```text
YAML / JSON
    ↓
schema validation
    ↓
OJQL AST
    ↓
semantic validation
    ↓
execution plan
```

---

# 45. JSON vs YAML

For humans and documentation:

```text
YAML
```

is ideal.

For MCP tool invocation:

```text
structured JSON
```

is preferable.

The language itself should not depend on YAML.

YAML is just a serialization format.

---

# 46. MCP Tool

Possible tool:

```text
jevQuery
```

Inside Octocode, this is cleaner than:

```text
octocodeJevQuery
```

because the tool already lives in the Octocode namespace.

Conceptual input:

```json
{
  "context": {},
  "checks": {},
  "decision": "",
  "return": {}
}
```

The tool handler should be thin.

Architecture:

```text
MCP jevQuery
    ↓
tools-core
    ↓
resolve Octocode locations
    ↓
octocode-jev
    ↓
JEV
```

---

# 47. Skill

The implementation should live in the package.

The skill should teach agents:

```text
when to use JEV

how to create atomic questions

when to use known context

when to use locations

when evidence should be required

how to avoid compound questions

how to select thresholds

how to compose decisions
```

Therefore:

```text
package = runtime

skill = agent instructions
```

Do not put the runtime implementation into the skill.

---

# 48. Validation

Before any JEV call, validate:

```text
check IDs are unique

context references exist

all `uses` references exist

decision variables exist

pass expressions are valid

decision returns boolean

Noul uses valid fields

Choice defines valid options

Score defines valid levels

context inheritance exists

URI format is recognized by Octocode

required evidence can be resolved
```

Fail early before spending JEV calls.

---

# 49. Semantic Linting

Later, add optional linting for bad JEV questions.

Bad:

```text
Is this code buggy, insecure, poorly designed, and responsible for the failure?
```

Better:

```text
has_bug

security_issue

design_issue

matches_failure
```

Possible warning:

```text
Question may contain multiple independent judgments.
Consider decomposing it into atomic checks.
```

Initially this should be a warning, not an error.

---

# 50. Error Handling

Never convert infrastructure failure into a negative judgment.

Bad:

```text
file retrieval failed
→ condition = false
```

Correct:

```json
{
  "decision": null,
  "status": "error",
  "errors": [
    {
      "type": "context_resolution_error",
      "location": "implementation"
    }
  ]
}
```

Likewise:

```text
JEV request failure
```

must not become:

```text
false
```

---

# 51. Context Budget

The planner must be context-budget aware.

Flow:

```text
group compatible questions
        ↓
estimate JEV request size
        ↓
within budget?
    /       \
  yes        no
   │          │
batch       split
```

Questions remain logically independent even when physical requests must be split.

---

# 52. Caching

Atomic judgments may eventually be cached using approximately:

```text
JEV model
+
resolved state hash
+
question definition
+
criteria/options
```

Example:

```text
hash(
    model,
    context_content,
    question,
    question_configuration
)
```

Immutable GitHub commits are especially cache-friendly.

Local file caching should depend on content hashes, not paths alone.

---

# 53. Security

OJQL v0.1 should be deliberately non-powerful.

It should not support:

```text
shell execution

arbitrary HTTP

filesystem mutation

JavaScript evaluation

Python evaluation

loops

recursion

arbitrary tool execution
```

It should describe judgments, not actions.

This makes the language safer and easier to reason about.

---

# 54. Non-Goals

v0.1 is not:

```text
a general programming language

an agent framework

a workflow engine

a replacement for Octocode search

a replacement for deterministic logic

a prompt scripting language

a chain-of-thought interface

a replacement for the Octocode research agent
```

It is:

> A declarative system for evaluating probabilistic conditions over evidence and composing them deterministically.

---

# 55. Minimal v0.1

Keep the first version small.

Required concepts:

```yaml
version:

context:
  known:
  locations:
  inherit:

params:

defaults:

checks:
  <id>:
    type:
    ask:
    uses:
    evidence:
    options:
    levels:
    pass:

decision:

return:
  context:
```

Avoid adding workflow features initially.

---

# 56. Minimal Agent Experience

A simple query should remain very small.

Example:

```yaml
context:
  known:
    report: The result is stale after concurrent writes.

  locations:
    code:
      uri: local://src/cache.rs

checks:
  race:
    ask: Is a race condition a plausible explanation?
    uses:
      - known.report
      - locations.code
    evidence: auto

  reachable:
    ask: Is the problematic path realistically reachable?
    uses:
      - locations.code
    evidence: required

decision: race && reachable
```

That should be enough for the majority of agent use cases.

---

# 57. Future: Dynamic Research Stages

Later:

```yaml
stage:
  candidate:
    type: choice
    ask: Which component is most likely responsible?

next:
  when: candidate == "cache"
  resolve:
    - cache_implementation

checks:
  ...
```

This would turn OJQL into a probabilistic research graph.

Do not build this initially.

---

# 58. Future: Composite Probabilities

Later support derived scores:

```yaml
derive:
  bug_probability: >
    (
      answers.race.noul * 0.4 +
      answers.reachable.noul * 0.35 +
      answers.matches_report.noul * 0.25
    )

decision: bug_probability >= 0.75
```

Potentially useful for ranking and research heuristics.

Again, not needed for v0.1.

---

# 59. Future: Search Locations

Locations could eventually be more than static files.

Example:

```yaml
locations:
  callers:
    search:
      symbol: cache.invalidate
      relation: callers
```

or:

```yaml
locations:
  suspicious_files:
    search:
      query: cache invalidation
      limit: 5
```

This would connect OJQL more deeply to Octocode's research engine.

But static references should come first.

---

# 60. Implementation Plan

## Phase 1 — Core Types

Create:

```text
packages/octocode-jev/
```

Implement:

```text
Query
Context
KnownContext
Location
Check
EvidencePolicy
JEVAnswer
CheckResult
QueryResult
```

No JEV calls yet.

---

## Phase 2 — Schema Validation

Create JSON Schema for the public query structure.

Implement:

```text
syntactic validation

reference validation

type-specific validation

expression validation
```

---

## Phase 3 — Expression Engine

Integrate CEL or equivalent safe expression runtime.

Support:

```text
boolean checks

threshold comparisons

params

Choice output

Score output

confidence

probabilities
```

---

## Phase 4 — JEV Client

Implement minimal client:

```text
Noul

Choice

Score

batch questions

usage/result normalization
```

Keep JEV API-specific code isolated.

---

## Phase 5 — Basic Planner

Implement:

```text
group checks by required state

batch questions

split based on context budget

aggregate answers
```

At this point support only:

```text
evidence: none
evidence: required
```

---

## Phase 6 — Octocode Context Adapter

Inside Octocode tools-core implement:

```text
local:// resolution

github:// resolution

range resolution

content hashing

permissions/security
```

Convert Octocode locations into resolved context values before calling `octocode-jev`.

---

## Phase 7 — MCP Tool

Add:

```text
jevQuery
```

Tool flow:

```text
receive structured query
    ↓
validate
    ↓
resolve context
    ↓
execute octocode-jev
    ↓
return structured result
```

---

## Phase 8 — Known Context

Add:

```text
context.known
```

Allow JEV checks to use only agent-provided knowledge.

Measure:

```text
latency

JEV cost

accuracy

agent usefulness
```

---

## Phase 9 — Auto Evidence

Implement:

```text
evidence: auto
```

Flow:

```text
cheap known-context judgment
+
context-sufficiency judgment
        ↓
resolve evidence only when necessary
        ↓
refine judgment
```

Benchmark heavily.

This behavior should not be trusted until measured.

---

## Phase 10 — Context Snapshots

Implement:

```text
context_ref

context.inherit
```

Store:

```text
known values

location metadata

resolved hashes

resolved evidence handles

provenance
```

Use immutable snapshots.

---

## Phase 11 — Context Return Policies

Support:

```text
none

manifest

used
```

Measure token impact on agents.

---

## Phase 12 — Agent Skill

Create Octocode JEV skill teaching agents:

```text
when JEV is useful

how to decompose questions

how to use evidence modes

how to select thresholds

how to use context inheritance

when not to use JEV
```

---

# 61. Benchmarks

This feature needs benchmarks from the beginning.

Measure at least:

```text
JEV requests per task

JEV questions per task

locations resolved

files read

tokens sent to JEV

tokens returned to agent

latency

cost

decision accuracy

context-gate accuracy

false "context sufficient" rate

false "need more evidence" rate
```

Especially important:

```text
evidence: auto
```

must prove that it saves retrieval/cost without materially hurting decisions.

---

# 62. Example Benchmark

Dataset:

```text
100 code-investigation questions
```

Compare:

```text
A. agent reasoning only

B. JEV with required evidence

C. JEV auto evidence

D. JEV known context only
```

Measure:

```text
accuracy

files fetched

JEV cost

total latency

agent tokens

false conclusions
```

The important experiment:

> How often can `known + auto` avoid code retrieval without losing useful accuracy?

---

# 63. Product Position

This is not merely a JEV wrapper.

A basic JEV wrapper is:

```text
question + state
    ↓
JEV
    ↓
probability
```

Octocode-JEV becomes:

```text
agent knowledge
+
potential evidence locations
+
lazy retrieval
+
multiple atomic probabilistic judgments
+
deterministic composition
+
context reuse
+
research-loop integration
```

That is a substantially stronger abstraction.

---

# 64. Relationship to Octocode Research

Current Octocode model:

```text
search
retrieve
inspect
reason
search
retrieve
inspect
reason
```

With JEV:

```text
search
retrieve
inspect
JUDGE
reason
search
JUDGE
retrieve only if needed
JUDGE
continue
```

The new primitive is:

```text
JUDGE
```

JEV gives Octocode a structured way to answer:

```text
How strongly does the current evidence support X?
```

---

# 65. Mental Model

The easiest explanation:

> **Octocode-JEV lets an agent write probabilistic conditional logic over code and evidence.**

Example:

```text
if (
    code_has_bug &&
    bug_is_reachable &&
    fix_matches_root_cause
)
```

The challenge is that those variables do not normally exist.

Octocode-JEV creates them.

Each fuzzy variable becomes a JEV judgment:

```text
code_has_bug       → JEV

bug_is_reachable   → JEV

fix_matches_root_cause → JEV
```

Then normal deterministic logic takes over.

---

# 66. High-Level Architecture

```text
                          AGENT
                            │
                            │
                        OJQL query
                            │
                            ▼
                 ┌────────────────────┐
                 │ Octocode Tools Core│
                 └─────────┬──────────┘
                           │
              ┌────────────┴────────────┐
              │                         │
              ▼                         ▼
      Context Providers           Octocode-JEV
                                  Query Runtime
      GitHub                       │
      Local FS                     ├── validator
      Search                       ├── planner
      LSP                          ├── context manager
      Repo engine                  ├── evaluator
                                  └── JEV client
                                       │
                                       ▼
                                      JEV
                                       │
                                       ▼
                              probabilistic answers
                                       │
                                       ▼
                              deterministic decision
                                       │
                                       ▼
                                context snapshot
                                       │
                                       ▼
                                     AGENT
```

---

# 67. Final Design Principles

### 1. Keep judgments atomic

```text
one fuzzy question
=
one JEV question
```

---

### 2. Batch aggressively

```text
one question
≠
one HTTP call
```

---

### 3. Separate beliefs from evidence

```text
known != verified evidence
```

---

### 4. Retrieve lazily

```text
location != automatic file read
```

---

### 5. Keep control flow deterministic

```text
JEV gives probabilities

Octocode evaluates conditions
```

---

### 6. Keep agents cheap

Allow:

```text
questions + known context + locations
```

without repeatedly resending everything.

---

### 7. Preserve context

Use:

```text
context_ref
```

to make iterative research compact.

---

### 8. Keep v0.1 small

Do not accidentally build another workflow engine.

---

### 9. Measure everything

Especially:

```text
auto evidence resolution
```

because it is potentially one of the most valuable features, but also one that must be proven.

---

# 68. Proposed v0.1 Deliverable

The first useful Octocode-JEV version should support:

```text
@octocodeai/octocode-jev package

Noul / Choice / Score

known context

location references

required evidence

atomic checks

JEV batching

pass predicates

CEL decisions

structured result

MCP jevQuery tool

basic context manifest
```

Then add:

```text
auto evidence

context_ref / inherit

semantic question linting

caching

advanced planning
```

after the fundamental model has been validated.

---

# 69. One-Sentence Definition

> **Octocode-JEV is a probabilistic decision runtime for agents: agents define fuzzy conditions over what they know and where evidence exists, Octocode retrieves evidence when necessary, JEV evaluates each atomic judgment, and deterministic logic combines the results into a reusable decision and context.**

---

# 70. Short Product Definition

> **An agentic `if` statement over code and evidence.**

```text
Agent asks:
"If A and B, or C, should I continue?"

Octocode determines what evidence is necessary.

JEV evaluates A, B, and C.

Octocode returns:
- probabilities
- condition results
- final decision
- evidence/context used
```

That is the core of Octocode-JEV.
