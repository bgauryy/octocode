// ─── Notifications ───────────────────────────────────────────────────────────

export type NotificationKind =
  | 'claim' | 'handoff' | 'question' | 'reply'
  | 'blocker' | 'request' | 'decision' | 'approval' | 'fyi';

export type NotificationStatus = 'open' | 'resolved';

export interface NotificationRecord {
  signal_id: string;
  workspace_path: string;
  artifact: string | null;
  repo: string | null;
  ref: string | null;
  from_agent: string;
  to_agent: string | null;
  kind: NotificationKind;
  subject: string;
  body: string | null;
  files: string[];
  refs: string[];
  thread_id: string;
  reply_to: string | null;
  importance: number;
  status: NotificationStatus;
  created_at: string;
}

export interface InsertNotificationParams {
  agentId: string;
  workspacePath?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  toAgent?: string | null;
  kind: NotificationKind;
  subject: string;
  body?: string | null;
  files?: string[];
  refIds?: string[];             // related task/memory/message ids
  inReplyTo?: string | null;     // inherits thread from parent
  importance?: number;
  cwd?: string;
}

export interface InsertNotificationResult {
  signal_id: string;
  thread_id: string;
  workspace_path: string;
  artifact: string | null;
}

export interface GetNotificationsParams {
  agentId: string;
  workspacePath?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  kinds?: NotificationKind[];
  signalIds?: string[];
  threadId?: string | null;
  unreadOnly?: boolean;          // default true
  markRead?: boolean;            // advance read cursor
  limit?: number;
  cursor?: string;
  cwd?: string;
}

export interface GetNotificationsResult {
  count: number;
  signals: NotificationRecord[];
  unread_only: boolean;
  partial?: boolean;
  partialReasons?: Array<'limit'>;
  next?: { list: { operation: 'agent_signal'; request: Record<string, unknown> } };
}

export interface ResolveNotificationParams {
  agentId?: string | null;
  notificationIds?: string[];
  threadId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  cwd?: string;
}

export interface ResolveNotificationResult {
  resolved: number;
  signal_ids: string[];
}

export interface PruneNotificationsParams {
  agentId: string;
  workspacePath?: string | null;
  artifact?: string | null;
  notificationIds?: string[];
  resolvedOnly?: boolean;
  olderThanDays?: number;
  dryRun?: boolean;
  cwd?: string;
}

export interface PruneNotificationsResult {
  deleted: number;
  dry_run?: true;
  would_delete?: number;
  signal_ids: string[];
}

export type AgentSignalAction = 'publish' | 'list' | 'reply' | 'resolve' | 'ack';

export interface AgentSignalParams {
  action: AgentSignalAction;
  agentId: string;
  workspacePath?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  kind?: NotificationKind;
  subject?: string;
  body?: string | null;
  data?: import('../signal-data.js').SignalData | string;
  toAgents?: string[];
  files?: string[];
  refs?: string[];
  importance?: number;
  inReplyTo?: string | null;
  threadId?: string | null;
  signalIds?: string[];
  unreadOnly?: boolean;
  markRead?: boolean;
  kinds?: NotificationKind[];
  limit?: number;
  cursor?: string;
  cwd?: string;
}

export interface AgentSignalRecord extends NotificationRecord {
  to_agents: string[];
  data?: import('../signal-data.js').SignalData;
}

export interface AgentSignalActionHint {
  operation: 'agent_signal';
  request: Record<string, unknown>;
  draft?: boolean;
  note?: string;
}

export interface AgentSignalActionHints {
  /** A single page-level acknowledgement request for all signals on the page. */
  ack: AgentSignalActionHint;
  /** Targeted replies are emitted only for explicit signal-id reads. */
  reply?: AgentSignalActionHint[];
}

export type AgentSignalResult =
  | { action: 'publish' | 'reply'; signal_id: string; signal_ids: string[]; thread_id: string; workspace_path: string; artifact: string | null }
  | ({ action: 'list'; count: number; signals: AgentSignalRecord[]; unread_only: boolean; actions?: AgentSignalActionHints } & Pick<GetNotificationsResult, 'partial' | 'partialReasons' | 'next'>)
  | { action: 'resolve'; resolved: number; signal_ids: string[] }
  | { action: 'ack'; acknowledged: number; signal_ids: string[] };
