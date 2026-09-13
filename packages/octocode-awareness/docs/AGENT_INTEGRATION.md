# Integrating any agent

Use the CLI from an agent with shell access, or import the client into a host. Both execute the same operation descriptors. Awareness owns observations, assessment, advisories, and recorded feedback; the host owns execution, compaction, concurrency, and model selection.

## Import the instruction sections

```ts
import {
  createAwarenessClient,
  getAwarenessAgentInstructions,
  AWARENESS_AGENT_INSTRUCTION_SECTIONS,
} from '@octocodeai/octocode-awareness';

const instructions = getAwarenessAgentInstructions();
const feedbackInstructions = getAwarenessAgentInstructions({
  sections: ['observe', 'advise', 'feedback'],
});
```

The renderer is pure and opens no database. A host may keep the minimal external kernel standing and load only the section needed for the next action. The available sections are `start`, `observe`, `advise`, `feedback`, `coordination`, `trust`, and `schema`; selection preserves canonical order and removes duplicates. Unknown sections fail explicitly. The package owns these rules; hosts supply only trusted bindings and delivery adapters.

## CLI discovery

```bash
npx -y @octocodeai/octocode-awareness --help
npx -y @octocodeai/octocode-awareness instructions
npx -y @octocodeai/octocode-awareness instructions --section observe --section feedback
npx -y @octocodeai/octocode-awareness schema command context observe --compact
npx -y @octocodeai/octocode-awareness schema command context feedback --compact
```

The lobby explains entry and discovery. `instructions` renders the same text as the imported function. Load exact operation schemas only when unfamiliar; avoid reinjecting catalogs on every call. The bundled `octocode-awareness` skill routes agents to this same instruction source.

For a context-only report, replace these example readings with actual host measurements and reuse the host's workspace, actor, session, and database bindings:

```bash
npx -y @octocodeai/octocode-awareness context observe \
  --workspace "$(pwd)" --agent-id worker --session-id session-1 \
  --observation-id "context-$(date +%s)" \
  --observed-at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --source host \
  --context '{"used":9500,"limit":10000}' --compact
```

## Bind one client

```ts
const awareness = createAwarenessClient({
  workspace: '/absolute/workspace',
  agentId: 'stable-host-assigned-actor',
  sessionId: 'host-assigned-session',
});

const first = await awareness.orient();
const refreshed = await awareness.orient({ if_revision: first.revision });
const observationContract = awareness.operations().find(
  descriptor => descriptor.operation === 'context.observe',
);
```

Select the intended database/scope in trusted host configuration and preserve those bindings across calls and continuations. Separate stores do not coordinate. Reuse an unchanged orientation; refresh after relevant observations, feedback, or shared-state changes.

## Run the feedback loop

Two flows share the same assessment:

| Flow | Entry | Result |
|---|---|---|
| Active inspection | `context.orient` after a relevant observation | `run_state`, a brief `regulation.nudge`, and underlying advisories |
| Passive sensing | `context.observe --acquisition passive` from a host lifecycle callback | Receipt, interpreted `run_state`, and one `nudge` when a new episode appears |

`source` identifies who supplied a measurement; `acquisition` identifies how it arrived. Both flows work through the CLI or imported client. A passive callback reports available measurements instead of asking the model to inspect its own runtime. The host delivers an offered nudge at its next safe boundary, deduplicates accepted delivery by `advisory_id`, and persists acceptance before advancing its event cursor. An absent nudge causes no interruption or follow-up poll. Replaying an observation preserves the nudge ID so a failed delivery can be retried. Additional conditions remain inspectable with `context.orient`.

`run_state.status` is `pressured`, `stuck`, `disrupted`, `progressing`, `observed`, or `unknown`. These are interpretations of attributed measurements. `observed` does not claim overall health or completion. Dismissing a nudge leaves the measured condition intact; missing readings do not prove recovery. Passive flow is enabled by a caller supplying lifecycle observations, not by installing a background daemon.

```ts
const receipt = await awareness.observe({
  observation_id: 'context-sample-1', observed_at: new Date().toISOString(),
  source: 'host', acquisition: 'passive',
  context: hostMeasuredContext, // { used, limit } from the host
});
// Check exitCode, then persist/deliver payload.nudge if present.
```

1. **Observe:** send available measurements or attributed self-reports through `context.observe`. Missing readings remain unknown. Equivalent retry identities and evidence identities allow repetition assessment without storing complete tool output.
2. **Assess and advise:** consume regulation from `context.orient`. The agent or host decides whether the nudge fits the current task and authorized scope.
3. **Act and verify:** perform the host action, submit a subsequent observation, and check whether the condition improved; delivery or tool success alone does not prove benefit.
4. **Record feedback:** link the advisory, action, and later `observation_id` through `context.feedback`. A `helpful` outcome requires that subsequent observation; use `unresolved` for unknown results and `unnecessary` for unhelpful interruptions. Orient again after feedback.

This function uses the client above and accepts real host measurement/compaction callbacks. It records helpful feedback only when measured context usage decreases:

```ts
import { randomUUID } from 'node:crypto';

async function relieveContextPressure(host: {
  readContext(): { used: number; limit: number };
  compact(): Promise<void>;
}) {
  const before = host.readContext();
  await awareness.observe({
    observation_id: randomUUID(), observed_at: new Date().toISOString(),
    source: 'host', context: before,
  });
  const packet = await awareness.orient();
  const advisory = !packet.unchanged
    ? packet.regulation.advisories?.find(item => item.kind === 'context-pressure')
    : undefined;
  if (!advisory) return;

  await host.compact(); // The host preserves the objective and compacts externally.
  const after = host.readContext();
  const observationId = randomUUID();
  await awareness.observe({
    observation_id: observationId, observed_at: new Date().toISOString(),
    source: 'host', context: after,
  });
  await awareness.feedback({
    feedback_id: randomUUID(), observed_at: new Date().toISOString(),
    advisory_id: advisory.id, action_taken: 'Host compacted context',
    outcome: after.used / after.limit < before.used / before.limit
      ? 'helpful' : 'unresolved',
    observation_id: observationId,
  });
  return awareness.orient();
}
```

Observations become stale after five minutes; submit a new measurement when needed. Observation, feedback, and advisory IDs accept 1–128 ASCII characters from `A–Z`, `a–z`, `0–9`, `_`, `.`, `:`, and `-`. Timestamps use ISO 8601 with a timezone. These calls do not install automatic monitoring: the host or agent must supply observations and invoke the loop.

Self-monitoring works during solo tasks; coordination operations are needed only when ownership, messages, dependencies, or recovery affect work. Host measurements and agent self-reports retain distinct provenance. Unavailable observations do not establish degraded recovery, and advisories grant no new permissions.

Evaluate changes using useful interventions and unnecessary interruptions alongside repeated failures, verified progress, and observation cost. These are evaluation goals, not claims that the current implementation improves agent productivity.
