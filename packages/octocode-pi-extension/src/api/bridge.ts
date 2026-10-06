import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { Box, Text } from '@earendil-works/pi-tui';
import { EventLog } from './events.js';
import { apiDir, ensurePrivateDir, newInstanceId, newToken, removeRecord, socketPath, writeRecord, type InstanceRecord } from './registry.js';
import { ApiServer, type Connection } from './server.js';
import { DEFAULT_EVENT_TYPES, ERR, numberParam, PROTOCOL_VERSION, RpcError, stringParam } from './protocol.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { envFlag } from '../shared/env.js';
import { formatClock } from '../shared/format.js';
import { clip, resultBlock } from '../shared/render.js';
import { capChars, contentText, errorMessage, toolHint } from '../shared/util.js';

const API_ENV = 'OCTOCODE_API';
const API_HTTP_ENV = 'OCTOCODE_API_HTTP';
const EXTERNAL_MESSAGE_TYPE = 'octocode-external-message';

/** Messages land in the agent's context and are never trimmed. */
const EXTERNAL_MAX_CHARS = 32_000;
const WAIT_DEFAULT_MS = 600_000;
const RESULT_MAX_CHARS = 64_000;

/** What the API needs from the team (kept as an interface so the API domain stays independent of it). */
export interface AgentDirectory {
  list(): unknown[];
  /** Sends a team message; a result with `error` (unknown recipient, nobody to reach) is a refusal. */
  tell(to: string, text: string): unknown;
  /** Longest message body the team accepts (default 8,000 characters). */
  maxChars?: number;
}

/** The team's own cap on one message body, used when the directory does not say. */
const AGENT_MESSAGE_MAX_CHARS = 8_000;

interface ApiOptions {
  /** Serve on a Unix socket (default when enabled; not available on Windows). */
  socket: boolean;
  /** Serve on loopback HTTP: a port, 0 for any free port, or undefined for no HTTP. */
  http?: number | undefined;
}

/** The transports the environment asks for, or undefined when the API is off. */
export function optionsFromEnv(env: NodeJS.ProcessEnv = process.env, platform = process.platform): ApiOptions | undefined {
  const httpText = env[API_HTTP_ENV]?.trim();
  const port = httpText === undefined || httpText === '' ? undefined : Number(httpText);
  if (port !== undefined && (!Number.isInteger(port) || port < 0 || port > 65_535)) return undefined;
  const on = envFlag(env, API_ENV);
  if (!on && port === undefined) return undefined;
  const socket = on && platform !== 'win32';
  // Windows has no Unix sockets here: HTTP on a free port is the only transport.
  return { socket, http: port ?? (socket ? undefined : 0) };
}

/**
 * Where a tool call sits in the call tree. Calls a tool makes itself (`ctx.executeTool`, e.g. codemode scripts) carry
 * `parentToolCallId`, and Pi names them `<parent>/<n>`; provider ids contain no `/`, so the parent's segment count is
 * the depth: `toolu_1` → 0, `toolu_1/1` (parent `toolu_1`) → 1, `toolu_1/1/2` (parent `toolu_1/1`) → 2.
 */
export function nesting(parentId: string | undefined): { depth: number; parentId?: string } {
  return parentId ? { parentId, depth: parentId.split('/').length } : { depth: 0 };
}

interface Waiter {
  resolve(result: { text: string; stopReason?: string }): void;
  reject(error: Error): void;
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
  private dir = apiDir();
  private readonly waiters = new Set<Waiter>();
  /** The last run's final assistant text, handed to `wait` callers when Pi settles. */
  private lastResult: { text: string; stopReason?: string } | undefined;
  private counter = 0;
  private lastAssistantText = '';
  /** When each running tool call's `tool.start` was published, keyed by call id; cleared on start/stop. */
  private readonly toolStarts = new Map<string, number>();

  constructor(
    private readonly pi: ExtensionAPI,
    private readonly agents: AgentDirectory,
    private readonly version: () => string | undefined,
    private readonly env: NodeJS.ProcessEnv = process.env,
  ) {}

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
    registerExternalMessageRenderer(pi);
    pi.on('agent_start', () => {
      // A new run: `wait` must never answer with the previous run's text.
      this.lastResult = undefined;
      if (this.server) log.publish('agent.start', {});
    });
    pi.on('agent_end', (event) => {
      if (!this.server) return;
      const last = [...event.messages].reverse().find((message) => message.role === 'assistant');
      const text = last ? contentText(last.content, { separator: '' }) : '';
      const stopReason = last && 'stopReason' in last ? String(last.stopReason) : undefined;
      this.lastAssistantText = text;
      this.lastResult = { text: capChars(text, RESULT_MAX_CHARS), ...(stopReason ? { stopReason } : {}) };
      log.publish('agent.end', this.lastResult);
    });
    // `wait` answers once Pi has settled: after agent_end it may still continue (a queued follow-up, a compaction
    // retry), and the caller wants the final answer, not an intermediate one.
    pi.on('agent_settled', () => {
      if (!this.server) return;
      const result = this.lastResult ?? { text: '' };
      log.publish('agent.settled', result);
      for (const waiter of [...this.waiters]) {
        this.waiters.delete(waiter);
        waiter.resolve(result);
      }
    });
    pi.on('message_end', (event) => {
      if (!this.server) return;
      const { role } = event.message;
      if (role === 'assistant' || role === 'user') log.publish('message', { role, text: capChars(contentText(event.message.content, { separator: '' }), RESULT_MAX_CHARS) });
    });
    pi.on('message_update', (event) => {
      if (!this.server) return;
      const inner = event.assistantMessageEvent as { type?: string; delta?: unknown };
      if (inner.type === 'text_delta' && typeof inner.delta === 'string') log.publish('message.delta', { delta: inner.delta });
    });
    pi.on('tool_execution_start', (event) => {
      if (!this.server) return;
      this.toolStarts.set(event.toolCallId, Date.now());
      log.publish('tool.start', { id: event.toolCallId, name: event.toolName, hint: toolHint(event.args), ...nesting(event.parentToolCallId) });
    });
    pi.on('tool_execution_end', (event) => {
      if (!this.server) return;
      const started = this.toolStarts.get(event.toolCallId);
      this.toolStarts.delete(event.toolCallId);
      log.publish('tool.end', {
        id: event.toolCallId,
        name: event.toolName,
        isError: event.isError,
        ...nesting(event.parentToolCallId),
        ...(started !== undefined ? { durationMs: Date.now() - started } : {}),
      });
    });
    pi.on('session_compact', () => void (this.server && log.publish('compaction', {})));
  }

  /** Bind to the session and start serving when the environment asks for it. */
  async start(ctx: ExtensionContext, options: ApiOptions | undefined = optionsFromEnv(this.env)): Promise<void> {
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
        record.socket = socketPath(this.dir, id);
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
    this.toolStarts.clear();
    for (const waiter of this.waiters) waiter.reject(new RpcError(ERR.unavailable, 'The session ended'));
    this.waiters.clear();
    if (record) removeRecord(this.dir, record.id);
    await server?.close();
    this.log.close();
  }

  private session(): ExtensionContext {
    if (!this.ctx) throw new RpcError(ERR.unavailable, 'No active session');
    return this.ctx;
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
      };
    } catch {
      throw new RpcError(ERR.unavailable, 'The session was replaced');
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
        const wasBusy = !ctx.isIdle();
        ctx.abort();
        return { aborted: wasBusy };
      }
      case 'messages.list':
        return this.history(numberParam(params, 'limit', { min: 1, max: 100 }) ?? 20);
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

  /** `agents.tell`: bounded like any team message; a refusal (unknown agent, nobody reachable) is an RPC error. */
  private tell(params: Record<string, unknown>): unknown {
    const to = stringParam(params, 'to')!;
    const text = this.label(stringParam(params, 'text', { max: this.agents.maxChars ?? AGENT_MESSAGE_MAX_CHARS })!);
    const result = this.agents.tell(to, text);
    if (result && typeof result === 'object' && typeof (result as { error?: unknown }).error === 'string') throw new RpcError(ERR.invalidParams, (result as { error: string }).error);
    return result;
  }

  private label(text: string): string {
    return sanitizeTerminalText(text);
  }

  private async send(params: Record<string, unknown>): Promise<unknown> {
    const ctx = this.session();
    const text = this.label(stringParam(params, 'text', { max: EXTERNAL_MAX_CHARS })!);
    const from = sanitizeTerminalText(stringParam(params, 'from', { max: 64, optional: true }) ?? 'external client').replace(/[\r\n"\s]+/g, ' ').trim() || 'external client';
    const mode = params['mode'] ?? 'auto';
    if (mode !== 'auto' && mode !== 'steer' && mode !== 'followUp') throw new RpcError(ERR.invalidParams, 'mode must be auto, steer or followUp');
    const wait = params['wait'] === true;
    const timeout = numberParam(params, 'timeoutMs', { min: 1_000, max: 3_600_000 }) ?? WAIT_DEFAULT_MS;
    const id = `x${++this.counter}`;
    const busy = !ctx.isIdle();
    const deliverAs = mode === 'followUp' ? 'followUp' : 'steer';

    let waiting: Promise<{ text: string; stopReason?: string }> | undefined;
    let timer: NodeJS.Timeout | undefined;
    if (wait) {
      waiting = new Promise((resolve, reject) => {
        const waiter: Waiter = { resolve, reject };
        this.waiters.add(waiter);
        timer = setTimeout(() => {
          this.waiters.delete(waiter);
          reject(new RpcError(ERR.timeout, 'Timed out waiting for the agent to finish; the message was delivered', { id }));
        }, timeout);
        timer.unref();
      });
      waiting.catch(() => undefined);
    }
    try {
      this.pi.sendMessage(
        { customType: EXTERNAL_MESSAGE_TYPE, content: `Message from ${from} through the Octocode API:\n${text}`, display: true, details: { id, from, at: Date.now() } },
        { triggerTurn: true, deliverAs },
      );
    } catch (error) {
      clearTimeout(timer);
      throw new RpcError(ERR.unavailable, `The session could not take the message: ${errorMessage(error)}`);
    }
    this.log.publish('external.message', { id, from, text: capChars(text, 2_000), queued: busy ? deliverAs : 'turn' });
    const accepted = { id, queued: busy ? deliverAs : 'turn' };
    if (!waiting) return accepted;
    try {
      return { ...accepted, ...(await waiting) };
    } finally {
      clearTimeout(timer);
    }
  }

  private history(limit: number): unknown {
    const ctx = this.session();
    const messages: Array<{ role: string; text: string }> = [];
    for (const entry of ctx.sessionManager.getBranch()) {
      const message = (entry as { type?: string; message?: { role?: string; content?: unknown } }).message;
      if ((entry as { type?: string }).type !== 'message' || !message?.role) continue;
      if (message.role !== 'user' && message.role !== 'assistant') continue;
      const text = contentText(message.content, { separator: '' });
      if (text) messages.push({ role: message.role, text: capChars(text, 8_000) });
    }
    return { messages: messages.slice(-limit), last: this.lastAssistantText ? capChars(this.lastAssistantText, 8_000) : null };
  }
}

/** `✉ from <client>`, then the client's text: untrusted, so resultBlock sanitizes and previews it instead of Markdown. */
function registerExternalMessageRenderer(pi: ExtensionAPI): void {
  pi.registerMessageRenderer(EXTERNAL_MESSAGE_TYPE, (message, { expanded, outputPad }, theme) => {
    const details = message.details as { from?: unknown; at?: unknown } | undefined;
    const from = clip(String(details?.from ?? 'API client'), 60);
    const text = contentText(message.content);
    // The content leads with `Message from <client> through the Octocode API:` for the model; the header says it.
    const newline = text.indexOf('\n');
    const body = newline >= 0 && text.slice(0, newline).endsWith(' through the Octocode API:') ? text.slice(newline + 1) : text;
    const box = new Box(outputPad ?? 1, 0);
    box.addChild(new Text(`${theme.fg('accent', '✉')} ${theme.fg('toolTitle', theme.bold(`from ${from}`))} ${theme.fg('dim', `· Octocode API · ${formatClock(typeof details?.at === 'number' ? details.at : Date.now())}`)}`, 0, 0));
    box.addChild(resultBlock(theme, { expanded }, { summary: '', body, error: false }));
    return box;
  });
}
