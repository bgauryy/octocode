import type { InlineSegment } from './components.js';
import type { UxPriority, UxSnapshotV1 } from '../tools/ux-snapshot.js';
import { isRecentAgentOutcome } from '../tools/agents/display-state.js';
import { truncateToWidth } from './width.js';

export interface AgentOutcomeCandidate {
  id: string;
  priority: UxPriority;
  order: number;
  segments: InlineSegment[];
  detailRoute: string;
}

export function recentAgentOutcomeCandidates(
  snapshot: UxSnapshotV1,
  width: number,
): AgentOutcomeCandidate[] {
  return snapshot.agents
    .filter(agent => isRecentAgentOutcome(agent, snapshot.observedAt))
    .sort((a, b) => b.updatedAt - a.updatedAt || a.id.localeCompare(b.id))
    .map((agent, index) => ({
      id: `worker-outcome:${agent.id}`,
      priority: 'P3',
      order: 1_500 + index,
      segments: [
        {
          text: agent.state,
          token: agent.state === 'done' ? 'success' : 'muted',
          keepWhole: true,
        },
        {
          text: truncateToWidth(
            agent.label,
            Math.max(4, Math.min(24, Math.floor(width / 4))),
          ),
          token: 'brand',
        },
        ...(agent.activeOperation ?? agent.assignment
          ? [{ text: agent.activeOperation ?? agent.assignment!, token: 'muted' as const }]
          : []),
        { text: '/octocode-inbox', token: 'link' },
      ],
      detailRoute: '/octocode-inbox',
    }));
}
