import fs from 'node:fs';
import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { addReply, errorMessage } from '../shared/util.js';
import { storedText } from '../shared/sanitize.js';
import { AGENT_ID_ENV, PARENT_ID_ENV, PARENT_PID_ENV, TeamStore } from './store.js';
import { envFlag, ownNonce, SUBAGENT_ENV } from '../shared/env.js';
import { interactive } from '../shared/interactive.js';
import { terminateSelf } from '../shared/process.js';
import type { Lease, Member } from './model.js';
import { Mail } from './mail.js';
import { LEASE_IDLE_MS } from './held.js';
import { Delivery } from './delivery.js';
import type { EditLog } from './edits.js';
import { AgentDbError } from '../agentdb/schema.js';
import { agentDbPath } from '../shared/home.js';
import { agentName, LINE_MAX, newMember } from './identity.js';
import { TeamTicker, type TeamView } from './ticker.js';
import type { Ask } from './asks.js';
import { Outbox, type MessageHook, type Resumer, type SendRequest, type SendResult } from './outbox.js';
import { Reservations } from './reservations.js';
import { watchParentProcess } from './parent.js';

/** Refresh the agent's row at least this often so peers can tell it is alive (and its leases stay valid). */
const HEARTBEAT_MS = 5_000;
/** Names drawn in turn when other processes keep taking them. */
const JOIN_ATTEMPTS = 8;

/** This process's place in the team: joins, reports what it is doing, holds reservations and delivers messages sent to it. */
export class Team {
  private me: Member | undefined;
  private db: TeamStore | undefined;
  private ctx: ExtensionContext | undefined;
  /** The one ticker: delivery and heartbeats while joined, a cached view for watchers (the panel). */
  private readonly ticker = new TeamTicker({
    joined: () => this.me !== undefined,
    beat: () => this.delivery.deliver(this.db),
    dbFile: () => {
      const file = agentDbPath(this.env);
      return this.ctx && (this.db || fs.existsSync(file)) ? file : undefined;
    },
    // Reads never write, but share the connection's busy wait: never wait long for another process's lock.
    read: () => (this.db ? this.db.quick(() => this.snapshot()) : this.snapshot()),
    resumed: (since, now) => {
      try {
        this.existing()?.resume(since, now);
      } catch {
        // Unusable database: the snapshot says why; nothing to repair.
      }
    },
    maxAgeMs: HEARTBEAT_MS,
  });
  /** A status or goal change waits for the next tick; activity, tool counts and usage ride the heartbeat. */
  private statusChanged = false;
  private savedAt = 0;
  /** Why the database cannot be used at all (a foreign file); shown in /agents and the footer. */
  private schemaProblem: string | undefined;
  /** When this agent last worked (or joined): an idle owner's reservations lapse `LEASE_IDLE_MS` (`held.ts`) after it. */
  private activeAt = 0;
  private readonly reservations = new Reservations({
    id: () => this.me?.id,
    existing: () => this.existing(),
    cwd: () => this.ctx?.cwd,
    sessionId: () => this.ctx?.sessionManager.getSessionId(),
    notice: (text) => this.mail.notice(text),
  });
  private readonly mail: Mail;
  private readonly delivery: Delivery;
  private readonly outbox: Outbox;
  /** Stops the parent check (a subagent only). */
  private stopParentWatch: (() => void) | undefined;
  /** Resumes a finished subagent that still lingers (set by the subagents domain); its receipt, else undefined. */
  setResumer(resume: Resumer): void { this.outbox.resumer = resume; }
  setMessageHook(hook: MessageHook): void { this.outbox.messageHook = hook; }

  /** Recent completion receipts; active-agent limits cannot bound a long session's history. */
  rememberFinished(id: string, receipt: string): void {
    this.outbox.rememberFinished(id, receipt);
  }
  constructor(pi: ExtensionAPI, private readonly env: NodeJS.ProcessEnv = process.env) {
    this.mail = new Mail(pi, () => ownNonce(this.env));
    this.delivery = new Delivery({
      id: () => this.me?.id,
      parentId: () => this.parentId,
      ctx: () => this.ctx,
      headless: () => this.headless(),
      heartbeat: (db) => this.heartbeat(db),
    }, this.mail);
    this.outbox = new Outbox({ id: () => this.me?.id, parentId: () => this.parentId, ctx: () => this.ctx, store: () => this.store() }, this.mail);
  }

  /**
   * Registers the secret this session gave subagent `id` (the secret in its `PERMISSION_NONCE_FILE_ENV` file), so `/agents tell` can sign for it and
   * the child can tell the user's message from a forged one. Returns what ends it, when the child is gone.
   */
  vouch(id: string, secret: string): () => void {
    return this.outbox.vouch(id, secret);
  }

  /** Aborted while an interrupt waits for this agent's run to stop (`agent_before_settle` waits end early on it). */
  get interrupted(): AbortSignal { return this.mail.interrupted; }

  get id(): string | undefined {
    return this.me?.id;
  }

  get parentId(): string | undefined {
    return this.env[PARENT_ID_ENV];
  }

  /** Bind to the session and, in a subagent, join straight away (the parent chose the id). */
  start(ctx: ExtensionContext): void {
    try { this.stop(); } catch { /* the old row stays (locked or closed database) and ages out; stop() cleared the state */ }
    this.ctx = ctx;
    this.schemaProblem = undefined;
    this.watchParent();
    if (this.env[AGENT_ID_ENV]) this.join();
  }

  /**
   * A crashed parent's child spends tokens for nobody: once the parent is gone, Pi's SIGTERM path disposes the session
   * and exits (ARCHITECTURE: Liveness). Its own unref'd timer, so a child that could not join the team stops too.
   */
  private watchParent(): void {
    const parent = this.env[PARENT_PID_ENV];
    if (!parent || this.stopParentWatch) return;
    this.stopParentWatch = watchParentProcess(parent, () => {
      this.stopParentWatch = undefined;
      // Where a signal ends the process outright (Windows) nothing runs after it: the row and reservations go now, not when a peer's sweep finds the pid dead.
      try { this.leave(); } catch { /* locked database: the sweep reaps the row */ }
      terminateSelf();
    });
  }

  /** The database opens on first use, so a session that never collaborates never creates it. */
  private store(): TeamStore {
    if (!this.ctx) throw new Error('The team is not available before the session starts.');
    if (this.schemaProblem) throw new Error(this.schemaProblem);
    try {
      return (this.db ??= TeamStore.shared(this.ctx.cwd, this.env));
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

  /** A name held by no member of the shared database, finished subagent of this session or caller in `inUse` (a subagent not yet in it, or a name another process took first). */
  newName(inUse: Iterable<string> = []): string {
    let live: string[] = [];
    try {
      live = this.existing()?.names() ?? [];
    } catch {
      // An unusable database holds no names this process can read; avoid the rest.
    }
    return agentName([...live, ...this.outbox.finishedIds(), ...inUse, ...(this.me ? [this.me.id] : [])]);
  }

  /** Join (or refresh) as this process. Idempotent; `goal` replaces the one-line goal (validated by the caller with `checkGoal`, `identity.ts`). */
  join(goal?: string): Member {
    const db = this.store(); // throws before the session starts
    const ctx = this.ctx!;
    if (!this.me) {
      this.me = this.claimName(db, ctx, goal);
      this.activeAt = this.me.joinedAt;
      this.ticker.sync();
    } else if (goal !== undefined) {
      this.me.goal = storedText(goal, LINE_MAX);
    }
    this.flush(db);
    return this.me;
  }

  /**
   * Takes a name in the shared database: the one the parent chose for a subagent, else a free one. Two processes can
   * draw the same free name; the database gives it to the first, and the other draws again (never overwrites the row).
   */
  private claimName(db: TeamStore, ctx: ExtensionContext, goal: string | undefined): Member {
    const chosen = this.env[AGENT_ID_ENV];
    const lost: string[] = [];
    for (let attempt = 0; attempt < JOIN_ATTEMPTS; attempt += 1) {
      const member = newMember(this.env, ctx, chosen ?? this.newName(lost), goal, Date.now());
      if (db.join(member)) return member;
      if (chosen) throw new Error(`The team name ${chosen} is held by another running agent.`);
      lost.push(member.id);
    }
    throw new Error('No free team name: the names kept being taken by other agents joining at the same time. Try again.');
  }

  /** Leave the team and release everything held; the database stays open for the next join. State clears first: a refused removal still throws, but leaves nothing stale (the row ages out). */
  leave(): void {
    const me = this.me;
    this.me = undefined;
    this.ticker.sync();
    this.reservations.clear();
    this.delivery.reset(this.db, me?.id);
    this.mail.reset();
    // Only a row that still names this process: a name another process holds now is not ours to remove.
    if (me && this.db) this.db.remove(me.id, Date.now(), me);
  }

  /** Leave and let go of the database (session end; the process connection stays open for the next session). */
  stop(): void {
    try { this.leave(); } finally {
      this.stopParentWatch?.();
      this.stopParentWatch = undefined;
      this.db = undefined;
      this.reservations.close();
      this.ticker.reset();
      this.outbox.forgetFinished();
      this.ctx = undefined;
    }
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
      if (this.schemaProblem) return { members: [], traffic: [], error: this.schemaProblem };
      return { members: [], traffic: [], error: `Team database unavailable: ${errorMessage(error)}`, retrying: true };
    }
  }

  /**
   * The questions `child` sent its `parent` since `since`, with their answers. None when the database cannot be read:
   * this is a measurement, so it never fails the run it describes.
   */
  asks(child: string, parent: string, since: number): Ask[] {
    try {
      return this.existing()?.asks(child, parent, since) ?? [];
    } catch {
      return [];
    }
  }

  /** Queue a message for the agents `to` resolves to; returns who got it and the message id, or an error text. */
  send(recipient: string | undefined, text: string, options: SendRequest = {}): Promise<SendResult> {
    return this.outbox.send(recipient, text, options);
  }

  /** Reserve paths (a trailing `/` marks a whole directory tree) for this agent. */
  lock(paths: string[], reason: string): { ok: true; leases: Lease[]; held: number } | { ok: false; conflicts: Array<{ path: string; heldBy: Lease }> } {
    const me = this.join();
    return this.reservations.lock(this.store(), me.id, paths, reason);
  }

  unlock(paths?: string[]): number {
    if (!this.me) return 0;
    return this.reservations.unlock(this.store(), this.me.id, paths);
  }

  /** Why the file may not be changed (another agent's reservation, in this team or in agents-communication; our lapsed one; or an unreadable team database), or undefined. */
  reservation(absolute: string, display: string): string | undefined {
    return this.reservations.reservation(absolute, display);
  }

  /** Who changed which file recently, once the database exists; throws when it is unusable. */
  editLog(): EditLog | undefined {
    return this.existing()?.edits;
  }

  // Activity the parent and the user see: state, current tool, tokens.
  onAgentStart(): void {
    this.update((me) => void (me.status = 'working'), true);
  }

  /** Delivers held interrupts first; a headless child then leaves because its parent closes its input. */
  onAgentSettled(): void {
    if (this.mail.settled()) return; // An interrupt stopped the run: its message starts the next turn.
    if (this.me && this.headless()) {
      this.flush();
      this.leave();
      return;
    }
    // The activity line was cleared by src/ui/activity.ts, which owns it.
    this.update((me) => void (me.status = 'idle'), true);
  }

  /** A subagent (no person to talk to, whatever its mode): once its run is over it gets no further turn. */
  private headless(): boolean {
    return Boolean(this.env[AGENT_ID_ENV] && this.ctx && !interactive(this.ctx, envFlag(this.env, SUBAGENT_ENV)));
  }

  /** Pi is about to call the model with `messages`: team mail among them is now delivered. */
  onContext(messages: ReadonlyArray<{ role?: string; customType?: string; details?: unknown }>): void {
    this.delivery.seen(this.db, messages);
  }

  /** A top-level tool call started (parallel calls each count; calls a tool makes itself do not). */
  onToolStart(): void {
    this.update((me) => void (me.toolCalls += 1));
  }

  /** What the agent does now (the line src/ui/activity.ts describes), or undefined between steps and when idle. */
  setActivity(line: string | undefined): void {
    this.update((me) => void (line ? (me.activity = storedText(line, LINE_MAX)) : delete me.activity));
  }

  /** One reply's usage; `model` wrote it (it changes when the user switches models); `unpriced`: that model has no pricing. */
  onUsage(usage: unknown, model: string | undefined, unpriced: boolean): void {
    this.update((me) => {
      if (model) me.model = storedText(model, LINE_MAX);
      addReply(me, usage, unpriced);
    });
  }

  /** Changes this agent's row in memory; `status` changes are written on the next tick, the rest with the heartbeat. */
  private update(change: (me: Member) => void, status = false): void {
    if (!this.me) return;
    change(this.me);
    if (status) this.statusChanged = true;
  }

  private flush(db: TeamStore | undefined = this.db): void {
    if (!this.me || !db) return;
    const now = Date.now();
    this.me.updatedAt = now;
    if (this.me.status === 'working') this.activeAt = now;
    if (!db.save(this.me)) throw new Error(`The team name ${this.me.id} is held by another running agent; this agent's row cannot be refreshed.`);
    // Reservations live while their owner works; an idle owner's lapse once it has been idle too long.
    const idle = now - this.activeAt >= LEASE_IDLE_MS;
    if (!idle) db.renew(this.me.id, now);
    this.savedAt = now;
    this.statusChanged = false;
    this.reservations.noticeLost(db, this.me.id, now, idle ? ` after ${Math.round((now - this.activeAt) / 60_000)}m idle` : '');
  }

  /** Writes the heartbeat when a status changed or `HEARTBEAT_MS` passed since the last; whether it did. */
  private heartbeat(db: TeamStore): boolean {
    if (!this.statusChanged && Date.now() - this.savedAt < HEARTBEAT_MS) return false;
    this.flush(db);
    return true;
  }
}
