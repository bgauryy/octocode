import type { PlanReadModelV1 } from '../../src/tools/plan-read-model.js';
import { deriveUxSnapshot } from '../../src/tools/ux-snapshot.js';
import { selectStatusRows } from '../../src/tui/status-policy.js';

/** Select the production plan row without initializing a host or workspace. */
export function projectPlanStatus(model: PlanReadModelV1) {
  const snapshot = deriveUxSnapshot({
    now: 0,
    runtime: {
      generation: 0,
      phase: 'ready',
      activity: { kind: 'idle' },
      context: {
        status: 'pending',
        mode: 'routing',
        systemPromptChars: 0,
        mcpChars: 0,
        dynamicChars: 0,
        directToolChars: 0,
        providerSubtotalChars: 0,
        estimatedTokens: 0,
        mcpServers: 0,
        mcpTools: 0,
        skills: 0,
      },
      footer: {
        sessionStartedAt: 0,
        completedTurns: 0,
        githubAuth: { status: 'checking' },
      },
    },
    plan: model,
  });
  const selection = selectStatusRows(snapshot, {
    width: 120,
    height: 40,
    density: 'expanded',
  });
  return selection.rows[selection.rowIds.indexOf('plan:progress')] ?? [];
}
