import type { ExtensionContext } from '@earendil-works/pi-coding-agent';
import { USER_SENDER, type MessageEnvelope } from '../shared/messages.js';
import { visibleControls } from '../shared/sanitize.js';
import { errorMessage } from '../shared/util.js';
import type { Mail } from './mail.js';
import type { SendOptions } from './model.js';
import { userProof } from './proof.js';
import { interruptRefusal, outside, resolveTarget } from './routing.js';
import { spilledMessage } from './spill.js';
import type { TeamStore } from './store.js';

/** Resumes a finished subagent that still lingers (set by the subagents domain); its receipt, else undefined. */
export type Resumer = (to: string, text: string, from: string, signal?: AbortSignal) => Promise<string | undefined>;
/** The hook every outbound message passes first; a throw refuses the message with its text. */
export type MessageHook = (message: MessageEnvelope, ctx: ExtensionContext, signal?: AbortSignal) => Promise<void>;
export type SendRequest = SendOptions & { from?: string | undefined; signal?: AbortSignal };
export type SendResult = { sent: string[]; id: number; departed?: string[]; resumed?: string } | { error: string };
/** A message on its way out: its text, its sender and the request it came with. */
interface Outgoing {
  text: string;
  from: string;
  options: SendRequest;
}
/** Where a message goes: its sender, the recipient it names, the session tree a broadcast stays in, and the team database. */
interface Route {
  from: string;
  to: string;
  scope: string | undefined;
  db: TeamStore;
}

/** What the outbox reads from the team session it belongs to; each is read when a message is sent. */
interface OutboxHost {
  /** This agent's id, once it joined. */
  id(): string | undefined;
  parentId(): string | undefined;
  ctx(): ExtensionContext | undefined;
  /** The team database; throws before the session starts or when it is unusable. */
  store(): TeamStore;
}

/**
 * The sending side of a team session: resolves recipients, runs the outbound message hook, signs the user's messages
 * for the subagents this session started, and resumes a finished one that still lingers.
 */
export class Outbox {
  /** Secrets of the subagents this session started, by agent id (`vouch`): what signs a message the user types for one. */
  private readonly secrets = new Map<string, string>();
  /** Subagents this session started that have finished: id → where their report went, for a later message to them. */
  private readonly finished = new Map<string, string>();
  resumer: Resumer | undefined;
  messageHook: MessageHook | undefined;

  constructor(private readonly host: OutboxHost, private readonly mail: Mail) {}

  /**
   * Registers the secret this session gave subagent `id` (the secret in its `PERMISSION_NONCE_FILE_ENV` file), so `/agents tell` can sign for it and
   * the child can tell the user's message from a forged one. Returns what ends it, when the child is gone.
   */
  vouch(id: string, secret: string): () => void {
    this.secrets.set(id, secret);
    return () => {
      if (this.secrets.get(id) === secret) this.secrets.delete(id);
    };
  }

  /** Recent completion receipts; active-agent limits cannot bound a long session's history. */
  rememberFinished(id: string, receipt: string): void {
    this.finished.delete(id);
    this.finished.set(id, receipt);
    if (this.finished.size > 128) this.finished.delete(this.finished.keys().next().value!);
  }

  /** Ids of the finished subagents this session remembers. */
  finishedIds(): Iterable<string> {
    return this.finished.keys();
  }

  forgetFinished(): void {
    this.finished.clear();
  }

  /** Queue a message for the agents `to` resolves to; returns who got it and the message id, or an error text. */
  async send(recipient: string | undefined, text: string, options: SendRequest = {}): Promise<SendResult> {
    options.signal?.throwIfAborted();
    const route = this.route(recipient, options);
    if ('error' in route) return route;
    const { from, to, scope, db } = route;
    const message: Outgoing = { text, from, options };
    const members = db.list();
    const targets = resolveTarget(members, from, to, this.host.parentId(), scope);
    if (typeof targets === 'string') return this.toUnresolved(to, targets, message);
    const refused = options.interrupt ? interruptRefusal(members, targets, from, to, scope) : undefined;
    if (refused) return { error: refused };
    for (const target of targets) {
      const denied = await this.beforeSend(target.id, message);
      if (denied) return { error: denied };
    }
    options.signal?.throwIfAborted();
    return this.deliver(db, targets, message);
  }

  /** Who sends, to whom and within which session tree; an error text when the request names no one it can reach. */
  private route(recipient: string | undefined, options: SendRequest): Route | { error: string } {
    const from = options.from ?? this.host.id();
    if (!from) return { error: 'Join first: call coordinate with action "join".' };
    const db = this.host.store();
    const answered = options.replyTo === undefined ? undefined : db.replyTarget(options.replyTo);
    if (options.replyTo !== undefined && !answered) return { error: `No message #${options.replyTo} in this repository to reply to; omit replyTo or use the number from the message you answer. Nothing was sent.` };
    // A reply goes to the sender of the message it answers unless the caller says otherwise.
    const to = recipient ?? answered?.from;
    if (to === undefined) return { error: 'Say who to send to: `to` (an agent id, role, "parent" or "all"), or `replyTo` (the message you answer). Nothing was sent.' };
    // A broadcast stays in the sender's session tree; the user's and an API client's in this session's tree.
    const scope = outside(from) ? this.host.id() : from;
    if (to === 'all' && !scope) return { error: 'No agents in this session yet: nothing to broadcast to.' };
    return { from, to, scope, db };
  }

  /** A recipient no member answers to: a finished subagent this session started is resumed with the message; else `problem`. */
  private async toUnresolved(to: string, problem: string, message: Outgoing): Promise<SendResult> {
    if (this.finished.has(to) && this.resumer) {
      const denied = await this.beforeSend(to, message);
      if (denied) return { error: denied };
    }
    const resumed = this.finished.has(to) ? await this.resumer?.(to, message.text, message.from, message.options.signal) : undefined;
    if (resumed) return { sent: [to], id: 0, resumed };
    return { error: this.finished.has(to) ? `${to} has finished and takes no more messages; ${this.finished.get(to)} To continue its work, start a new agent and give it the context it needs.` : problem };
  }

  /** Runs the outbound message hook for recipient `name`; its refusal text, else undefined. */
  private async beforeSend(name: string, { text, from, options }: Outgoing): Promise<string | undefined> {
    const ctx = this.host.ctx();
    if (!this.messageHook || !ctx) return undefined;
    const signal = options.signal ? AbortSignal.any([options.signal, ...(ctx.signal ? [ctx.signal] : [])]) : ctx.signal;
    try {
      await this.messageHook({ channel: 'internal', direction: 'outbound', from, to: name, text, ...(options.replyTo !== undefined ? { replyTo: String(options.replyTo) } : {}) }, ctx, signal);
      options.signal?.throwIfAborted();
      if (this.host.ctx() !== ctx) throw new Error('The session changed before the message could be sent.');
    } catch (error) { return errorMessage(error); }
    return undefined;
  }

  /** Stores the message for `targets`: who got it and its id, or an error when it cannot be stored or a recipient left. */
  private deliver(db: TeamStore, targets: ReadonlyArray<{ id: string }>, { text, from, options }: Outgoing): SendResult {
    // A message is FYI unless the sender says it needs an answer: a default that asked for answers made two agents wake each other forever.
    const replyRequired = options.replyRequired ?? false;
    // Stored with controls shown as escapes: every reader (the model, the panel, other processes) gets text that cannot steer a terminal, and the evidence stays.
    const delivered = spilledMessage(visibleControls(text), from);
    if (typeof delivered !== 'string') return delivered;
    const reply = options.replyTo !== undefined ? { replyTo: options.replyTo } : {};
    // Only the user's messages are signed, and only for a child this session started: any other recipient reads them as unverified.
    const sign = from === USER_SENDER ? (id: number, child: string) => this.signed(id, child, delivered) : undefined;
    const flags = { replyRequired, wake: outside(from), interrupt: options.interrupt === true, ...reply, ...(sign ? { sign } : {}) };
    const { id, departed } = db.send(from, targets.map((target) => target.id), delivered, flags);
    if (id === undefined) return { error: `${departed.join(', ')} left the team; the message was not sent.` };
    if (from === USER_SENDER) this.mail.createdForUser(id);
    const sent = targets.map((target) => target.id).filter((target) => !departed.includes(target));
    return { sent, id, ...(departed.length > 0 ? { departed } : {}) };
  }

  /** The signature of the user's message `text` (stored as message `id`) for child `recipient`; undefined when this session did not start it. */
  private signed(id: number, recipient: string, text: string): string | undefined {
    const secret = this.secrets.get(recipient);
    return secret ? userProof(secret, { id, to: recipient, text }) : undefined;
  }
}
