# Agent Defect Evidence

Load when deciding how much review a class deserves, or when a reviewer disputes that a class is real. Why: prevalence is measured per class, so rigor can follow evidence instead of taste.

Each row states the finding and what it licenses here. Rates are priority signals for review attention, never per-repository predictions.

## Measured findings

| Finding | What it licenses |
|---|---|
| Concurrency bug density spans 7x across models, from 69 to 470 per million lines, and ranges from under 3% to nearly 50% of a model's bugs — [Sonar LLM Leaderboard](https://www.sonarsource.com/blog/gpt-5-5-biggest-blind-spot/) | Reviewing thread safety on every concurrent slice, and knowing the rate is model-dependent |
| The three recurring shapes are unsafe lazy publication, locking on a value-based or interned object, and sleeping while holding a lock; all "compile and pass functional tests but break in production because their correctness depends on thread timing that no test framework controls" — same source | Detecting these structurally rather than by adding tests |
| LLMs "consistently creating severe bugs like resource leaks and API contract violations" — [Sonar, 4,400+ Java tasks](https://www.sonarsource.com/company/press-releases/the-coding-personalities-of-leading-llms/) | Treating resource lifetime and contract compatibility as standing review items |
| Over 90% of all issues found were code smells, for every model evaluated — same source | Expecting structural rather than functional defects as the dominant output |
| A 6.3% pass-rate gain came with a 93% rise in high-severity bugs — same source | Not relaxing review when the model improves |
| Hard-coded credentials and path-traversal injection were common across all models, with blocker-severity shares near 60–70% — same source | Keeping both on the security pass regardless of task type |
| Only 68.3% of generated projects run in a clean environment using only their declared dependencies; Python 89.2%, JavaScript 61.9%, Java 44.0% — [dependency-gap study, 300 projects](https://arxiv.org/html/2512.22387v1) | Requiring a clean-environment run before calling a deliverable done |
| Projects declaring 3 dependencies needed an average of 37 at runtime, a 13.5x expansion — same source | Verifying the declared closure, not just that it runs locally |
| Failures were mostly "malformed syntax, incorrect file paths, uninitialized variables, and structural issues" rather than missing imports (10.5%) — same source | Reading the artifact's structure, not only its manifest |
| Architectural drift is "step by step, and commit by commit, you get to a situation where your architecture is different from the intentions… cyclic dependencies and connections that shouldn't be there" — [SIG](https://www.softwareimprovementgroup.com/blog/architectural-debt-ai/) | Stating intended boundaries before a slice so drift becomes measurable |
| An agent-swarm browser engine of over 3 million Rust lines scored 1.3/5 maintainability, bottom 5% of systems measured, with architecture quality 2.1/5, "tightly coupled" and low modularity — same source | Treating volume as unrelated to structural quality |
| 333 hand-classified bugs yield 10 validated patterns including missing corner case, wrong input type, hallucinated object, wrong attribute, incomplete generation, and prompt-biased code — [bug pattern taxonomy](https://arxiv.org/abs/2403.08937) | The specification-fit checklist |
| Agents pick the correct file in 72–81% of failed attempts — [trajectory study](https://arxiv.org/abs/2511.00197) | Reviewing what changed rather than whether the right place was found |

## Named incidents and agent artifacts

Single cases, not rates. They license inspecting a class, not predicting its frequency.

| Case | What it licenses |
|---|---|
| Adding a required method to a public interface reverted a WooCommerce release, because external implementers fataled on load once they no longer satisfied the contract — [WooCommerce AGENTS.md](https://github.com/woocommerce/woocommerce/blob/trunk/AGENTS.md) | Treating any addition to an externally implementable interface as breaking, and deprecating instead of renaming |
| An agent's own wait mechanism was also its wake-up generator: each backgrounded completion re-invoked the agent, leaving "no terminal state while timers are pending" so the run "ends only when a human kills it" — [claude-code #90930](https://github.com/anthropics/claude-code/issues/90930) | The termination row — bounding poll loops and checking that a completion signal cannot re-arm its own waiter |
| A dependency-graph gate kept reporting green after its analyzer stopped being able to parse the sources, because the tool "exits 0 anyway" — [nearform/lastlight](https://github.com/nearform/lastlight/blob/main/CLAUDE.md) | The gate-integrity row — proving a guard rejects a case it must reject before trusting it |
| Agent-facing rule sets describe premature interfaces as the default failure — "every service gets an interface and one implementation… the interface is pure ceremony" — and dependency creep as adding a library for a concern an existing one covers — [code-slop skill](https://github.com/AsyrafHussin/agent-skills/tree/main/skills/code-slop) | Inspecting implementation counts and the existing dependency set; these are practitioner claims, not measurements |

## Evidence limits

The Sonar and SIG figures are vendor-measured, and the bug-pattern taxonomy predates current agents, so use its shapes rather than its rates. The browser-engine system was published as an experiment, not production.

Three adjacent classes stay weakly evidenced — type-checker escape hatches added to silence an error, N+1 and missing-index query defects, and observability gaps on new failure paths — so review them on merit and do not cite a rate. On missing timeouts, one figure circulates second-hand: the code-slop skill attributes "76% of AI-assisted PRs miss timeouts on external calls" to an OX Security 2025 report that is not verified at its primary source, so keep timeouts on the checklist and do not quote the number as measured.

Agent-facing rule files are normative artifacts rather than observations, and they disagree with each other — some forbid removing logging while others forbid adding it — so treat a rule's existence as evidence that practitioners hit the class, never as evidence of its rate. Copied rule files also inflate apparent consensus: the same text recurs verbatim across forks, so count a canonical source once.

Next: to classify a finding load `references/agent-defect-classes.md`; for coordination defects across parallel agents, the `octocode-agents-communication` package docs own that evidence.
