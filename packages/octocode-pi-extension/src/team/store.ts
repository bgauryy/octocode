import path from 'node:path';
import { openAgentDb, sharedAgentDb } from '../agentdb/db.js';
import { agentDbPath, canonicalPath, repoKey } from '../shared/home.js';
import { sameProcess } from '../shared/process.js';
import { num, type Row } from '../agentdb/rows.js';
import { ancestorKeys, leaseKey, overlappingRequests } from './lease-key.js';
import type { Ask } from './asks.js';
import { toAsk, toLease, toMessage, toTraffic, type DeadLetter, type Lease, type LeaseRequest, type Message, type SendOptions, type Traffic, WAKE } from './model.js';
import { LEASE_MS, TeamRoster } from './roster.js';

/** Makes the `proof` of message `id` for `recipient` (see `userProof`); undefined for a recipient the caller cannot sign for. */
type SignProof = (id: number, recipient: string) => string | undefined;

/**
 * The team tables of the agent database: who works in this repository, what they say to each other, and which files
 * they have reserved. One SQLite file shared by every Pi process on the machine (WAL, so readers never block),
 * rows scoped by workspace = git root. Entities, after the octocode-agents-communication skill:
 *   agents      one live agent; its owner rewrites the row as it works (status, tool, tokens, heartbeat)
 *   messages    immutable; may require a reply and may answer another message (replyTo)
 *   deliveries  one row per recipient: delivered when the model saw it, completed when answered, dead when the recipient left unread
 *               (the sender is told once: notified_at); `wake` says whether it may start a turn of an idle recipient
 *               (1), or also stop a busy one's current step (2, an interrupt)
 *   leases      cooperative file/tree reservations that expire unless the owner keeps renewing them
 */

export const AGENT_ID_ENV = 'OCTOCODE_AGENT_ID';
export const PARENT_ID_ENV = 'OCTOCODE_PARENT_ID';
/** Set in a subagent: its parent Pi's pid. The child leaves and shuts down when that process is gone (a crashed parent). */
export const PARENT_PID_ENV = 'OCTOCODE_PARENT_PID';
/** Set in a subagent: the role it was started with (its profile), shown next to its name. */
export const AGENT_ROLE_ENV = 'OCTOCODE_AGENT_ROLE';
/** Set in a subagent: its goal, at most five words (`identity.ts`). Stored in the `task` column. */
export const AGENT_GOAL_ENV = 'OCTOCODE_AGENT_GOAL';
/** Set in a subagent started with `collaborate`: it works with its sibling subagents, not only for its parent. */
export const AGENT_COLLABORATE_ENV = 'OCTOCODE_AGENT_COLLABORATE';
/** A subagent's own handoff folder (`<workspace>/.octocode/tmp/agents/<id>`): long results go there as files, not into its report. */
export const AGENT_SCRATCH_ENV = 'OCTOCODE_AGENT_SCRATCH';
/** Absolute path used as-is as the team workspace (e.g. a subagent in a git worktree joins its parent's team). */
export const TEAM_WORKSPACE_ENV = 'OCTOCODE_TEAM_WORKSPACE';

export const INBOX_BATCH_SIZE = 32;

/**
 * The team workspace for `cwd`: one team per repository (`repoKey(cwd).root`, symlinks resolved), unless
 * `TEAM_WORKSPACE_ENV` names it (a subagent in an isolated worktree joins its parent's team).
 */
export function teamWorkspace(cwd: string, env: NodeJS.ProcessEnv = process.env): string {
  const override = env[TEAM_WORKSPACE_ENV];
  if (override && path.isAbsolute(override)) return canonicalPath(override);
  return repoKey(cwd).root;
}

/**
 * Canonical comparison key of `file` below `workspace`: its components folded as `lease-key.ts` says (the rules the
 * agents-communication package shares), so `Src/A.ts` and `src/a.ts` collide on every platform.
 */
export function pathKey(workspace: string, file: string): string {
  const relative = path.relative(workspace, canonicalPath(path.resolve(workspace, file)));
  return leaseKey(relative.split(path.sep));
}

/** The team tables (above): the `agents` roster and the connection come from `TeamRoster` (`roster.ts`); this adds messages and leases. */
export class TeamStore extends TeamRoster {
  /** Opens its own connection to the agent database (`agentDbPath`), which checks the file is ours and adds any missing schema first. */
  static open(cwd: string, file = agentDbPath(), env: NodeJS.ProcessEnv = process.env): TeamStore {
    const agent = openAgentDb(file, env);
    try {
      const store = new TeamStore(agent, teamWorkspace(cwd, env), true);
      store.housekeep();
      return store;
    } catch (error) {
      agent.close();
      throw error;
    }
  }

  /** The team over the process's one agent database connection (`sharedAgentDb`), which sessions, memory and backlog use too. */
  static shared(cwd: string, env: NodeJS.ProcessEnv = process.env): TeamStore {
    const store = new TeamStore(sharedAgentDb(env), teamWorkspace(cwd, env), false);
    store.housekeep();
    return store;
  }

  // ── messages ──────────────────────────────────────────────────────────────

  /**
   * Store one message for every recipient that can still read it (a stalled, stale one keeps its mail). A reply
   * completes the delivery it answers. Recipients that left or whose process ended are refused and returned in
   * `departed`; with none left, nothing is stored and `id` is undefined.
   * `wake` (a question, or a sender outside the team) lets it start an idle recipient's turn; otherwise only the
   * answer to a question that recipient asked, and still awaited, wakes it. `sign` makes the recipient's `proof` once the
   * message has its id (see `userProof`); it returns undefined for a recipient the caller cannot sign for.
   */
  send(
    from: string, to: string[], body: string, options: SendOptions & { replyRequired: boolean; wake?: boolean; sign?: SignProof },
    now = Date.now(),
  ): { id: number | undefined; departed: string[] } {
    return this.transaction(() => {
      // Read in the same IMMEDIATE transaction as the insert: no recipient can leave in between.
      const live = this.reachable();
      const departed = to.filter((recipient) => !live.has(recipient));
      const recipients = to.filter((recipient) => live.has(recipient));
      if (recipients.length === 0) return { id: undefined, departed };
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
      const insert = this.db.prepare('INSERT INTO deliveries (message, recipient, wake, proof) VALUES (?, ?, ?, ?)');
      for (const recipient of recipients) {
        const wake = options.interrupt ? WAKE.interrupt : options.wake || options.replyRequired || recipient === asker ? WAKE.wake : WAKE.none;
        insert.run(id, recipient, wake, options.sign?.(id, recipient) ?? null);
      }
      if (options.replyTo !== undefined) this.db.prepare('UPDATE deliveries SET completed_at = ? WHERE message = ? AND recipient = ? AND completed_at IS NULL').run(now, options.replyTo, from);
      return { id, departed };
    });
  }

  /** The sender of message `id` in this workspace and whether it asked for a reply; undefined when there is no such message. */
  replyTarget(id: number): { from: string; replyRequired: boolean } | undefined {
    const row = this.db.prepare('SELECT sender, reply_required FROM messages WHERE id = ? AND workspace = ?').get(id, this.workspace) as Row | undefined;
    return row ? { from: String(row['sender']), replyRequired: row['reply_required'] === 1 } : undefined;
  }

  /**
   * The questions `child` sent `parent` (messages with `replyRequired`) since `since`, oldest first, each with the
   * parent's first reply to it (a message with `replyTo` that id). Read from the existing tables: nothing is stored for it.
   */
  asks(child: string, parent: string, since: number): Ask[] {
    const rows = this.db
      .prepare(
        `SELECT m.id, m.body, m.created_at, r.id AS reply_id, r.created_at AS reply_at, r.sender AS reply_by
         FROM messages m JOIN deliveries d ON d.message = m.id AND d.recipient = ?
         LEFT JOIN messages r ON r.id = (SELECT min(id) FROM messages WHERE reply_to = m.id AND sender = d.recipient)
         WHERE m.workspace = ? AND m.sender = ? AND m.reply_required = 1 AND m.created_at >= ? ORDER BY m.id`,
      )
      .all(parent, this.workspace, child, since) as Row[];
    return rows.map(toAsk);
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

  /** How many messages newer than `after` wait undelivered for `id` (the inbox index alone): the inbox reads them only when this is non-zero. */
  waiting(id: string, after = 0): number {
    return num((this.db.prepare('SELECT COUNT(*) AS n FROM deliveries WHERE recipient = ? AND message > ? AND delivered_at IS NULL AND dead_at IS NULL').get(id, after) as Row)['n']);
  }

  /** The ids of the messages up to `through` that still wait undelivered for `id`: the inbox handed those over, or its mark ran ahead of this database. */
  waitingThrough(id: string, through: number): number[] {
    const rows = this.db.prepare('SELECT message FROM deliveries WHERE recipient = ? AND message <= ? AND delivered_at IS NULL AND dead_at IS NULL').all(id, through) as Row[];
    return rows.map((row) => num(row['message']));
  }

  /** One bounded batch of messages newer than `after` waiting for `id`, oldest first. `ack` marks the ones the model saw. */
  pending(id: string, after = 0): Message[] {
    const rows = this.db
      .prepare(
        `SELECT m.id, m.sender, m.body, m.created_at, m.reply_required, m.reply_to, d.wake, d.proof, s.role AS sender_role, s.task AS sender_goal
         FROM deliveries d JOIN messages m ON m.id = d.message LEFT JOIN agents s ON s.id = m.sender
         WHERE d.recipient = ? AND d.message > ? AND d.delivered_at IS NULL AND d.dead_at IS NULL ORDER BY m.id LIMIT ?`,
      )
      .all(id, after, INBOX_BATCH_SIZE) as Row[];
    return rows.map((row) => toMessage(row, id));
  }

  /** Mark messages delivered to `id` (FYI messages are complete on delivery). One reader per recipient, so no lease. */
  ack(id: string, messageIds: number[], now = Date.now()): void {
    if (messageIds.length === 0) return;
    this.transaction(() => {
      const mark = this.db.prepare(
        'UPDATE deliveries SET delivered_at = ?, completed_at = CASE WHEN (SELECT reply_required FROM messages WHERE id = ?) = 1 THEN completed_at ELSE COALESCE(completed_at, ?) END WHERE message = ? AND recipient = ? AND delivered_at IS NULL',
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
    return rows.map((row) => toTraffic(row, deliveries.all(num(row['id'])) as Row[]));
  }

  // ── leases ────────────────────────────────────────────────────────────────

  /** A live lease held by someone other than `self` that covers `file` (or, for a tree request, lies under it). */
  conflict(file: string, self: string, kind: 'file' | 'tree' = 'file', now = Date.now()): Lease | undefined {
    const key = pathKey(this.workspace, file);
    const ancestors = ancestorKeys(key);
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
      // A crashed owner (killed, never left) frees its files now rather than when the lease runs out; a stalled one
      // (alive, silent) keeps them until they lapse, since its process may resume.
      if (!this.retireOwner(lease.owner, now)) return lease;
    }
  }

  /** Retires a lease owner that is gone (it has no row, or its process died or was replaced); whether it was. */
  private retireOwner(owner: string, now: number): boolean {
    const judged = this.processOf(owner);
    if (judged && sameProcess(judged.pid, judged.started, now)) return false;
    return this.reap(owner, judged, now);
  }

  /**
   * Reserve every request or none: a conflict with another live lease rejects the whole set. The requests must not
   * overlap each other (a tree and a path under it, two spellings of one path, a path twice): that is an error, as in
   * the agents-communication package, because the caller's intent is then unclear (ask for the tree alone).
   */
  lock(owner: string, requests: LeaseRequest[], now = Date.now()): { ok: true; leases: Lease[] } | { ok: false; conflicts: Array<{ path: string; heldBy: Lease }> } {
    if (requests.length === 0) throw new Error('Give at least one path.');
    const overlap = overlappingRequests(requests.map((request) => ({ ...request, key: pathKey(this.workspace, request.path) })));
    if (overlap) {
      const shown = (request: LeaseRequest) => path.relative(this.workspace, canonicalPath(path.resolve(this.workspace, request.path))) || '.';
      throw new Error(`Requested paths overlap: ${shown(overlap[0])} and ${shown(overlap[1])}. Nothing was locked; reserve the tree alone, or each path once.`);
    }
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
        const relative = path.relative(this.workspace, canonicalPath(path.resolve(this.workspace, request.path))) || '.';
        const key = pathKey(this.workspace, request.path);
        const existing = find.get(this.workspace, owner, key, request.kind, now) as Row | undefined;
        if (existing) touch.run(request.reason, now + LEASE_MS, num(existing['id']));
        else insert.run(this.workspace, relative, key, request.kind, owner, request.reason, now, now + LEASE_MS);
      }
      return { ok: true as const, leases: this.leases(now).filter((lease) => lease.owner === owner) };
    });
  }

  /**
   * Keep the owner's reservations alive; called with every heartbeat. Leases that had already lapsed (a peer may have
   * taken the path since) are released, not revived: the owner learns of them from `owned`, which no longer lists
   * them, and must lock them again.
   */
  renew(owner: string, now = Date.now()): void {
    this.transaction(() => {
      this.db.prepare('DELETE FROM leases WHERE owner = ? AND expires_at <= ?').run(owner, now);
      this.db.prepare('UPDATE leases SET expires_at = ? WHERE owner = ?').run(now + LEASE_MS, owner);
    });
    this.prune(now);
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
