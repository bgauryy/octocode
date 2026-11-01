Load when selecting a workflow, owner skill, actor split, completion evidence, recovery route, or limit.

# Workflow catalog

Choose one primary card; techniques and role patterns compose with it. During execution, use one orchestrator, at least one actor, and the [single ledger](verification.md#keep-one-compact-coverage-ledger). A proposed selection needs no ledger until work begins. Owner skills retain their method, permissions, and output.

Every entry below and every [role pattern](role-patterns.md) inherits the [shared Octocode tool access](composition.md#octocode-tools-for-every-role). The owner/helper columns route domain methods; they do not restrict which actors can use Octocode tools.

## Execution patterns

| Workflow | When / why | Actor roles and owner skills | Handoff, completion, recovery, limits |
|---|---|---|---|
| Bounded execution | A scoped question or change needs one concrete execution step; avoids needless coordination | One domain actor; owner is the relevant specialist, plus Research for code evidence or Architect for changed boundaries | Actor returns anchors/result to orchestrator; proportionate owner check completes; report missing actor capability rather than silent orchestration fallback |
| Single-actor BFS/DFS audit | A declared system scope needs all surfaces, junctions, critical paths, and live behavior; breadth prevents deep-path bias | One traversal actor; Architect + Research, repository dev helper when installed | [Traversal ledger](single-actor-traversal.md); reconciled inventory and explicit gaps complete; no fan-out, untested runtime stays unverified |
| Runtime surgery | A changed running feature requires process evidence separate from source change | Code/architecture actor and runtime actor; Architect + Research + Agents communication | [Feature loop](runtime-surgery.md); matching loaded identity and replay complete; unavailable runtime actor is a reported limitation |
| Independent evidence lanes | Separable questions or risk areas justify independent raw evidence, and delegation is authorized | Assigned evidence actors; domain owner, Brainstorming/Research/Architect/Eval as applicable | Row-keyed anchors and counterevidence merge at orchestrator; complete only after deciding evidence is reread; no recursive spawning or duplicate summaries |
| Parallel change lanes | Independent change items touch separable files, and concurrent writes are authorized | One actor per path partition; Architect for boundaries, the repository dev helper for build and contract steps | [Parallel write lanes](composition.md#parallel-write-lanes): owned paths, a locked generator, and one builder. Done when the orchestrator's full checks pass on the merged tree. Cross-lane needs return as requests |
| Independent acceptance | A material contract has a useful independently derived check, and delegation is authorized | Builder actor and inspection actor; domain owner, Architect/Research/Documentation as applicable | Inspector receives contract/inputs before conclusion; stable oracle plus consumer/edge evidence completes; inspector does not race with fixes |
| Hypothesis checking | Rival explanations lead to different action | Investigator actor and discriminating-check actor; Research + domain owner, Eval for controlled claims | Cause, strongest rival, probe, observation return to one row; a distinguishing outcome or named gap completes; do not repeat unchanged failed probe |
| Stateful procedure | Dependent actions need current-state confirmation | Action actor and optional checking actor; Browser or repository dev/domain owner | Preconditions, durable effect, replay conditions hand off; verified state transition completes; timeout is unknown, inspect state before repeat |

## Evidence techniques

| Technique | When / why | Owner and actor output | Completion and limit |
|---|---|---|---|
| AST structural discovery | Declarations, branches, repeated structure, or candidate graph paths need locating | Research owns live tool semantics; an evidence actor returns exact reads, coverage, and candidates | [AST workflow](ast-lsp-workflows.md#ast-workflow); syntax is not identity, reachability, or runtime proof |
| LSP semantic tracing | Definitions, callers, implementations, types, or consumers determine impact | Research owns semantic tool rules; actor returns resolved anchors and coverage | [LSP workflow](ast-lsp-workflows.md#lsp-workflow); partial/empty results are a gap, not absence |

## Task workflows

| Task family | When / why | Owner skills and actors | Handoff, completion, recovery, limits |
|---|---|---|---|
| Code/package investigation | A repository, upstream, history, dependency, or root-cause claim needs source evidence | Research actor; Architect when structural impact matters | Exact source/consumer anchors complete; use research continuation/recovery rules; a textual hit is not identity |
| Reproduce → fix → verify | Supported behavior is wrong or requested | Domain implementation actor; Architect for boundary, Research for proof, Dev for build pipeline | Stable failing/passing acceptance row completes; diagnose and replan after failure; a plan does not authorize a fix |
| Architecture decision/refactor | Boundaries, ownership, dependencies, or interfaces change | Architect actor; Research, RFC generator, Cleanup conditionally | Decision/proof/change discipline complete; Architect gates apply; no generic workflow rule overrides authorization |
| PR/diff review and repair | A patch needs correctness/impact verdict | Research review actor; Architect/Dev conditional | Changed behavior and consumers evidence completes; report limits; repair only in authorized scope |
| Safe cleanup/unification | Dead code, duplicate policy, wrappers, or junk need removal | Clean-agentic-code actor; Research for reachability, Architect for behavior/boundary | Consumer and behavior proof completes; no empty-search deletion |
| Harsh critique | User explicitly wants blunt critique | Roast actor; Research/Architect prove claims | Ranked demonstrated defects complete; do not manufacture a finding or repair without authority |
| Think, validate, compare | Idea, choice, or landscape remains open | Brainstorming actor; Research/Scraping/Browser/Eval conditionally | Competing evidence and recommendation complete; use Eval if a comparison claim must be measured |
| RFC/design/migration plan | Consequential proposal needs decision and rollout | RFC-generator actor; Architect/Research/Documentation conditionally | Reviewable decision, risks, rollout, rollback complete; plan remains non-implementation authority |
| Public-page corpus | Public pages need extraction, citations, or reuse | Scraping actor; Browser when interactive | Captured/cited evidence completes; static fetch does not replace live interaction |
| Live browser investigation/action | Rendered/logged-in UI, action, console, or network evidence is needed | Chrome-devtools action actor; code owner conditionally | Observed page state/effect completes; uncertain mutation follows browser's stricter replay rule |
| Documentation | README, guide, runbook, ADR, or comment needs accurate prose | Documentation actor; Research/Architect/RFC conditionally | Owner-source-backed content completes; small correction remains direct work |
| Prompt/protocol repair | Instructions cause wrong agent action | Agentic-prompts actor; Skills/Eval conditionally | Effective context and intended action complete; do not mistake paraphrase for compliance |
| Skill creation/maintenance/install | A skill needs authoring, migration, repair, or distribution | Skills actor; Agentic-prompts/Documentation/Eval conditionally | Structure/routes/checker complete; activation benefit needs evaluation |
| Evaluation keep/revert | Variant, model, prompt, skill, or retrieval change must be compared | Eval actor; Research/Prompts/Skills conditionally | Controlled cases and honest denominator complete; sealed evaluation rules prohibit ordinary retry/repair |
| Resource-flow audit | Context growth, repeated work, cache behavior, lost meaning, or runtime resources need causal diagnosis | Plumber actor; Prompts/Research/Architect/Eval conditional | [Measured flow](resource-flow.md) and retained correctness complete; no token-only conclusion |
| Coordination/handoff/resume | Separate sessions share work/evidence or interrupted work resumes | Assigned domain actor; Agents communication transport | [Composition](composition.md#compose-and-resume) plus reconciled ledger completes; delivery alone is not work completion |

## Repository-only and conditional helpers

`octocode-dev` owns this repository's build/test/contract pipeline; `octocode-context-audit` is an optional inventory helper; Rust and Zod are implementation helpers. Discover installed skills before offering them. They are not implicit workflow owners.

Next: use [role patterns](role-patterns.md) for duties or [verification](verification.md) for acceptance and recovery.
