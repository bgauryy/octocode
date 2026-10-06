import fs from 'node:fs';
import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { errorMessage } from '../shared/util.js';
import { sanitizeTerminalText, storedText } from '../shared/sanitize.js';
import { dbFailure, leaseAge, MESSAGE_MAX_CHARS, messageText, newId, outside, resolveLockPath, resolveTarget, USER_SENDER, wakes } from './routing.js';
import {
  AGENT_ID_ENV,
  AGENT_TASK_ENV,
  PARENT_ID_ENV,
  pathKey,
  TeamStore,
} from './store.js';
import type { DeadLetter, Lease, LeaseRequest, Member, Message } from './model.js';
import { HeldLeases, leaseIdleMs } from './held.js';
import { Inbox } from './inbox.js';
import type { EditLog } from './edits.js';
import { AgentDbError } from '../agentdb/db.js';
import { agentDbPath } from '../shared/home.js';
import { dbStamp, TeamTicker, type TeamView } from './ticker.js';

export type { TeamView } from './ticker.js';

export const MESSAGE_TYPE = 'octocode-agent-message';
export { EXTERNAL_SENDER, MESSAGE_MAX_CHARS, USER_SENDER } from './routing.js';
/** Refresh the agent's row at least this often so peers can tell it is alive (and its leases stay valid). */
const HEARTBEAT_MS = 5_000;
/** Consecutive failed ticks (about 30s) before the agent hears that the team database is unavailable. */
const FAILED_TICKS_NOTICE = 30;
const ROLE_SUFFIX = /-[0-9a-f]{6}$/;
/** Longest one-line task or activity stored for an agent (the panel and `coordinate list` clip further). */
const LINE_MAX = 200;

/** This process's place in the team: joins, reports what it is doing, holds reservations and delivers messages sent to it. */
export class Team {
  private me: Member | undefined;
  private db: TeamStore | undefined;
  private ctx: ExtensionContext | undefined;
  /** The one ticker: delivery and heartbeats while joined, a cached view for watchers (the panel). */
  private readonly ticker = new TeamTicker({
    joined: () => this.me !== undefined,
    beat: () => this.deliverPending(),
    dbFile: () => {
      const file = agentDbPath(this.env);
      return this.ctx && (this.db || fs.existsSync(file)) ? file : undefined;
    },
    // Background reads may prune departed agents (a write): never wait long for another process's lock.
    read: () => (this.db ? this.db.quick(() => this.snapshot()) : this.snapshot()),
    maxAgeMs: HEARTBEAT_MS,
  });
  private dirty = false;
  private savedAt = 0;
  /** Why the database cannot be used at all (foreign or newer schema); shown in /agents and the footer. */
  private schemaProblem: string | undefined;
  private failedTicks = 0;
  /** When this agent last worked (or joined): an idle owner's reservations lapse `leaseIdleMs` after it. */
  private activeAt = 0;
  private readonly held = new HeldLeases();
  /** Ordered, paced delivery with acknowledgement retries. */
  private readonly inbox = new Inbox(HEARTBEAT_MS);
  /** Subagents this session started that have finished: id → where their report went, for a later message to them. */
  private readonly finished = new Map<string, string>();

  /** Recent completion receipts; active-agent limits cannot bound a long session's history. */
  rememberFinished(id: string, receipt: string): void {
    this.finished.delete(id);
    this.finished.set(id, receipt);
    if (this.finished.size > 128) this.finished.delete(this.finished.keys().next().value!);
  }
  constructor(
    private readonly pi: ExtensionAPI,
    private readonly env: NodeJS.ProcessEnv = process.env,
  ) {}

  get id(): string | undefined {
    return this.me?.id;
  }

  get parentId(): string | undefined {
    return this.env[PARENT_ID_ENV];
  }

  /** Bind to the session and, in a subagent, join straight away (the parent chose the id). */
  start(ctx: ExtensionContext): void {
    this.stop();
    this.ctx = ctx;
    this.schemaProblem = undefined;
    if (this.env[AGENT_ID_ENV]) this.join();
  }

  /** The database opens on first use, so a session that never collaborates never creates it. */
  private store(): TeamStore {
    if (!this.ctx) throw new Error('The team is not available before the session starts.');
    if (this.schemaProblem) throw new Error(this.schemaProblem);
    try {
      return (this.db ??= TeamStore.open(this.ctx.cwd, agentDbPath(this.env), this.env));
    } catch (error) {
      const problem = `Team database unavailable: ${errorMessage(error)}`;
      if (error instanceof AgentDbError) this.schemaProblem = problem;
      throw new Error(problem);
    }
  }

  /** Set when the team database is foreign or too new; nothing team-related works until it is fixed. */
  get problem(): string | undefined {
    return this.schemaProblem;
  }

  /** Join (or refresh) as this process. Idempotent; `task` replaces the one-line description. */
  join(task?: string): Member {
    const ctx = this.ctx;
    const db = this.store();
    if (!ctx) throw new Error('The team is not available before the session starts.');
    if (!this.me) {
      const id = this.env[AGENT_ID_ENV] ?? newId('main');
      const parentId = this.parentId;
      const description = storedText(task ?? this.env[AGENT_TASK_ENV], LINE_MAX);
      const now = Date.now();
      this.activeAt = now;
      this.me = {
        id,
        role: id.replace(ROLE_SUFFIX, ''),
        ...(parentId ? { parentId } : {}),
        pid: process.pid,
        status: ctx.isIdle() ? 'idle' : 'working',
        ...(description ? { task: description } : {}),
        joinedAt: now, updatedAt: now,
        toolCalls: 0, input: 0, output: 0, cost: 0,
      };
      this.ticker.sync();
    } else if (task !== undefined) {
      this.me.task = storedText(task, LINE_MAX);
    }
    this.flush(db);
    return this.me;
  }

  /** Leave the team and release everything held. The database stays open for the next join. */
  leave(): void {
    if (this.me && this.db) this.db.remove(this.me.id);
    this.me = undefined;
    this.ticker.sync();
    this.held.clear();
    this.inbox.reset();
  }

  /** Leave and close the database (session end). */
  stop(): void {
    this.leave();
    this.db?.close();
    this.db = undefined;
    this.ticker.reset();
    this.finished.clear();
    this.ctx = undefined;
  }

  /** Call `listener` on every tick with a fresh `view` (shared ticker and cache; watchers never read the database). */
  watch(listener: () => void): () => void {
    return this.ticker.watch(listener);
  }

  /** The latest snapshot the ticker read (for watchers). */
  get view(): TeamView {
    return this.ticker.view;
  }

  /**
   * The database only if someone already created it: reads never create it, since before anyone has collaborated
   * there is nothing to read. Throws when it exists but is unusable.
   */
  private existing(): TeamStore | undefined {
    return this.db || fs.existsSync(agentDbPath(this.env)) ? this.store() : undefined;
  }

  /** Live agents (none when the database cannot be read; `snapshot` says why). */
  members(): Member[] {
    return this.snapshot().members;
  }

  /** Live agents and the latest messages between them, from one read. */
  snapshot(): TeamView {
    try {
      const db = this.existing();
      return db ? { members: db.list(), traffic: db.recent() } : { members: [], traffic: [] };
    } catch (error) {
      return { members: [], traffic: [], error: this.schemaProblem ?? `Team database unavailable: ${errorMessage(error)}` };
    }
  }

  /** Queue a message for the agents `to` resolves to; returns who got it and the message id, or an error text. */
  send(to: string, text: string, options: { from?: string | undefined; replyRequired?: boolean; replyTo?: number } = {}): { sent: string[]; id: number; departed?: string[] } | { error: string } {
    const from = options.from ?? this.me?.id;
    if (!from) return { error: 'Join first: call coordinate with action "join".' };
    if (text.length > MESSAGE_MAX_CHARS) return { error: `Message exceeds ${MESSAGE_MAX_CHARS} characters; send a summary and a file path to the full content. Nothing was sent.` };
    const db = this.store();
    if (options.replyTo !== undefined && !db.replyTarget(options.replyTo)) return { error: `No message #${options.replyTo} in this repository to reply to; omit replyTo or use the number from the message you answer. Nothing was sent.` };
    // A broadcast stays in the sender's session tree; the user's and an API client's in this session's tree.
    const scope = outside(from) ? this.me?.id : from;
    if (to === 'all' && !scope) return { error: 'No agents in this session yet: nothing to broadcast to.' };
    const targets = resolveTarget(db.list(), from, to, this.parentId, scope);
    if (typeof targets === 'string') return { error: this.finished.has(to) ? `${to} has finished and takes no more messages; ${this.finished.get(to)} To continue its work, start a new agent and give it the context it needs.` : targets };
    // Replies default to FYI: an answer that asks for an answer makes two agents wake each other forever.
    const replyRequired = options.replyRequired ?? (to !== 'all' && !outside(from) && options.replyTo === undefined);
    // Stored sanitized: every reader (the model, the panel, other processes) gets plain text.
    const reply = options.replyTo !== undefined ? { replyTo: options.replyTo } : {};
    const { id, departed } = db.send(from, targets.map((target) => target.id), sanitizeTerminalText(text), { replyRequired, wake: outside(from), ...reply });
    if (id === undefined) return { error: `${departed.join(', ')} left the team; the message was not sent.` };
    const sent = targets.map((target) => target.id).filter((target) => !departed.includes(target));
    return { sent, id, ...(departed.length > 0 ? { departed } : {}) };
  }

  /** Reserve paths (a trailing `/` marks a whole directory tree) for this agent. */
  lock(paths: string[], reason: string): { ok: true; leases: Lease[]; held: number } | { ok: false; conflicts: Array<{ path: string; heldBy: Lease }> } {
    const me = this.join();
    // Stored resolved from this session's cwd, the way edits resolve; conflicts still show the paths as given.
    const shown = new Map(paths.map((entry) => [resolveLockPath(this.ctx?.cwd, entry), entry]));
    const requests: LeaseRequest[] = [...shown].map(([resolved, entry]) => ({ path: resolved, kind: /[\\/]$/.test(entry) ? 'tree' : 'file', reason }));
    const db = this.store();
    const result = db.lock(me.id, requests);
    if (!result.ok) return { ok: false, conflicts: result.conflicts.map((conflict) => ({ ...conflict, path: shown.get(conflict.path) ?? conflict.path })) };
    this.held.granted(result.leases.map((lease) => ({ key: `${lease.kind}:${pathKey(db.workspace, lease.path)}`, lease })));
    // The store returns every lease this agent holds: keep the requested ones, and count the rest apart.
    const asked = new Set(requests.map((request) => `${request.kind}:${pathKey(db.workspace, request.path)}`));
    return { ok: true, leases: result.leases.filter((lease) => asked.has(`${lease.kind}:${pathKey(db.workspace, lease.path)}`)), held: result.leases.length };
  }

  unlock(paths?: string[]): number {
    if (!this.me) return 0;
    const db = this.store();
    const resolved = paths?.map((entry) => resolveLockPath(this.ctx?.cwd, entry));
    const keys = resolved?.length ? resolved.flatMap((entry) => [`file:${pathKey(db.workspace, entry)}`, `tree:${pathKey(db.workspace, entry)}`]) : this.held.keys();
    this.held.release(keys);
    return db.unlock(this.me.id, resolved);
  }

  /** Whether this agent takes part in the team: it joined, or still tracks reservations. */
  private collaborating(): boolean {
    return this.me !== undefined || this.held.any;
  }

  /**
   * Whether this agent may change `file`: another agent's live lease, a reservation of ours that lapsed, or a
   * database error all forbid it. A database error fails closed only for an agent that collaborates; a solo session
   * (never joined, nothing held) keeps editing when the database is missing, foreign, too new or unusable.
   */
  private check(file: string): { lease?: Lease; lapsed?: string; error?: string } {
    try {
      const db = this.existing();
      if (!db) return {};
      const lease = db.conflict(file, this.me?.id ?? USER_SENDER);
      if (lease) return { lease };
      const lapsed = this.lapsedCover(db, file);
      return lapsed ? { lapsed } : {};
    } catch (error) {
      return this.collaborating() ? { error: dbFailure(error) } : {};
    }
  }

  /** A reservation of ours covering `file` that is no longer live in the database (now tracked as lapsed), if any. */
  private lapsedCover(db: TeamStore, file: string): string | undefined {
    const me = this.me;
    if (!me || !this.held.any) return undefined;
    return this.held.lapsedCover(pathKey(db.workspace, file), () => db.owned(me.id));
  }

  /** The live lease of another agent that covers this file, if any. */
  blockedBy(file: string): Lease | undefined {
    return this.check(file).lease;
  }

  /** Why the file may not be changed (another agent's reservation, our lapsed one, or an unreadable team database), or undefined. */
  reservation(absolute: string, display: string): string | undefined {
    const { lease, lapsed, error } = this.check(absolute);
    if (error) return `Team database unavailable (${error}); retry the change.`;
    if (lapsed) return `Your reservation on ${lapsed} lapsed; lock it again before editing ${display}.`;
    return lease ? sanitizeTerminalText(`${display} is reserved by ${lease.owner} (${lease.reason}; ${leaseAge(lease)}). Do other work, or sendMessage ${lease.owner} and retry after they unlock.`) : undefined;
  }

  /** Who changed which file recently, once the database exists; throws when it is unusable. */
  editLog(): EditLog | undefined {
    return this.existing()?.edits;
  }

  // Activity the parent and the user see: state, current tool, tokens.
  onAgentStart(): void {
    this.update((me) => void (me.status = 'working'));
  }

  /**
   * The run is over. A headless subagent (print/JSON mode, no UI) gets no further turn and exits next: it leaves now,
   * releasing its reservations, so a message sent from here on is a dead letter its sender hears about rather than an
   * FYI acknowledged into a context nobody reads.
   */
  onAgentSettled(): void {
    if (this.me && this.env[AGENT_ID_ENV] && this.ctx && !this.ctx.hasUI) {
      this.flush();
      this.leave();
      return;
    }
    // The activity line was cleared by src/ui/activity.ts, which owns it.
    this.update((me) => void (me.status = 'idle'));
  }

  /** A top-level tool call started (parallel calls each count; calls a tool makes itself do not). */
  onToolStart(): void {
    this.update((me) => void (me.toolCalls += 1));
  }

  /** What the agent does now (the line src/ui/activity.ts describes), or undefined between steps and when idle. */
  setActivity(line: string | undefined): void {
    this.update((me) => void (line ? (me.activity = storedText(line, LINE_MAX)) : delete me.activity));
  }

  onUsage(usage: { input?: number; output?: number; cacheRead?: number; cacheWrite?: number; cost?: { total?: number } }): void {
    this.update((me) => {
      me.input += (usage.input ?? 0) + (usage.cacheRead ?? 0) + (usage.cacheWrite ?? 0);
      me.output += usage.output ?? 0;
      me.cost += usage.cost?.total ?? 0;
    });
  }

  private update(change: (me: Member) => void): void {
    if (!this.me) return;
    change(this.me);
    this.dirty = true;
  }

  private flush(db: TeamStore | undefined = this.db): void {
    if (!this.me || !db) return;
    const now = Date.now();
    this.me.updatedAt = now;
    if (this.me.status === 'working') this.activeAt = now;
    db.save(this.me);
    // Reservations live while their owner works; an idle owner's lapse once it has been idle too long.
    const idle = now - this.activeAt >= leaseIdleMs(this.env);
    if (!idle) db.renew(this.me.id, now);
    this.savedAt = now;
    this.dirty = false;
    if (!this.held.holding) return;
    // A heartbeat stall can let a lease lapse (and a peer take the path): say so once, and refuse edits until re-locked.
    const lost = this.held.lost(db.owned(this.me.id, now));
    const why = idle ? ` after ${Math.round((now - this.activeAt) / 60_000)}m idle` : '';
    if (lost.length > 0) this.notice(`Your reservation on ${lost.join(', ')} lapsed${why}; lock it again before editing.`);
  }

  /** A team notice for this agent, read at its next step; never starts a turn. False when the session cannot take it. */
  private notice(text: string): boolean {
    try {
      this.pi.sendMessage({ customType: MESSAGE_TYPE, content: text, display: true, details: { from: 'team', text, at: Date.now() } }, { deliverAs: 'steer' });
      return true;
    } catch {
      return false; // The session ended or is not ready.
    }
  }

  /** Two-phase delivery: read what is waiting, hand each message to Pi, then record only the ones Pi accepted. */
  private deliverPending(): void {
    if (!this.me || !this.db) return;
    const db = this.db;
    try {
      db.quick(() => this.deliverNow(db));
      this.failedTicks = 0;
    } catch (error) {
      // A locked or closed database is retried on the next tick; say so once if it lasts long enough for leases to lapse.
      if (++this.failedTicks === FAILED_TICKS_NOTICE) this.notice(`Team database unavailable (${errorMessage(error)}): your reservations may lapse and messages wait until it recovers.`);
    }
  }

  private deliverNow(db: TeamStore): void {
    if (!this.me) return;
    this.inbox.deliver(db, this.me.id, dbStamp(agentDbPath(this.env)), (message) => this.deliver(message));
    this.reportDeadLetters(db, this.me.id);
    if (this.dirty || Date.now() - this.savedAt >= HEARTBEAT_MS) this.flush(db);
  }

  /** Tells this agent, once and without waking it, about messages it sent that their recipients left without reading. */
  private reportDeadLetters(db: TeamStore, id: string): void {
    const letters = db.deadLetters(id);
    if (letters.length === 0) return;
    const told: DeadLetter[] = [];
    for (const letter of letters) {
      const who = letter.recipients.join(', ');
      const text = sanitizeTerminalText(`Message #${letter.id} to ${who} was not read: ${letter.recipients.length === 1 ? `${who} left` : 'they left'} the team first. Its text began: "${letter.text.split('\n')[0]!.slice(0, 120)}"`);
      if (!this.notice(text)) break; // Told on a later tick.
      told.push(letter);
    }
    db.markNotified(told);
  }

  private deliver(message: Message): boolean {
    try {
      // steer: read at the next step of a busy agent; only a message that wants action wakes an idle one.
      this.pi.sendMessage({ customType: MESSAGE_TYPE, content: messageText(message, this.parentId), display: true, details: message }, { triggerTurn: wakes(message), deliverAs: 'steer' });
      return true;
    } catch {
      // The session ended or is not ready: the message stays pending.
      return false;
    }
  }
}
