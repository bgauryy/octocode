/** The team's records as the rest of the extension sees them, and how they are read from database rows. */

export interface Member {
  /** Unique, and the address used by sendMessage (e.g. `researcher-3fa9`, `main-91c0`). */
  id: string;
  /** `main` for a session, the profile name (or `general`) for a subagent. */
  role: string;
  parentId?: string;
  pid: number;
  status: 'idle' | 'working';
  /** What the agent was asked to do, or said it is doing (one line). */
  task?: string;
  /** The tool it is running right now. */
  activity?: string;
  /** The model it runs (`claude-opus-4-5`), as Pi names it; absent until known. */
  model?: string;
  joinedAt: number;
  updatedAt: number;
  toolCalls: number;
  input: number;
  output: number;
  cost: number;
  /** Set by `list`: messages it received that still need an answer. */
  pending?: number;
  /** Set by `list`: paths it has reserved (trees end with `/`). */
  locks?: string[];
}

export interface Message {
  id: number;
  from: string;
  to: string;
  text: string;
  at: number;
  replyRequired: boolean;
  replyTo?: number;
  /** Whether it may start a turn of an idle recipient; decided when sent (absent for rows stored before v4). */
  wake?: boolean;
}

/** A message whose recipients left before reading it, for its sender. */
export interface DeadLetter {
  id: number;
  recipients: string[];
  text: string;
}

/** One message as the user sees it in the feed: who talked to whom, and how far it got. */
export interface Traffic {
  id: number;
  from: string;
  to: string[];
  text: string;
  at: number;
  replyTo?: number;
  state: 'queued' | 'delivered' | 'awaiting reply' | 'answered' | 'dead-lettered';
}

export interface Lease {
  id: number;
  path: string;
  kind: 'file' | 'tree';
  owner: string;
  reason: string;
  acquiredAt: number;
  expiresAt: number;
}

export type LeaseRequest = Pick<Lease, 'path' | 'kind' | 'reason'>;

export type Row = Record<string, unknown>;
export const num = (value: unknown) => (typeof value === 'number' ? value : 0);
export const text = (value: unknown) => (typeof value === 'string' ? value : undefined);

export function toMember(row: Row): Member {
  const task = text(row['task']);
  const activity = text(row['activity']);
  const parentId = text(row['parent_id']);
  const model = text(row['model']);
  return {
    id: String(row['id']),
    role: String(row['role']),
    ...(parentId ? { parentId } : {}),
    pid: num(row['pid']),
    status: row['status'] === 'working' ? 'working' : 'idle',
    ...(task ? { task } : {}),
    ...(activity ? { activity } : {}),
    ...(model ? { model } : {}),
    joinedAt: num(row['joined_at']),
    updatedAt: num(row['seen_at']),
    toolCalls: num(row['tool_calls']),
    input: num(row['input']),
    output: num(row['output']),
    cost: num(row['cost']),
    pending: num(row['pending']),
  };
}

export function toLease(row: Row): Lease {
  return { id: num(row['id']), path: String(row['path']), kind: row['kind'] === 'tree' ? 'tree' : 'file', owner: String(row['owner']), reason: String(row['reason']), acquiredAt: num(row['acquired_at']), expiresAt: num(row['expires_at']) };
}

