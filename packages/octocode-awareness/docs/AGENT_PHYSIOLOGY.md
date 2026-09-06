# Agent Physiology

Agent Physiology separates the agent's computational condition from its external
observations. The operational body includes context, evidence, memory, tools,
workers, execution, budget, and mutations. Computational interoception measures
that condition; cognitive homeostasis keeps operation within explicit ranges.
These terms describe engineering responsibilities, not emotions or sentience.

## Implemented workspace sensing

`attend` returns `operational_state` and `regulation` in both compact and full
output. No extra flag enables them. The state has `scope: workspace_observation`;
it describes observations available to the selected workspace query.

| State | Meaning | Limit |
|---|---|---|
| `verification.owned_observed` | Owned verification rows in the observed packet | A bounded observation, not all owned debt |
| `verification.total` | Total verification rows reported by the workspace view | Includes other agents; does not prove task quality |
| `coordination.overlaps_observed` | Selected file rows showing peer overlap | Presence is advisory; overlap does not prove conflict |
| `coordination.locks_observed` | Selected file rows showing locks | Inspect the exact path and holder before acting |
| `evidence.recalled` | Leads in the bounded recall before compact display limits | Does not measure evidence coverage |
| `evidence.reference_warnings` | Reference trust warnings in that bounded recall | No warning does not prove correctness or consistency |
| `coverage.bounded` | Always true: this is a bounded observation | Does not promise a complete census |
| `coverage.omitted_rows` | Rows omitted by the bounded source workboard query before compact display formatting | Does not count further compact display omissions or prove complete task knowledge |
| `unavailable` | Sensors this observation does not provide | Absence means unknown, never zero pressure |

The implementation lives in the [attend query](../src/attend-query.ts) and its
[result contract](../src/attend-model.ts). SQLite remains the source of Awareness
records; the snapshot does not establish repository truth.

## Three control speeds

| Layer | Owner | Behavior |
|---|---|---|
| Reflex | Owning host's admission path and installed hooks | Enforce supported prerequisites and lock guards at the action boundary |
| Regulation | Awareness CLI/library | Turn observed facts into bounded, deterministic advice |
| Deliberation | Reasoning model | Interpret evidence, explain hypotheses, choose an authorized strategy |

`regulation.advisory` is true. Its `actions` can request `verify_owned_work`,
`inspect_overlap`, `inspect_lock`, `revalidate_memory`, `narrow_read`,
`inspect_recent_tool_failures`, or `inspect_context_headroom`.
The packet's next action uses these observations. Advice neither executes tools
nor grants authority, creates a check receipt, or bypasses policy. When there is
no actionable shared state, the agent continues its authorized task without a
recursive attend request.

Installed [hooks](HOOKS.md) already provide bounded reflexes around supported host
events. They do not become a general runtime controller because the CLI exposes
operational state. Skill instructions teach how to consume the packet; they do
not install hooks, sample a model's context, or schedule workers.

The [notification briefing](../src/maintenance-briefing.ts) suppresses unchanged
output for the same consumer and scope. After a successful empty observation,
the same pressure can emit again: `A -> empty -> A` represents a recurrence.
A failed observation does not establish recovery. Empty means the selected
briefing for this prompt, not resolution of every workspace issue. This feedback
restores a useful pointer without repeating unchanged context or adding a
background loop.

## Native runtime sensing and enforced feedback

The native CLI attaches a session-local `runtime` observation to the admitted
`awareness` tool's `attend` result. The ordinary standalone Awareness CLI has no
runtime connection and retains its explicit unavailable sensors. Native model
arguments cannot supply or override the observation. Awareness accepts the host
observation through a separate typed context and copies only declared numeric
fields; it never imports agent core or reads the Agent databases.

| Runtime field | Observation and bound |
|---|---|
| `context.current_tokens` / `measured_at` | Core's last current-context token measurement, not cumulative billing usage; invalidated after input, appended context, tool settlement, compaction, model selection, a new provider request, or runtime stop until the next core measurement |
| `context.input_limit_tokens` / `remaining_input_tokens` / `saturation_basis_points` | Numeric limit, headroom, and occupancy ratio. Native emits them only when the current-token measurement is fresh and its active-model limit matches; model identity remains internal. With an unknown limit, it retains raw occupancy and omits the ratio and headroom. |
| `tools` | Last 32 non-Awareness tool settlements in this runtime instance, separated into failed, cancelled, blocked, and observed counts; Awareness status calls cannot displace failures by polling |
| `controls.compactions_committed` / `compactions_failed` | Counts of actual core compaction receipts observed by this instance, with no summary content retained |
| `controls.retries_scheduled` | Actual core provider retry decisions; a scheduled retry does not establish successful recovery |
| `controls.provider_attempt` / `provider_max_attempts` | Most recent provider attempt and the limit published by core for that request; absent before a request |

A tool sample with failures adds deterministic `inspect_recent_tool_failures`
advice. Successful later tool settlements age failures out of the fixed window.
Empty samples remain unavailable, and cancellation or a policy block is not
classified as a tool failure. No tool names, arguments, outputs, provider error
bodies, or compaction summaries are stored in this projection. It resets when the
runtime is recreated; it is not a durable session-wide failure census.

The actuator already lives in core: provider retries stop at the request's
`maxAttempts`; compaction commits through the durable service before replacing
live context; preflight compaction is bounded to one attempt per iteration and
hard model input limits reject before provider admission. Native resolves those
limits from the active model on each preflight, so selecting a smaller model
cannot retain the larger launch model's input budget. The physiology adapter
observes those actual decisions after event persistence. It does not create a
second retry loop, replay effects, or authorize a compaction from advisory data.
Saturation guidance can request a bounded read or re-observation, but it cannot
start compaction or change the selected model.
The native integration test drives a failing provider through core, observes
exactly two attempts under a two-attempt ceiling, then reads the resulting retry
receipt through the admitted native Awareness tool. Existing core compaction and
input-budget tests own those enforcement boundaries.

## Host capabilities without sensors

| Proposed capability | Required evidence and owner |
|---|---|
| Context saturation and remaining budget | Native reports saturation and headroom only for a fresh occupancy measurement with a matching active-model input limit. Unknown limits preserve occupancy but omit normalized values; task budgets remain a separate capability. |
| Repetition reflexes | Native exposes a bounded tool outcome sample; repetition still requires canonical action identity and fresh-observation boundaries |
| Uncertainty and evidence coverage | Explicit task acceptance criteria and provenance; task or search counts are insufficient |
| Cognitive metabolism | Host resource costs paired with independently verified progress or information gain |
| Error pressure | Validated combination of measured failures, spend, uncertainty, and reversibility with explicit thresholds |
| Prospective interoception | Host action forecast with calibration, assumptions, and observed prediction error |
| Worker regulation | Host worker lifecycle, dependencies, merge costs, and budget; shared file presence alone is insufficient |
| Branch divergence and rollback cost | Repository and effect evidence from the owning host; never infer from open task counts |

The remaining sensors stay in `unavailable`; native removes `context` only while
a current measurement exists and `tool_health` only when the sample is nonempty. No normalized health, entropy, or yield score substitutes for missing
measurements. Memory hygiene remains a separate operation from resource-to-progress
metabolism. Decomposition can bound work and verification, but does not establish
a measured forecast of the next action's resource use.

## Acceptance criteria

A control earns stronger authority only with an observable sensor, declared range,
bounded actuator, guard, and feedback. Start with advisory output; host enforcement
requires tests at its actual admission boundary.

- The same observed facts produce the same advice; unknown sensors stay explicit.
- Limits preserve omissions and avoid treating a sample as a complete measurement.
- Read-only sensing does not mutate work or declare verification success.
- Peer overlap stays advisory; real locks retain their existing enforcement.
- Unchanged or empty state does not create a polling loop.
- A correction reduces its measured target without regressing safety, quality,
  output size, or useful work on an independent scenario.
- A future repetition reflex distinguishes a retry after fresh observation from
  an identical unobserved replay; a resource forecast records calibration error.

The [homeostatic thesis](THESIS.md) owns the overall control model. The canonical
Awareness skill owns the short operating policy and conditional detail routes.
