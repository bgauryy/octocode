import path from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { openAgentDb, type AgentDb } from '../agentdb/db.js';
import { agentDbPath, findRepoRoot } from '../shared/home.js';
import { processAlive } from '../shared/process.js';
import { EditLog } from './edits.js';
import { num, toLease, toMember, type DeadLetter, type Lease, type LeaseRequest, type Member, type Message, type Row, type Traffic } from './model.js';

/**
 * The team tables of the agent database: who works in this repository, what they say to each other, and which files
 * they have reserved. One SQLite file shared by every Pi process on the machine (WAL, so readers never block),
 * rows scoped by workspace = git root. Entities, after the octocode-agents-communication skill:
 *   agents      one live agent; its owner rewrites the row as it works (status, tool, tokens, heartbeat)
 *   messages    immutable; may require a reply and may answer another message (replyTo)
 *   deliveries  one row per recipient: delivered when injected, completed when answered, dead when the recipient left unread
 *               (the sender is told once: notified_at); `wake` says whether it may start a turn of an idle recipient
 *   leases      cooperative file/tree reservations that expire unless the owner keeps renewing them
 */

export const AGENT_ID_ENV = 'OCTOCODE_AGENT_ID';
export const PARENT_ID_ENV = 'OCTOCODE_PARENT_ID';
export const AGENT_TASK_ENV = 'OCTOCODE_AGENT_TASK';
/** Set in a subagent started with `collaborate`: it works with its sibling subagents, not only for its parent. */
export const AGENT_COLLABORATE_ENV = 'OCTOCODE_AGENT_COLLABORATE';
/** A subagent's own handoff folder (`<workspace>/.octocode/tmp/agents/<id>`): long results go there as files, not into its report. */
export const AGENT_SCRATCH_ENV = 'OCTOCODE_AGENT_SCRATCH';
/** Absolute path used as-is as the team workspace (e.g. a subagent in a git worktree joins its parent's team). */
export const TEAM_WORKSPACE_ENV = 'OCTOCODE_TEAM_WORKSPACE';

/** An agent that has not refreshed its row for this long (or whose process is gone) is dropped. */
const STALE_MS = 60_000;
/** Leases live this long past the owner's last heartbeat, so a crashed owner frees its files quickly. */
export const LEASE_MS = 90_000;
const MESSAGE_RETENTION_MS = 24 * 3_600_000;
export const INBOX_BATCH_SIZE = 32;
const PRUNE_INTERVAL_MS = 60_000;
const MAX_LEASES_PER_CALL = 32;

/** One team per repository: the git root when there is one, else the working directory. */
function repoRoot(cwd: string, env: NodeJS.ProcessEnv = process.env): string {
  const override = env[TEAM_WORKSPACE_ENV];
  if (override && path.isAbsolute(override)) return path.resolve(override);
  return findRepoRoot(cwd);
}

/**
 * Canonical comparison key: each component NFD-normalised and case-folded, so `Src/A.ts` and `src/a.ts`
 * collide on every platform (a deliberately conservative choice, as in the skill's lock contract).
 */
export function pathKey(workspace: string, file: string): string {
  const relative = path.relative(workspace, path.resolve(workspace, file));
  const parts = relative.split(path.sep).filter((part) => part && part !== '.');
  return `/${parts.map((part) => part.normalize('NFD').toLowerCase()).join('/')}`.replace(/\/$/, '');
}

/** Busy wait for a background write (heartbeat, delivery): a locked database is retried on the next tick instead. */
const QUICK_BUSY_MS = 100;
/** Busy wait for a write an agent asked for (a lock, a message). */
const BUSY_MS = 5_000;

export class TeamStore {
  private readonly db: DatabaseSync;
  /** Who changed which file recently, in the same database. */
  readonly edits: EditLog;
  private prunedAt = -Infinity;

  private constructor(
    private readonly agent: AgentDb,
    readonly workspace: string,
  ) {
    this.db = agent.db;
    this.edits = new EditLog(agent.db, workspace);
  }

  /** Opens its own connection to the agent database (`agentDbPath`), which checks and migrates the schema first. */
  static open(cwd: string, file = agentDbPath(), env: NodeJS.ProcessEnv = process.env): TeamStore {
    const agent = openAgentDb(file, env);
    try {
      const store = new TeamStore(agent, repoRoot(cwd, env));
      store.prune();
      return store;
    } catch (error) {
      agent.close();
      throw error;
    }
  }

  close(): void {
    this.agent.close();
  }

  private transaction<T>(work: () => T): T {
    return this.agent.transaction(work);
  }

  /** Runs background work with a short busy wait, so a database held by another process never freezes this one. */
  quick<T>(work: () => T): T {
    this.db.exec(`PRAGMA busy_timeout=${QUICK_BUSY_MS}`);
    try {
      return work();
    } finally {
      this.db.exec(`PRAGMA busy_timeout=${BUSY_MS}`);
    }
  }

  private prune(now = Date.now()): void {
    if (now - this.prunedAt < PRUNE_INTERVAL_MS) return;
    this.db.prepare('DELETE FROM messages WHERE created_at < ?').run(now - MESSAGE_RETENTION_MS);
    this.db.prepare('DELETE FROM leases WHERE expires_at < ?').run(now);
    this.prunedAt = now;
  }

  // ── agents ────────────────────────────────────────────────────────────────

  save(member: Member): void {
    this.db
      .prepare(
        `INSERT INTO agents (id, workspace, role, parent_id, pid, status, task, activity, model, joined_at, seen_at, tool_calls, input, output, cost)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET status=excluded.status, task=excluded.task, activity=excluded.activity, model=excluded.model, seen_at=excluded.seen_at,
           tool_calls=excluded.tool_calls, input=excluded.input, output=excluded.output, cost=excluded.cost`,
      )
      .run(member.id, this.workspace, member.role, member.parentId ?? null, member.pid, member.status, member.task ?? null, member.activity ?? null, member.model ?? null,
        member.joinedAt, member.updatedAt, member.toolCalls, member.input, member.output, member.cost);
  }

  /** Leave: the agent and its reservations go; the messages it never read become dead letters the sender can see. */
  remove(id: string, now = Date.now()): void {
    this.transaction(() => {
      this.db.prepare('DELETE FROM agents WHERE id = ?').run(id);
      this.db.prepare('DELETE FROM leases WHERE owner = ?').run(id);
      this.db.prepare('UPDATE deliveries SET dead_at = ? WHERE recipient = ? AND delivered_at IS NULL AND dead_at IS NULL').run(now, id);
    });
  }

  /** Live agents of this workspace, oldest first, with unanswered-message counts and held paths. Dead ones are removed. */
  list(now = Date.now()): Member[] {
    const rows = this.db
      .prepare(
        `SELECT a.*, (SELECT count(*) FROM deliveries d JOIN messages m ON m.id = d.message
                      WHERE d.recipient = a.id AND d.delivered_at IS NOT NULL AND d.completed_at IS NULL AND m.reply_required = 1) AS pending
         FROM agents a WHERE a.workspace = ? ORDER BY a.joined_at, a.id`,
      )
      .all(this.workspace) as Row[];
    const live: Member[] = [];
    for (const row of rows) {
      const member = toMember(row);
      if (now - member.updatedAt > STALE_MS || !processAlive(member.pid)) this.remove(member.id);
      else live.push(member);
    }
    const held = this.leases(now);
    return live.map((member) => {
      const locks = held.filter((lease) => lease.owner === member.id).map((lease) => (lease.kind === 'tree' ? `${lease.path}/` : lease.path));
      return locks.length > 0 ? { ...member, locks } : member;
    });
  }

  // ── messages ──────────────────────────────────────────────────────────────

  /**
   * Store one message for every live recipient. A reply completes the delivery it answers. Recipients that are no
   * longer live are refused and returned in `departed`; with none left, nothing is stored and `id` is undefined.
   * `wake` (a question, or a sender outside the team) lets it start an idle recipient's turn; otherwise only the
   * answer to a question that recipient asked, and still awaited, wakes it.
   */
  send(from: string, to: string[], body: string, options: { replyRequired: boolean; replyTo?: number; wake?: boolean }, now = Date.now()): { id: number | undefined; departed: string[] } {
    const live = new Set(this.list(now).map((member) => member.id));
    const departed = to.filter((recipient) => !live.has(recipient));
    const recipients = to.filter((recipient) => live.has(recipient));
    if (recipients.length === 0) return { id: undefined, departed };
    const id = this.transaction(() => {
      // The asker is woken by the answer to its own open question, never by a reply to an FYI or a thank-you.
      let asker: string | undefined;
      if (options.replyTo !== undefined) {
        const question = this.replyTarget(options.replyTo);
        const open = this.db.prepare('SELECT 1 FROM deliveries WHERE message = ? AND recipient = ? AND completed_at IS NULL').get(options.replyTo, from);
        if (question?.replyRequired && open !== undefined) asker = question.from;
      }
      const { lastInsertRowid } = this.db
        .prepare('INSERT INTO messages (workspace, sender, body, reply_required, reply_to, created_at) VALUES (?, ?, ?, ?, ?, ?)')
        .run(this.workspace, from, body, options.replyRequired ? 1 : 0, options.replyTo ?? null, now);
      const id = Number(lastInsertRowid);
      const insert = this.db.prepare('INSERT INTO deliveries (message, recipient, wake) VALUES (?, ?, ?)');
      for (const recipient of recipients) insert.run(id, recipient, options.wake || options.replyRequired || recipient === asker ? 1 : 0);
      if (options.replyTo !== undefined) this.db.prepare('UPDATE deliveries SET completed_at = ? WHERE message = ? AND recipient = ? AND completed_at IS NULL').run(now, options.replyTo, from);
      return id;
    });
    return { id, departed };
  }

  /** The sender of message `id` in this workspace and whether it asked for a reply; undefined when there is no such message. */
  replyTarget(id: number): { from: string; replyRequired: boolean } | undefined {
    const row = this.db.prepare('SELECT sender, reply_required FROM messages WHERE id = ? AND workspace = ?').get(id, this.workspace) as Row | undefined;
    return row ? { from: String(row['sender']), replyRequired: row['reply_required'] === 1 } : undefined;
  }

  /** Messages `sender` sent whose recipients left before reading them, not yet reported to it (oldest first). */
  deadLetters(sender: string, limit = INBOX_BATCH_SIZE): DeadLetter[] {
    const rows = this.db
      .prepare(
        `SELECT m.id, m.body, d.recipient FROM deliveries d JOIN messages m ON m.id = d.message
         WHERE m.workspace = ? AND m.sender = ? AND d.dead_at IS NOT NULL AND d.notified_at IS NULL ORDER BY m.id, d.recipient LIMIT ?`,
      )
      .all(this.workspace, sender, limit) as Row[];
    const letters = new Map<number, DeadLetter>();
    for (const row of rows) {
      const id = num(row['id']);
      const letter = letters.get(id) ?? { id, recipients: [], text: String(row['body']) };
      letter.recipients.push(String(row['recipient']));
      letters.set(id, letter);
    }
    return [...letters.values()];
  }

  /** Records that the sender was told these dead letters went unread. */
  markNotified(letters: DeadLetter[], now = Date.now()): void {
    if (letters.length === 0) return;
    this.transaction(() => {
      const mark = this.db.prepare('UPDATE deliveries SET notified_at = ? WHERE message = ? AND recipient = ? AND dead_at IS NOT NULL');
      for (const letter of letters) for (const recipient of letter.recipients) mark.run(now, letter.id, recipient);
    });
  }

  /** One bounded batch waiting for `id`, oldest first. `ack` marks only messages actually handed over. */
  pending(id: string): Message[] {
    const rows = this.db
      .prepare(
        `SELECT m.id, m.sender, m.body, m.created_at, m.reply_required, m.reply_to, d.wake FROM deliveries d JOIN messages m ON m.id = d.message
         WHERE d.recipient = ? AND d.delivered_at IS NULL AND d.dead_at IS NULL ORDER BY m.id LIMIT ?`,
      )
      .all(id, INBOX_BATCH_SIZE) as Row[];
    return rows.map((row) => {
      const replyTo = row['reply_to'];
      const wake = row['wake'];
      return {
        id: num(row['id']), from: String(row['sender']), to: id, text: String(row['body']), at: num(row['created_at']), replyRequired: row['reply_required'] === 1,
        ...(typeof replyTo === 'number' ? { replyTo } : {}),
        ...(typeof wake === 'number' ? { wake: wake === 1 } : {}),
      };
    });
  }

  /** Mark messages delivered to `id` (FYI messages are complete on delivery). One reader per recipient, so no lease. */
  ack(id: string, messageIds: number[], now = Date.now()): void {
    if (messageIds.length === 0) return;
    this.transaction(() => {
      const mark = this.db.prepare(
        'UPDATE deliveries SET delivered_at = ?, completed_at = CASE WHEN (SELECT reply_required FROM messages WHERE id = ?) = 1 THEN NULL ELSE ? END WHERE message = ? AND recipient = ? AND delivered_at IS NULL',
      );
      for (const message of messageIds) mark.run(now, message, now, message, id);
    });
  }

  /**
   * The latest messages in this workspace, newest first, with their delivery state. The workspace is shared by every
   * session in the repository, so the default fetches enough rows for a panel to filter its own and still show a few.
   */
  recent(limit = 30, now = Date.now(), windowMs = 10 * 60_000): Traffic[] {
    const rows = this.db.prepare('SELECT * FROM messages WHERE workspace = ? AND created_at > ? ORDER BY id DESC LIMIT ?').all(this.workspace, now - windowMs, limit) as Row[];
    const deliveries = this.db.prepare('SELECT recipient, delivered_at, completed_at, dead_at FROM deliveries WHERE message = ? ORDER BY recipient');
    return rows.map((row) => {
      const sent = deliveries.all(num(row['id'])) as Row[];
      const replyRequired = row['reply_required'] === 1;
      const state: Traffic['state'] =
        sent.some((d) => d['delivered_at'] === null && d['dead_at'] === null)
          ? 'queued'
          : sent.length === 0 || sent.some((d) => d['dead_at'] !== null)
            ? 'dead-lettered'
            : replyRequired
              ? sent.every((d) => d['completed_at'] !== null)
                ? 'answered'
                : 'awaiting reply'
              : 'delivered';
      const replyTo = row['reply_to'];
      return { id: num(row['id']), from: String(row['sender']), to: sent.map((d) => String(d['recipient'])), text: String(row['body']), at: num(row['created_at']), ...(typeof replyTo === 'number' ? { replyTo } : {}), state };
    });
  }

  // ── leases ────────────────────────────────────────────────────────────────

  leases(now = Date.now()): Lease[] {
    return (this.db.prepare('SELECT * FROM leases WHERE workspace = ? AND expires_at > ? ORDER BY id').all(this.workspace, now) as Row[]).map(toLease);
  }

  /** A live lease held by someone other than `self` that covers `file` (or, for a tree request, lies under it). */
  conflict(file: string, self: string, kind: 'file' | 'tree' = 'file', now = Date.now()): Lease | undefined {
    const key = pathKey(this.workspace, file);
    const ancestors: string[] = [];
    for (let at = key.lastIndexOf('/'); at > 0; at = key.lastIndexOf('/', at - 1)) ancestors.push(key.slice(0, at));
    ancestors.push('');
    const marks = ancestors.map(() => '?').join(',');
    // Same key, a tree above it, or (for a tree request) anything below it. Component boundaries, not string prefixes.
    const query = this.db.prepare(
      `SELECT * FROM leases WHERE workspace = ? AND owner <> ? AND expires_at > ? AND
         (path_key = ? OR (kind = 'tree' AND path_key IN (${marks})) OR (? = 'tree' AND path_key > ? AND path_key < ?)) ORDER BY id LIMIT 1`,
    );
    for (;;) {
      const row = query.get(this.workspace, self, now, key, ...ancestors, kind, `${key}/`, `${key}0`) as Row | undefined;
      if (!row) return undefined;
      const lease = toLease(row);
      // A crashed owner (killed, never left) frees its files now rather than when the lease runs out.
      if (!this.ownerGone(lease.owner, now)) return lease;
      this.remove(lease.owner, now);
    }
  }

  /** Whether a lease owner is gone: it has no row (it left), its process died, or it stopped heartbeating. */
  private ownerGone(owner: string, now: number): boolean {
    const row = this.db.prepare('SELECT pid, seen_at FROM agents WHERE id = ?').get(owner) as Row | undefined;
    return row === undefined || now - num(row['seen_at']) > STALE_MS || !processAlive(num(row['pid']));
  }

  /** Reserve every request or none: a conflict with another live lease rejects the whole set. */
  lock(owner: string, requests: LeaseRequest[], now = Date.now()): { ok: true; leases: Lease[] } | { ok: false; conflicts: Array<{ path: string; heldBy: Lease }> } {
    if (requests.length === 0 || requests.length > MAX_LEASES_PER_CALL) throw new Error(`Give 1–${MAX_LEASES_PER_CALL} paths.`);
    return this.transaction(() => {
      this.prune(now);
      const conflicts = requests.flatMap((request) => {
        const heldBy = this.conflict(request.path, owner, request.kind, now);
        return heldBy ? [{ path: request.path, heldBy }] : [];
      });
      if (conflicts.length > 0) return { ok: false as const, conflicts };
      const find = this.db.prepare('SELECT id FROM leases WHERE workspace = ? AND owner = ? AND path_key = ? AND kind = ? AND expires_at > ?');
      const insert = this.db.prepare('INSERT INTO leases (workspace, path, path_key, kind, owner, reason, acquired_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)');
      const touch = this.db.prepare('UPDATE leases SET reason = ?, expires_at = ? WHERE id = ?');
      for (const request of requests) {
        const relative = path.relative(this.workspace, path.resolve(this.workspace, request.path)) || '.';
        const key = pathKey(this.workspace, request.path);
        const existing = find.get(this.workspace, owner, key, request.kind, now) as Row | undefined;
        if (existing) touch.run(request.reason, now + LEASE_MS, num(existing['id']));
        else insert.run(this.workspace, relative, key, request.kind, owner, request.reason, now, now + LEASE_MS);
      }
      return { ok: true as const, leases: this.leases(now).filter((lease) => lease.owner === owner) };
    });
  }

  /**
   * Keep the owner's reservations alive; called with every heartbeat. Returns the leases that had already lapsed
   * (a peer may have taken the path since): they are released, and the owner must lock them again.
   */
  renew(owner: string, now = Date.now()): Lease[] {
    const lapsed = this.transaction(() => {
      const lapsed = (this.db.prepare('SELECT * FROM leases WHERE owner = ? AND expires_at <= ? ORDER BY id').all(owner, now) as Row[]).map(toLease);
      this.db.prepare('DELETE FROM leases WHERE owner = ? AND expires_at <= ?').run(owner, now);
      this.db.prepare('UPDATE leases SET expires_at = ? WHERE owner = ?').run(now + LEASE_MS, owner);
      return lapsed;
    });
    this.prune(now);
    return lapsed;
  }

  /** `kind:key` of every live lease the owner holds in this workspace. */
  owned(owner: string, now = Date.now()): Set<string> {
    return new Set(this.leases(now).filter((lease) => lease.owner === owner).map((lease) => `${lease.kind}:${pathKey(this.workspace, lease.path)}`));
  }

  /** Release the owner's reservations for the given paths, or all of them; returns how many were released. */
  unlock(owner: string, paths?: string[]): number {
    if (!paths || paths.length === 0) return Number(this.db.prepare('DELETE FROM leases WHERE owner = ?').run(owner).changes);
    const remove = this.db.prepare('DELETE FROM leases WHERE workspace = ? AND owner = ? AND path_key = ?');
    return paths.reduce((count, file) => count + Number(remove.run(this.workspace, owner, pathKey(this.workspace, file)).changes), 0);
  }
}
