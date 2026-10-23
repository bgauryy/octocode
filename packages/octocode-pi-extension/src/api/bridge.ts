import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { boundedExternal, boundedText } from './bounded.js';
import { EventLog, nesting } from './events.js';
import { registerExternalMessageRenderer } from './external-message.js';
import { HISTORY_MAX_PAGE, HISTORY_PAGE, historyPage } from './history.js';
import type { ApiOptions } from './options.js';
import { apiDir, ensurePrivateDir, newInstanceId, newToken, removeRecord, socketPath, writeRecord, type InstanceRecord } from './registry.js';
import { SendReceipts, type RunResult, type SendReceipt } from './receipts.js';
import { ApiServer, type Connection } from './server.js';
import { clampedParam, DEFAULT_EVENT_TYPES, ERR, MESSAGE_KEY_MAX, numberParam, PROTOCOL_VERSION, RpcError, stringParam, WAIT_MS } from './protocol.js';
import { apiClientLabel, fenceUntrusted, headerText, messageHeader } from '../shared/fence.js';
import { redactData, redactSecrets, visibleControls } from '../shared/sanitize.js';
import { EXTERNAL_MESSAGE_TYPE, EXTERNAL_SENDER, USER_SENDER, type MessageEnvelope } from '../shared/messages.js';
import { sha256 } from '../shared/atomic.js';
import { capChars, contentText, errorMessage, isRecord, isStaleSessionError, toolHint } from '../shared/util.js';

/** Session entry (`pi.appendEntry`, never sent to the model) written for every turn an API client starts. */
const EXTERNAL_AUDIT_TYPE = 'octocode-external';
/** Senders a client may not claim: the user, the team's own API sender, and (checked per call) any live agent. */
const RESERVED_SENDERS = new Set([USER_SENDER, EXTERNAL_SENDER]);

/** What the API needs from the team (kept as an interface so the API domain stays independent of it). */
export interface AgentDirectory {
  list(): unknown[];
  /** Sends a team message (the team saves a long body to a file and delivers a pointer); a result with `error` (unknown recipient, nobody to reach) is a refusal. */
  tell(to: string, text: string): unknown;
  /** What a reader of a message from `from` gets for `text`: the text, or (when it is long) its head and the path of a file with all of it; an `error` when it cannot be saved. */
  deliverable(text: string, from: string): string | { error: string };
}

export interface ApiOperations {
  messageSend?(message: MessageEnvelope, ctx: ExtensionContext, signal?: AbortSignal): Promise<void>;
  externalCall?(params: unknown, ctx: ExtensionContext): Promise<unknown>;
}

/**
 * An external API for one running Pi session: other programs discover it, send it messages, follow what it does and
 * ask it to stop. JSON-RPC 2.0 over a private Unix socket (and optionally authenticated loopback HTTP + SSE), the
 * shape Codex's app-server and ACP use, scoped to this extension's needs.
 */
export class OctocodeApi {
  readonly log = new EventLog();
  private ctx: ExtensionContext | undefined;
  private server: ApiServer | undefined;
  private record: InstanceRecord | undefined;
  private dir: string;
  /** The last run's final assistant text, handed to `wait` callers when Pi settles. */
  private lastResult: RunResult | undefined;
  private counter = 0;
  private readonly receipts = new SendReceipts();
  private lastAssistantText = '';
  private pendingDelta = '';

  constructor(
    private readonly pi: ExtensionAPI,
    private readonly agents: AgentDirectory,
    private readonly version: () => string | undefined,
    private readonly env: NodeJS.ProcessEnv = process.env,
    private readonly operations: ApiOperations = {},
  ) {
    this.dir = apiDir(env);
  }

  get running(): boolean {
    return this.server !== undefined;
  }

  get directory(): string {
    return this.dir;
  }

  get instance(): InstanceRecord | undefined {
    return this.record;
  }

  /** Publish Pi's events to subscribers. Cheap when nobody listens: the log only keeps a short ring buffer. */
  observe(): void {
    const { pi, log } = this;
    const flushDelta = (): void => {
      if (!this.pendingDelta) return;
      const delta = boundedText(this.pendingDelta);
      this.pendingDelta = '';
      log.publish('message.delta', { delta });
    };
    registerExternalMessageRenderer(pi);
    pi.on('agent_start', () => {
      // A new run: `wait` must never answer with the previous run's text.
      this.lastResult = undefined;
      this.pendingDelta = '';
      if (this.server) log.publish('agent.start', {});
    });
    pi.on('agent_end', (event) => {
      if (!this.server) return;
      flushDelta();
      const last = [...event.messages].reverse().find((message) => message.role === 'assistant');
      const text = last ? contentText(last.content, { separator: '' }) : '';
      const stopReason = last && 'stopReason' in last ? String(last.stopReason) : undefined;
      this.lastAssistantText = text;
      this.lastResult = { text: boundedText(text), ...(stopReason ? { stopReason } : {}) };
      log.publish('agent.end', this.lastResult);
    });
    // `wait` answers once Pi has settled: after agent_end it may still continue (a queued follow-up, a compaction
    // retry), and the caller wants the final answer, not an intermediate one.
    pi.on('agent_settled', (event) => {
      if (!this.server) return;
      const result = { ...(this.lastResult ?? { text: '' }), ...(event.aborted ? { stopReason: 'aborted' } : {}) };
      this.receipts.settle(result);
      log.publish('agent.settled', result);
    });
    pi.on('message_end', (event) => {
      if (!this.server) return;
      const { role } = event.message;
      if (role === 'assistant') flushDelta();
      if (role === 'assistant' || role === 'user') log.publish('message', { role, text: boundedText(contentText(event.message.content, { separator: '' })) });
    });
    pi.on('message_update', (event) => {
      if (!this.server) return;
      const inner = event.assistantMessageEvent as { type?: string; delta?: unknown };
      if (inner.type === 'text_delta' && typeof inner.delta === 'string') this.pendingDelta += inner.delta;
    });
    pi.on('tool_execution_start', (event) => {
      if (!this.server) return;
      log.publish('tool.start', { id: event.toolCallId, name: event.toolName, hint: toolHint(event.args), ...nesting(event.parentToolCallId) });
    });
    pi.on('tool_execution_end', (event) => {
      if (!this.server) return;
      log.publish('tool.end', {
        id: event.toolCallId,
        name: event.toolName,
        isError: event.isError,
        ...nesting(event.parentToolCallId),
        ...(event.durationMs !== undefined ? { durationMs: event.durationMs } : {}),
      });
    });
    pi.on('session_compact', () => void (this.server && log.publish('compaction', {})));
  }

  /** Bind to the session, and serve on `options` (undefined: bound, not serving). */
  async start(ctx: ExtensionContext, options: ApiOptions | undefined): Promise<void> {
    await this.stop();
    this.ctx = ctx;
    if (!options) return;
    this.dir = apiDir(this.env);
    ensurePrivateDir(this.dir);
    const id = newInstanceId();
    const token = newToken();
    const server = new ApiServer((method, params, connection) => this.dispatch(method, params, connection), this.log, token);
    try {
      const record: InstanceRecord = { id, pid: process.pid, cwd: ctx.cwd, startedAt: Date.now(), protocol: PROTOCOL_VERSION };
      if (options.socket) {
        record.socket = socketPath(this.dir, id, this.env);
        await server.listenUnix(record.socket);
      }
      if (options.http !== undefined) {
        await server.listenHttp(options.http);
        record.http = { url: server.httpUrl!, token };
      }
      this.server = server;
      this.record = record;
      writeRecord(this.dir, record);
    } catch (error) {
      await server.close();
      throw new Error(`Octocode API could not start: ${errorMessage(error)}`);
    }
  }

  async stop(): Promise<void> {
    const { server, record } = this;
    this.server = undefined;
    this.record = undefined;
    this.ctx = undefined;
    this.lastResult = undefined;
    this.lastAssistantText = '';
    this.pendingDelta = '';
    this.receipts.end();
    if (record) removeRecord(this.dir, record.id);
    await server?.close();
    this.log.close();
  }

  private session(): ExtensionContext {
    if (!this.ctx) throw new RpcError(ERR.unavailable, 'No active session');
    this.receipts.follow(this.ctx.sessionManager.getSessionId());
    return this.ctx;
  }

  private guardSession(params: Record<string, unknown>, ctx: ExtensionContext): void {
    const expected = stringParam(params, 'expectedSessionId', { optional: true });
    if (expected !== undefined && expected !== ctx.sessionManager.getSessionId()) throw new RpcError(ERR.unavailable, 'The expected session was replaced; nothing was sent', { execution: 'not-sent' });
  }

  private status(): Record<string, unknown> {
    const ctx = this.session();
    try {
      const usage = ctx.getContextUsage();
      return {
        state: ctx.isIdle() ? 'idle' : 'working',
        pending: ctx.hasPendingMessages(),
        cwd: ctx.cwd,
        sessionId: ctx.sessionManager.getSessionId(),
        sessionName: ctx.sessionManager.getSessionName() ?? null,
        model: ctx.model ? `${ctx.model.provider}/${ctx.model.id}` : null,
        context: usage ?? null,
        seq: this.log.seq,
        ...(this.receipts.activeKey ? { activeMessageId: this.receipts.active()?.accepted.id } : {}),
      };
    } catch (error) {
      // Pi's own words for a context used after its session was replaced; anything else is a real failure, reported as such.
      if (isStaleSessionError(error)) throw new RpcError(ERR.unavailable, 'The session was replaced');
      throw new RpcError(ERR.internal, `The session status could not be read: ${errorMessage(error)}`);
    }
  }

  private async dispatch(method: string, params: Record<string, unknown>, connection: Connection): Promise<unknown> {
    switch (method) {
      case 'ping':
        return {};
      case 'initialize':
        return {
          protocolVersion: PROTOCOL_VERSION,
          instance: { id: this.record?.id, pid: process.pid, cwd: this.ctx?.cwd, version: this.version() },
          capabilities: { events: DEFAULT_EVENT_TYPES, optionalEvents: ['message.delta'], wait: true, agents: true, streaming: connection.subscribe !== undefined },
        };
      case 'status':
        return this.status();
      case 'message.send':
        return this.send(params);
      case 'turn.abort': {
        const ctx = this.session();
        this.guardSession(params, ctx);
        const key = stringParam(params, 'key', { optional: true, max: MESSAGE_KEY_MAX });
        if (key !== undefined && this.receipts.abort(key) === 'cancelled') return { aborted: false, cancelledPreparation: true };
        const wasBusy = !ctx.isIdle();
        ctx.abort();
        return { aborted: wasBusy };
      }
      case 'external.call': {
        if (!this.operations.externalCall) throw new RpcError(ERR.unavailable, 'External agents are not registered');
        try { return boundedExternal(await this.operations.externalCall(params, this.session())); }
        catch (error) {
          const details = isRecord(error) ? error : {};
          const data = Object.fromEntries(['partial', 'execution', 'stderr'].filter((key) => details[key] !== undefined).map((key) => [key, details[key]]));
          const evidence = boundedExternal(data);
          const visible = isRecord(evidence) && evidence['fullOutput'] ? { ...redactData({ execution: data['execution'] }), ...evidence } : evidence;
          throw new RpcError(ERR.unavailable, redactSecrets(errorMessage(error)), visible);
        }
      }
      case 'messages.list':
        return this.history(clampedParam(params, 'limit', { min: 1, max: HISTORY_MAX_PAGE }) ?? HISTORY_PAGE, clampedParam(params, 'before', { min: 0, max: Number.MAX_SAFE_INTEGER }));
      case 'events.subscribe': {
        if (!connection.subscribe) throw new RpcError(ERR.invalidRequest, 'Streaming needs the socket transport or GET /events');
        const since = numberParam(params, 'since', { min: 0, max: Number.MAX_SAFE_INTEGER });
        const types = params['types'];
        if (types !== undefined && (!Array.isArray(types) || types.some((type) => typeof type !== 'string'))) throw new RpcError(ERR.invalidParams, 'types must be an array of strings');
        return connection.subscribe({ ...(since !== undefined ? { since } : {}), ...(types ? { types: types as string[] } : {}) });
      }
      case 'events.unsubscribe':
        connection.unsubscribe?.();
        return {};
      case 'agents.list':
        return { agents: this.agents.list() };
      case 'agents.tell':
        return this.tell(params);
      default:
        throw new RpcError(ERR.methodNotFound, `Unknown method ${method}`);
    }
  }

  /** `agents.tell`: a long text is the team's to save and point to; a refusal (unknown agent, nobody reachable) is an RPC error. */
  private async tell(params: Record<string, unknown>): Promise<unknown> {
    const to = stringParam(params, 'to')!;
    const text = visibleControls(stringParam(params, 'text')!);
    const result = await this.agents.tell(to, text);
    this.audit({ method: 'agents.tell', to, chars: text.length, accepted: !(result && typeof result === 'object' && 'error' in result) });
    if (result && typeof result === 'object' && typeof (result as { error?: unknown }).error === 'string') throw new RpcError(ERR.invalidParams, (result as { error: string }).error);
    return result;
  }

  /** Records an API-started turn in the session (a `octocode-external` entry), so what an outside program caused can be reviewed. */
  private audit(data: Record<string, unknown>): void {
    try {
      this.pi.appendEntry(EXTERNAL_AUDIT_TYPE, { at: Date.now(), ...data });
    } catch {
      // The session ended: nothing to record in.
    }
  }

  /** The sender name a client gave, on one line; a name that would pass for the user, the team or a live agent is refused. */
  private sender(params: Record<string, unknown>): string {
    const given = stringParam(params, 'from', { max: 64, optional: true });
    const from = headerText((given ?? 'external client').replaceAll('"', ' '), 64) || 'external client';
    const taken = this.agents.list().some((agent) => typeof agent === 'object' && agent !== null && (agent as { id?: unknown }).id === from);
    if (RESERVED_SENDERS.has(from.toLowerCase()) || taken) throw new RpcError(ERR.invalidParams, `The sender name ${JSON.stringify(from)} is reserved for the user or a team agent; name your client instead (for example "ci bot"). Nothing was sent.`);
    return from;
  }

  private async send(params: Record<string, unknown>): Promise<unknown> {
    const ctx = this.session();
    const originalSessionId = ctx.sessionManager.getSessionId();
    this.guardSession(params, ctx);
    const from = this.sender(params);
    const raw = stringParam(params, 'text')!;
    const whole = visibleControls(raw);
    const mode = params['mode'] ?? 'auto';
    if (mode !== 'auto' && mode !== 'steer' && mode !== 'followUp') throw new RpcError(ERR.invalidParams, 'mode must be auto, steer or followUp');
    if (params['requireIdle'] !== undefined && typeof params['requireIdle'] !== 'boolean') throw new RpcError(ERR.invalidParams, 'requireIdle must be a boolean');
    const requireIdle = params['requireIdle'] === true;
    const wait = params['wait'] === true;
    const timeout = numberParam(params, 'timeoutMs', WAIT_MS) ?? WAIT_MS.default;
    const key = stringParam(params, 'key', { optional: true, max: MESSAGE_KEY_MAX });
    const fingerprint = sha256(JSON.stringify({ raw, from, mode, requireIdle }));
    const prior = key ? this.receipts.get(key) : undefined;
    if (prior) {
      if (prior.fingerprint !== fingerprint) throw new RpcError(ERR.invalidParams, 'The message key was already used for different input; nothing was sent', { execution: 'not-sent' });
      if (wait) return { ...prior.accepted, ...(await this.receipts.wait(prior, timeout)) };
      await prior.ready;
      return prior.accepted;
    }
    const busy = !ctx.isIdle() || ctx.hasPendingMessages() || this.receipts.busy;
    if (requireIdle && busy) throw new RpcError(ERR.unavailable, 'The session is busy; nothing was sent', { execution: 'not-sent' });
    this.receipts.assertRoom(key);
    let text: string;
    const id = `x${++this.counter}`;
    const deliverAs = mode === 'followUp' ? 'followUp' : 'steer';
    const at = Date.now();
    const accepted = { id, queued: busy ? deliverAs : 'turn' };
    const { receipt, waiter } = this.receipts.open(fingerprint, accepted, key, busy);
    const message: MessageEnvelope = { channel: 'external', direction: 'inbound', from, to: originalSessionId, text: raw, id };
    const deadline = Date.now() + timeout;
    const preparingTimer = setTimeout(() => this.receipts.cancelPreparation(receipt, 'Message preparation timed out; nothing was sent'), timeout);
    preparingTimer.unref();
    let dispatched = false;
    try {
      if (this.operations.messageSend) await Promise.race([this.operations.messageSend(message, ctx, receipt.controller.signal), receipt.settlement]);
      this.guardPreparedSend(params, ctx, originalSessionId, key, receipt);
      const given = this.agents.deliverable(whole, from);
      if (typeof given !== 'string') throw new RpcError(ERR.unavailable, given.error);
      text = given;
      clearTimeout(preparingTimer);
      this.receipts.accept(receipt, waiter, key !== undefined || wait);
      dispatched = true;
      this.pi.sendMessage(
        { customType: EXTERNAL_MESSAGE_TYPE, content: fenceUntrusted(messageHeader(apiClientLabel(from), at), text), display: true, details: { id, from, at, message } },
        { triggerTurn: true, deliverAs },
      );
      receipt.resolveReady();
    } catch (error) {
      clearTimeout(preparingTimer);
      const execution = dispatched ? 'uncertain' : 'not-sent';
      const failure = new RpcError(ERR.unavailable, `The session could not take the message: ${errorMessage(error)}`, { execution, ...(dispatched ? accepted : {}) });
      this.receipts.fail(receipt, waiter, key, failure, dispatched);
      throw failure;
    }
    this.audit({ method: 'message.send', id, from, mode: deliverAs, queued: accepted.queued, chars: whole.length });
    this.log.publish('external.message', { id, from, text: redactSecrets(capChars(text, 2_000)), queued: accepted.queued });
    if (!wait) return accepted;
    try { return { ...accepted, ...(await this.receipts.wait(receipt, Math.max(1, deadline - Date.now()))) }; }
    finally { if (!key) this.receipts.release(waiter); }
  }

  private guardPreparedSend(params: Record<string, unknown>, original: ExtensionContext, sessionId: string, key: string | undefined, receipt: SendReceipt): void {
    const ctx = this.session();
    this.guardSession(params, ctx);
    if (ctx !== original || ctx.sessionManager.getSessionId() !== sessionId) throw new RpcError(ERR.unavailable, 'The session was replaced; nothing was sent');
    if (receipt.phase !== 'preparing') throw new RpcError(ERR.unavailable, 'Message preparation was cancelled; nothing was sent');
    if (params['requireIdle'] === true && (!ctx.isIdle() || ctx.hasPendingMessages() || this.receipts.activeKey !== key || this.receipts.preparingCount > 1)) {
      throw new RpcError(ERR.unavailable, 'The session became busy; nothing was sent');
    }
  }

  private history(limit: number, before?: number): unknown {
    return historyPage(this.session().sessionManager.getBranch(), this.lastAssistantText, limit, before);
  }
}
