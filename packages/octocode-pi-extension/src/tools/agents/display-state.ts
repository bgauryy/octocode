export type WorkerDisplayState =
  | 'starting'
  | 'queued'
  | 'running'
  | 'idle'
  | 'done'
  | 'blocked'
  | 'failed'
  | 'killed';

/** Keep a just-settled worker visible long enough for the outcome to be noticed. */
export const RECENT_AGENT_OUTCOME_MS = 10_000;

export function isRecentAgentOutcome(
  entry: {
    state?: string;
    status?: string;
    normalizedStatus?: string;
    pendingMessages?: number;
    updatedAt: number;
  },
  now = Date.now(),
): boolean {
  const state = effectiveAgentStatus({
    ...entry,
    status: entry.status ?? entry.state,
  });
  return (
    (state === 'done' || state === 'killed') &&
    Math.max(0, now - entry.updatedAt) <= RECENT_AGENT_OUTCOME_MS
  );
}

/** Shared by the footer, inbox, event journal, and agent result cards. */
export function effectiveAgentStatus(entry: {
  status?: string;
  normalizedStatus?: string;
  pendingMessages?: number;
}): WorkerDisplayState {
  const processStatus = entry.status?.toLowerCase() ?? 'starting';
  const outcome = entry.normalizedStatus?.toLowerCase();
  if (processStatus === 'killed') return 'killed';
  if (
    processStatus === 'failed' ||
    processStatus === 'error'
  )
    return 'failed';
  if (processStatus === 'running') return 'running';
  const exited = ['done', 'completed', 'exited'].includes(processStatus);
  // A queued follow-up supersedes the previous handback before its next turn starts.
  if (!exited && (entry.pendingMessages ?? 0) > 0) return 'queued';
  if (outcome === 'failed') return 'failed';
  if (outcome === 'blocked' || processStatus === 'blocked') return 'blocked';
  if (exited || outcome === 'done') return 'done';
  return processStatus === 'idle' ? 'idle' : 'starting';
}
