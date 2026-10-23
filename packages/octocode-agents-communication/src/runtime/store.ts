/** Bound workspace/session operations over the shared coordination protocol. */
import { randomUUID } from 'node:crypto';
import { statSync } from 'node:fs';
import * as activity from './activity.js';
import * as catalog from './catalog.js';
import { completionCheck } from './completion.js';
import { execute, openDatabase, query, rowInteger, rowOptionalText, rowText, transaction, type Database, type Row, type SqlValue } from './database.js';
import type { AttachInput, ClaudeInbound, CommandInputs, CommandResults, HostInboundData, CompleteInput, CompletionCheckInput, RecordUsageInput, RetryDeliveryInput, SendMessageInput } from './catalog-types.generated.js';
import type { DeliveryApi, Dispatched, Staged } from './delivery-types.js';
import * as delivery from './dispatch.js';
import * as outcomes from './dispatch-outcomes.js';
import { ValidationError } from './errors.js';
import { RESERVED_KEY_PREFIXES } from './expiry.js';
import { health } from './health.js';
import * as leases from './leases.js';
import { peerContext, peersChanged } from './peers.js';
import { answerSlots, finishRequest, isRequest, messageSlots, wakesByDefault } from './protocols.js';
import { fetch, record } from './records.js';
import { inboxItem, sessionRow, type Session } from './rows.js';
import { Registration, register } from './registry.js';
import { context, readDocument, shareDocument } from './documents.js';
import * as sessions from './sessions.js';
import type { AttachmentHost } from './store-types.js';
import { stripNulls } from './store-types.js';
import { truncateCodePoints } from './text.js';
import { HOUR_MS, now } from './timing.js';
import { jsonEqual, parseObject, type JsonObject } from './validation.js';
import { compact, dumps } from './json-text.js';
import { checkScope, coordinationScope, scopeWorkspaces, storedScope } from './workspace.js';
import { realpath } from './paths.js';

const INBOX_COLUMNS = 'm.id,m.sender,s.name AS senderName,m.kind,m.body,m.reasoning,m.topic,m.expiresAt,m.wake,m.conversationId,m.replyTo,m.replyRequired FROM deliveries d JOIN messages m ON m.id=d.message JOIN sessions s ON s.id=m.sender';

/** What `send` stores: a send_message input, or the final reply `complete` builds from its own input. */
type Outgoing = SendMessageInput & { replyTo?: number; slots?: JsonObject; outcome?: CompleteInput['outcome'] };

export class Store implements AttachmentHost, DeliveryApi {
  readonly workspace: string;
  readonly coordinationScope: string;
  readonly database: string;
  readonly db: Database;
  private readonly identity: { dev: bigint; ino: bigint };
  private readonly writer: boolean;
  private readonly registration: Registration | null;
  private idleInbox: string | null = null;

  constructor(database: string, workspace: string, readOnly = false, create = false) {
    this.workspace = realpath(workspace, true);
    this.coordinationScope = coordinationScope(this.workspace);
    this.database = database;
    this.db = openDatabase(this.database, readOnly, create);
    this.identity = statSync(this.database, { bigint: true });
    this.writer = !readOnly;
    // A non-default database is announced where tools that read only the default one can find it.
    this.registration = this.writer ? register(this.database) : null;
  }

  close(): void {
    if (this.writer) {
      try {
        this.db.exec('PRAGMA optimize');
      } catch {
        /* optimizing is best effort */
      }
    }
    this.db.close();
  }

  checkDatabase(): void {
    let current;
    try {
      current = statSync(this.database, { bigint: true });
    } catch {
      throw new ValidationError('Coordination database disappeared; stop this worker');
    }
    if (!current.isFile()) throw new ValidationError('Coordination database is no longer a file; stop this worker');
    if (current.dev !== this.identity.dev || current.ino !== this.identity.ino) throw new ValidationError('Coordination database was replaced; stop this worker');
  }

  known(session: string, active = true): Session {
    this.checkDatabase();
    checkScope(this.db, this.workspace, this.coordinationScope);
    const rows = query(this.db, 'SELECT * FROM sessions WHERE id=? AND workspace=? AND (?=0 OR expiresAt>?)', [session, this.workspace, active, now()]);
    if (!rows.length) throw new ValidationError('Unknown or expired session in this workspace. Refresh peers and copy the exact current ID; never reconstruct it. If your bound identity expired, its lifecycle owner resumes it (a raw CLI agent runs resume {"vendor":...} itself), then acquire fresh leases before writing.');
    return sessionRow(rows[0]!);
  }

  peer(id: string): Session {
    const rows = query(this.db, 'SELECT * FROM sessions WHERE id=?', [id]);
    if (!rows.length || storedScope(this.db, rowText(rows[0]!, 'workspace')) !== this.coordinationScope) throw new ValidationError('Unknown session in this repository. Refresh peers and use its exact ID.');
    return sessionRow(rows[0]!);
  }

  sharedWorkspaces(): string[] {
    return scopeWorkspaces(this.db, this.coordinationScope, this.workspace);
  }

  /**
   * Live peers in one repository scope have distinct names, so a name can address a peer.
   * A taken name gets the first free -2, -3 … suffix, as Claude Code does for sessions.
   */
  uniqueName(name: string, exclude = ''): string {
    const taken = sessions.liveNames(this, exclude);
    let candidate = name;
    for (let number = 2; taken.has(candidate); number += 1) {
      const suffix = '-' + number;
      candidate = truncateCodePoints(name, 256 - suffix.length) + suffix;
    }
    return candidate;
  }

  /** Maps a `to` value to a session ID: an exact ID, else a unique live peer name, else the only session ever named so in this repository. */
  resolvePeer(value: string): string {
    if (query(this.db, 'SELECT 1 FROM sessions WHERE id=?', [value]).length) return value;
    const scope = dumps(this.sharedWorkspaces());
    for (const live of [true, false]) {
      const rows = query(this.db, 'SELECT id FROM sessions WHERE name=? AND workspace IN (SELECT value FROM json_each(?)) AND (?=0 OR expiresAt>?) ORDER BY id LIMIT 2', [value, scope, live, now()]);
      if (rows.length === 1) return rowText(rows[0]!, 'id');
      if (rows.length) throw new ValidationError(`Name ${value} matches several ${live ? 'live ' : ''}peers; use the exact ID from peers`);
    }
    throw new ValidationError(`Unknown peer ${value}. Use a peer name or exact ID from peers.`);
  }

  hostIdentities(vendor: string, host: string): { id: string }[] {
    return query(this.db, 'SELECT s.id FROM sessions s LEFT JOIN attachments a ON a.session=s.id WHERE s.workspace=? AND s.vendorSession=? AND (s.vendor=? OR a.transport=?) ORDER BY s.id LIMIT 2', [this.workspace, host, vendor, vendor]).map(row => ({ id: rowText(row, 'id') }));
  }

  /** The commands the store runs; the others (delivery loops, hooks, database tools) are run by their own modules. */
  private readonly handlers: { [N in catalog.CommandName]?: (session: string, input: CommandInputs[N]) => CommandResults[N] | Promise<CommandResults[N]> } = {
    fetch: (session, input) => fetch(this, session, input),
    record: (session, input) => record(this, session, input),
    activity: async (session, input) => {
      this.known(session, false);
      return activity.read(this.workspace, input);
    },
    binding: session => {
      const identity = this.known(session, false);
      return sessions.withCheckpoint(this, session, { ...identity, coordinationScope: this.coordinationScope, active: identity.expiresAt > now() });
    },
    health: (_session, input) => health(this, input),
    peers: (_session, input) => catalog.page(query(this.db, 'SELECT id,workspace,(SELECT coordinationScope FROM workspaces w WHERE w.workspace=sessions.workspace) AS coordinationScope,name,vendor,vendorSession,branch,task,status,expiresAt FROM sessions WHERE workspace IN (SELECT value FROM json_each(?)) AND expiresAt>? AND id>? ORDER BY id LIMIT 101', [dumps(this.sharedWorkspaces()), now(), input.after ?? '']).map(row => ({ ...sessionRow(row), coordinationScope: rowOptionalText(row, 'coordinationScope') })), true, 'peers', input),
    inbox: (session, input) => {
      if (input.message === undefined) return this.inbox(session, input.after ?? 0);
      this.known(session);
      return catalog.page(query(this.db, 'SELECT ' + INBOX_COLUMNS + ' WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND m.expiresAt>? AND m.id=?', [session, now(), input.message]).map(inboxItem), false, 'inbox', input);
    },
    join: (_session, input) => sessions.join(this, input),
    resume: (session, input) => sessions.resume(this, session, input),
    heartbeat: (session, input) => sessions.heartbeat(this, session, input),
    set_status: (session, input) => sessions.setStatus(this, session, input),
    leave: session => sessions.leave(this, session),
    subscribe: (session, input) => sessions.subscribe(this, session, input),
    prune: () => sessions.prune(this),
    check_write: (session, input) => leases.checkWrite(this, session, input),
    check_paths: (session, input) => leases.checkPaths(this, session, input),
    share_document: (session, input) => shareDocument(this, session, input),
    read_document: (session, input) => readDocument(this, session, input),
    context: (session, input) => context(this, session, input),
    locks: (session, input) => leases.locks(this, session, input),
    lock: (session, input) => leases.lock(this, session, input),
    lock_many: (session, input) => leases.lockMany(this, session, input),
    renew: (session, input) => leases.renew(this, session, input),
    unlock: (session, input) => leases.unlock(this, session, input),
    send_message: (session, input) => this.send(session, input, false, false),
    // notify_all's input is a subset of send_message's.
    notify_all: (session, input) => this.send(session, { ...input }, true, false),
    complete: (session, input) => this.complete(session, input),
  };

  /** Runs one catalog command; only `activity` waits on anything (Git), every database step is synchronous. */
  async call<N extends catalog.CommandName>(session: string, name: N, input: CommandInputs[N]): Promise<CommandResults[N]> {
    this.checkDatabase();
    this.registration?.touch();
    catalog.command(name, input);
    const handler = this.handlers[name];
    if (!handler) throw new ValidationError('Unknown operation: ' + name);
    return stripNulls(await handler(session, input));
  }

  private complete(session: string, input: CompleteInput): CommandResults['complete'] {
    if ('reply' in input) {
      const [slots, body] = answerSlots(this, session, input);
      // The outcome also travels in reasoning, which inbox returns beside the BLUF body.
      const reasoning = `Outcome: ${input.outcome}` + ('reasoning' in input ? '; ' + input.reasoning : '');
      catalog.text({ reasoning }, 'reasoning');
      return this.send(session, { replyTo: input.message, body, slots, outcome: input.outcome, reasoning, key: `complete:${input.message}`, wake: 'action' }, false, true);
    }
    return transaction(this.db, () => {
      this.known(session);
      const messages: number[] = input.messages ?? [input.message!];
      const required = query(this.db, 'SELECT m.id FROM deliveries d JOIN messages m ON m.id=d.message WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND m.replyRequired=1 AND m.id IN (SELECT value FROM json_each(?)) LIMIT 1', [session, dumps(messages)]);
      if (required.length) throw new ValidationError(`Message ${rowInteger(required[0]!, 'id')} requires a final answer: use complete with message and reply, or leave unfinished work pending`);
      const at = now();
      for (const message of messages) {
        if (execute(this.db, 'UPDATE deliveries SET acknowledgedAt=coalesce(acknowledgedAt,?) WHERE message=? AND recipient=?', [at, message, session]) !== 1) throw new ValidationError('Completion requires every ID to be received by this session');
      }
      return { completed: true, count: messages.length };
    });
  }

  send(session: string, input: Outgoing, broadcast?: boolean, completing?: false): CommandResults['send_message'];
  send(session: string, input: Outgoing, broadcast: boolean, completing: true): Extract<CommandResults['complete'], { id: number }>;
  send(session: string, input: Outgoing, broadcast = false, completing = false): CommandResults['send_message'] | Extract<CommandResults['complete'], { id: number }> {
    if (!completing && 'replyTo' in input) throw new ValidationError('Final replies require complete with message and reply');
    if (!broadcast && (('to' in input && 'topic' in input) || !['to', 'topic', 'replyTo'].some(key => key in input))) throw new ValidationError('Supply exactly one of to or topic. For replies use complete with message and reply.');
    let target: string | null = broadcast ? '*' : 'to' in input ? catalog.text(input, 'to') : 'topic' in input ? catalog.text(input, 'topic') : null;
    const reasoning = catalog.text(input, 'reasoning');
    // The kind decides the policy: requests owe an answer; requests and blockers wake.
    const kind = completing ? 'answer' : catalog.kindOf(input);
    const replyRequired = isRequest(kind);
    const duration = catalog.ttl(input, HOUR_MS);
    const wake: string = input.wake ?? (wakesByDefault(kind) ? 'action' : 'passive');
    const key = 'key' in input ? catalog.text(input, 'key') : randomUUID();
    if (!completing && RESERVED_KEY_PREFIXES.some(prefix => key.startsWith(prefix))) throw new ValidationError(`Keys beginning ${RESERVED_KEY_PREFIXES.join(', ')} are reserved for final replies and runtime notices`);
    return transaction(this.db, () => {
      this.known(session);
      if ('to' in input && !broadcast) target = this.resolvePeer(target!);
      let slots: JsonObject | null;
      let body: string;
      let conversation: string | undefined;
      if (completing) {
        // `complete` always supplies both.
        slots = input.slots!;
        body = input.body!;
      } else {
        [slots, body, conversation] = messageSlots(this, session, kind, input, target);
      }
      catalog.text({ body }, 'body');
      let replySender: string | null = null;
      if (input.replyTo !== undefined) {
        const parents = query(this.db, 'SELECT m.conversationId,m.sender,m.replyRequired FROM messages m JOIN sessions s ON s.id=m.sender WHERE m.id=? AND s.workspace IN (SELECT value FROM json_each(?)) AND (m.sender=? OR EXISTS(SELECT 1 FROM deliveries d WHERE d.message=m.id AND d.recipient=?))', [input.replyTo, dumps(this.sharedWorkspaces()), session, session]);
        if (!parents.length) throw new ValidationError(`Unknown message ${input.replyTo} for this session. Use data.messageId from fetch or item.id from inbox, never a recordId.`);
        const parent = parents[0]!;
        if (!parent.replyRequired) throw new ValidationError(`Message ${input.replyTo} is informational and accepts no reply. Handle it with complete using message or messages only; start a new direct request for new work`);
        conversation = rowOptionalText(parent, 'conversationId');
        replySender = rowText(parent, 'sender');
      }
      target = target ?? replySender;
      if (target === null) throw new ValidationError('Reply parent has no sender');
      if (completing && (replySender !== target || !query(this.db, 'SELECT 1 FROM deliveries WHERE message=? AND recipient=?', [input.replyTo!, session]).length)) throw new ValidationError('Completion requires replying to a message received by this session');
      const receipt = (id: number, recipients: number, effects: ReturnType<typeof finishRequest> = {}, offline = false): CommandResults['send_message'] | Extract<CommandResults['complete'], { id: number }> => {
        if (!completing) return { id, recipients, ...(offline ? { recipientOffline: true as const } : {}) };
        execute(this.db, 'UPDATE deliveries SET acknowledgedAt=? WHERE message=? AND recipient=? AND acknowledgedAt IS NULL', [now(), input.replyTo!, session]);
        return { id, recipients, ...(offline ? { recipientOffline: true as const } : {}), completed: true, count: 1, ...effects };
      };
      const existing = query(this.db, 'SELECT * FROM messages WHERE sender=? AND key=?', [session, key]);
      if (existing.length) {
        const row = existing[0]!;
        const expected: Row = { target, kind, body, topic: input.topic, reasoning, wake, conversationId: conversation, replyTo: input.replyTo, replyRequired, ttlMs: duration };
        if (Object.entries(expected).some(([field, value]) => (row[field] ?? null) !== (value ?? null)) || !jsonEqual(rowOptionalText(row, 'slots') === undefined ? null : parseObject(rowText(row, 'slots'), 'message slots'), slots ?? null))
          throw new ValidationError(completing ? 'Final reply already stored with different content; use send_message for new work' : 'Message key reused with different content (target, topic, body, reasoning, wake, correlation or ttlMs); use a new key');
        return receipt(rowInteger(row, 'id'), this.deliveryCount(rowInteger(row, 'id')));
      }
      if (completing && query(this.db, 'SELECT 1 FROM deliveries WHERE message=? AND recipient=? AND acknowledgedAt IS NOT NULL', [input.replyTo!, session]).length) throw new ValidationError('Message already completed without this reply; send_message for new work');
      let offline = false;
      let recipients: { id: string }[];
      if (broadcast) recipients = query(this.db, 'SELECT id FROM sessions WHERE workspace IN (SELECT value FROM json_each(?)) AND expiresAt>? AND id<>?', [dumps(this.sharedWorkspaces()), now(), session]).map(row => ({ id: rowText(row, 'id') }));
      else if (!('topic' in input)) {
        offline = this.peer(target).expiresAt <= now();
        recipients = [{ id: target }];
      } else recipients = query(this.db, 'SELECT s.id FROM sessions s JOIN subscriptions t ON s.id=t.session WHERE t.topic=? AND s.workspace IN (SELECT value FROM json_each(?)) AND s.expiresAt>? AND s.id<>?', [target, dumps(this.sharedWorkspaces()), now(), session]).map(row => ({ id: rowText(row, 'id') }));
      const id = this.db.run('INSERT INTO messages(sender,target,topic,body,key,expiresAt,reasoning,wake,conversationId,replyTo,ttlMs,replyRequired,kind,slots) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)', [session, target, input.topic ?? null, body, key, now() + duration, reasoning, wake, conversation ?? null, input.replyTo ?? null, duration, replyRequired, kind, slots === null ? null : compact(slots)] as SqlValue[]).lastInsertRowid;
      for (const recipient of recipients) execute(this.db, 'INSERT INTO deliveries(message,recipient) VALUES(?,?)', [id, recipient.id]);
      const effects = completing ? finishRequest(this, session, input.replyTo!, id, input.outcome!, now()) : {};
      return receipt(id, this.deliveryCount(id), effects, offline);
    });
  }

  private deliveryCount(message: number): number {
    return rowInteger(query(this.db, 'SELECT count(*) AS n FROM deliveries WHERE message=?', [message])[0]!, 'n');
  }

  inbox(session: string, after = 0): CommandResults['inbox'] {
    this.known(session);
    const state = `${session}\0${after}\0${this.db.value('PRAGMA data_version')}\0${this.db.value('SELECT total_changes()')}`;
    if (this.idleInbox === state) return { items: [] };
    const rows = query(this.db, 'SELECT ' + INBOX_COLUMNS + ' WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND m.expiresAt>? AND d.message>? ORDER BY d.message LIMIT 101', [session, now(), after]);
    const result = catalog.page(rows.map(inboxItem), false, 'inbox', { after });
    this.idleInbox = rows.length ? null : state;
    return result;
  }

  completionCheck(session: string, input: CompletionCheckInput): CommandResults['completion-check'] {
    return completionCheck(this, session, input);
  }

  peersChanged(session: string, consumer: string, generation: string): boolean {
    return peersChanged(this, session, consumer, generation);
  }

  peerContext(session: string, consumer: string, generation: string): string {
    return peerContext(this, session, consumer, generation);
  }

  attach(session: string, input: AttachInput): CommandResults['attach'] { return delivery.attach(this, session, input); }
  attachment(session: string): CommandResults['attach'] { return delivery.attachment(this, session); }
  present(session: string, managed = false): Promise<void> { return delivery.present(this, session, managed); }
  hostInbound(session: string): HostInboundData | null { return delivery.hostInbound(this, session); }
  claudeInbound(session: string, endpoint: string, ownChild?: boolean): ClaudeInbound { return delivery.claudeInbound(this, session, endpoint, ownChild); }
  stage(session: string, mode: string, actionOnly?: boolean | null): Staged { return delivery.stage(this, session, mode, actionOnly); }
  stageBound(session: string, mode: string, binding: CommandResults['attach'] | null = null, actionOnly: boolean | null = null): Staged { return delivery.stageBound(this, session, mode, binding, actionOnly); }
  hasDispatchable(session: string, actionOnly: boolean): boolean { return delivery.hasDispatchable(this, session, actionOnly); }
  finishDispatch(session: string, items: readonly Dispatched[], error: string | null = null): void { outcomes.finishDispatch(this, session, items, error); }
  releaseDispatch(session: string, items: readonly Dispatched[], reason: string): void { outcomes.releaseDispatch(this, session, items, reason); }
  retryDelivery(session: string, input: RetryDeliveryInput): CommandResults['retry_delivery'] { return outcomes.retryDelivery(this, session, input); }
  recordUsage(session: string, input: RecordUsageInput): CommandResults['record_usage'] { return outcomes.recordUsage(this, session, input); }
  ensureBindingChangeAllowed(session: string): void { delivery.ensureBindingChangeAllowed(this, session); }
  reconcileCompletedOrphan(session: string): void { delivery.reconcileCompletedOrphan(this, session); }
}
