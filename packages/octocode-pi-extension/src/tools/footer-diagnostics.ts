import type { InlineSegment } from '../tui/components.js';
import type { StatusDiagnosticV1 } from '../tui/status-policy.js';
import type { AwarenessStatusHealth, CachedAwarenessStatus } from './awareness-status.js';
import type { RuntimeFooterState, RuntimeState } from './runtime-store.js';
import { githubAuthLabel } from './github-auth-status.js';

interface FooterDiagnosticsInput {
  identity: InlineSegment[];
  metrics: InlineSegment[];
  statuses: RuntimeState['statuses'];
  githubStatus: RuntimeFooterState['githubAuth']['status'];
  awareness: CachedAwarenessStatus | null;
  awarenessHealth: AwarenessStatusHealth;
}

function attentionRow(
  id: string,
  priority: StatusDiagnosticV1['priority'],
  text: string,
  route: string
): StatusDiagnosticV1 {
  return {
    id,
    priority,
    segments: [
      { text, token: 'warning', attention: true },
      { text: route, token: 'link', attention: true },
    ],
  };
}

/** Project sampled runtime facts into stable footer diagnostic rows. */
export function buildFooterDiagnostics(
  input: FooterDiagnosticsInput
): StatusDiagnosticV1[] {
  const rows: StatusDiagnosticV1[] = [
    {
      id: 'session',
      priority: 'P4',
      segments: input.identity.map((segment, index) => ({
        ...segment,
        keepWhole: index > 0 && !segment.text.startsWith('/'),
      })),
    },
  ];
  const eventLog = input.statuses['octocode-event-log'];
  if (eventLog)
    rows.push(attentionRow('event-log', 'P1', eventLog, '/octocode-status events'));
  rows.push({
    id: 'github',
    priority: 'P3',
    segments: [
      { text: githubAuthLabel(input.githubStatus), token: input.githubStatus === 'authenticated'
        ? 'success' : input.githubStatus === 'checking' ? 'dim' : 'warning' },
      { text: '/configuration', token: 'link' },
    ],
  });
  if (input.metrics.length > 0)
    rows.push({ id: 'metrics', priority: 'P4', segments: input.metrics });
  if (input.awareness?.verifyTasks)
    rows.push(
      attentionRow(
        'awareness-checks',
        'P1',
        `Verify · ${input.awareness.verifyTasks} checks pending`,
        '/octocode-status'
      )
    );
  const peerEvents = input.statuses['octocode-awareness-events'];
  if (peerEvents)
    rows.push(
      attentionRow('awareness-delivery', 'P2', peerEvents, '/octocode-inbox')
    );
  const sessionMemory = input.statuses['octocode-session-memory'];
  if (sessionMemory)
    rows.push(
      attentionRow(
        'session-memory',
        'P2',
        sessionMemory,
        '/octocode-status context'
      )
    );
  if (input.awarenessHealth.state === 'unavailable')
    rows.push(
      attentionRow(
        'awareness-source',
        'P2',
        input.awareness
          ? 'Awareness unavailable · showing last known state'
          : 'Awareness status unavailable',
        '/octocode-status'
      )
    );
  return rows;
}
