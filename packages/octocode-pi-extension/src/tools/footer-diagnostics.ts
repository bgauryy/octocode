import type { InlineSegment } from '../tui/components.js';
import type { StatusDiagnosticV1 } from '../tui/status-policy.js';
import type { RuntimeFooterState, RuntimeState } from './runtime-store.js';
import { githubAuthLabel } from './github-auth-status.js';

interface FooterDiagnosticsInput {
  identity: InlineSegment[];
  metrics: InlineSegment[];
  statuses: RuntimeState['statuses'];
  githubStatus: RuntimeFooterState['githubAuth']['status'];
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
  // Authenticated is the happy path — hide it; the session row already links /configuration.
  // Show only when there is an actionable or transient state the operator should notice.
  if (input.githubStatus !== 'authenticated') {
    rows.push({
      id: 'github',
      priority: 'P3',
      segments: [
        { text: githubAuthLabel(input.githubStatus), token: input.githubStatus === 'checking' ? 'dim' : 'warning' },
        { text: '/configuration', token: 'link' },
      ],
    });
  }
  if (input.metrics.length > 0)
    rows.push({ id: 'metrics', priority: 'P4', segments: input.metrics });
  const peerEvents = input.statuses['octocode-communication-events'];
  if (peerEvents)
    rows.push(
      attentionRow('communication-delivery', 'P2', peerEvents, '/octocode-inbox')
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
  return rows;
}
