import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { realpathSync, statSync } from 'node:fs';
import { Readable, Writable } from 'node:stream';
import {
  client, methods, ndJsonStream, PROTOCOL_VERSION,
  type ClientConnection, type RequestPermissionRequest, type RequestPermissionResponse, type SessionNotification,
} from '@agentclientprotocol/sdk';
import { adoptTree, KILL_GRACE_MS, prepareTree, stopTree } from '../shared/process.js';
import { errorMessage, untilAborted } from '../shared/util.js';
import type { ExternalBackend, ExternalReply } from './protocol.js';

interface AcpConnectInput {
  command: string;
  args?: string[];
  vendor: string;
  cwd: string;
  sessionId?: string;
  timeoutMs?: number;
}

export type AcpPermission = (request: RequestPermissionRequest, signal: AbortSignal) => Promise<RequestPermissionResponse>;

export class AcpError extends Error {
  readonly execution = 'uncertain';
  constructor(message: string, readonly partial: ExternalReply, readonly stderr: string, cause: unknown) {
    super(`${message}; external execution may have occurred. The prompt was not replayed.`, { cause });
    this.name = 'AcpError';
  }
}

function timeoutSignal(timeoutMs: number): AbortSignal {
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs <= 0 || timeoutMs > 2 ** 31 - 1) throw new Error('ACP timeoutMs must be a positive timer duration.');
  return AbortSignal.timeout(timeoutMs);
}

/** A text chunk notification with its text blanked: two chunks with the same value differ only in their text. */
function withoutText(notification: SessionNotification): string {
  const { update } = notification;
  return JSON.stringify(update.sessionUpdate === 'agent_message_chunk' && update.content.type === 'text' ? { ...notification, update: { ...update, content: { ...update.content, text: '' } } } : notification);
}

interface Turn {
  controller: AbortController;
  signal: AbortSignal;
  chunks: string[];
  updates: unknown[];
}

export class AcpBackend implements ExternalBackend {
  private connection?: ClientConnection;
  private readonly lifetime = new AbortController();
  private active?: Turn;
  private closing?: Promise<void>;
  private cancelling?: Promise<void>;
  private stderr = '';
  private constructor(
    private readonly child: ChildProcessWithoutNullStreams,
    readonly cwd: string,
    private currentSessionId: string,
    private readonly permission: AcpPermission,
  ) {
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (text: string) => { this.stderr += text; });
    child.once('error', (error) => { void this.shutdown(error); });
    child.once('exit', (code, signal) => { void this.shutdown(new Error(`ACP process exited (${signal ?? code}).`)); });
    child.stdin.on('error', (error) => { void this.shutdown(error); });
    child.stdout.on('error', (error) => { void this.shutdown(error); });
  }

  get sessionId(): string { return this.currentSessionId; }

  static async connect(input: AcpConnectInput, permission: AcpPermission, signal?: AbortSignal): Promise<AcpBackend> {
    const startup = AbortSignal.any([timeoutSignal(input.timeoutMs ?? 60_000), ...(signal ? [signal] : [])]);
    startup.throwIfAborted();
    const cwd = realpathSync(input.cwd);
    if (!statSync(cwd).isDirectory()) throw new Error('ACP cwd must be a directory.');
    if (!input.command.trim() || !input.vendor.trim()) throw new Error('ACP requires an executable and the real host vendor.');
    await untilAborted(prepareTree(), startup);
    startup.throwIfAborted();
    const child = spawn(input.command, input.args ?? [], { cwd, stdio: ['pipe', 'pipe', 'pipe'], detached: process.platform !== 'win32', windowsHide: true });
    adoptTree(child.pid);
    const backend = new AcpBackend(child, cwd, input.sessionId ?? '', permission);
    let phase = 'initialize';
    try {
      const stream = ndJsonStream(Writable.toWeb(child.stdin), Readable.toWeb(child.stdout));
      const connection = client({ name: 'octocode-pi-external' })
        .onRequest(methods.client.session.requestPermission, (ctx) => backend.requestPermission(ctx.params, ctx.signal))
        .onNotification(methods.client.session.update, (ctx) => backend.update(ctx.params))
        .connect(stream);
      backend.connection = connection;
      connection.signal.addEventListener('abort', () => { void backend.shutdown(new Error('ACP connection closed.')); }, { once: true });
      const initialization = await untilAborted(connection.agent.request(methods.agent.initialize, {
        protocolVersion: PROTOCOL_VERSION, clientCapabilities: {},
      }), AbortSignal.any([startup, backend.lifetime.signal]));
      if (initialization.protocolVersion !== PROTOCOL_VERSION) throw new Error(`Unsupported ACP protocol version ${initialization.protocolVersion}.`);
      if (input.sessionId) {
        phase = 'session/load';
        if (!initialization.agentCapabilities?.loadSession) throw new Error('ACP agent does not advertise session/load; existing session was not replaced.');
        await untilAborted(connection.agent.request(methods.agent.session.load, { sessionId: input.sessionId, cwd, mcpServers: [] }), startup);
      } else {
        phase = 'session/new';
        const session = await untilAborted(connection.agent.request(methods.agent.session.new, { cwd, mcpServers: [] }), startup);
        if (!session.sessionId) throw new Error('ACP agent returned an empty session ID.');
        backend.currentSessionId = session.sessionId;
      }
      startup.throwIfAborted();
      backend.lifetime.signal.throwIfAborted();
      return backend;
    } catch (error) {
      await backend.shutdown(error);
      throw new Error(`ACP connection failed during ${phase}: ${errorMessage(error)}`, { cause: error });
    }
  }

  async prompt(text: string, options: { timeoutMs: number; signal?: AbortSignal; key?: string }): Promise<ExternalReply> {
    if (this.active) throw new Error('ACP session already has an active prompt.');
    if (!this.connection || this.closing) throw new Error('ACP connection is closed; connect explicitly before sending another prompt.');
    const controller = new AbortController();
    const signal = AbortSignal.any([this.lifetime.signal, controller.signal, timeoutSignal(options.timeoutMs), ...(options.signal ? [options.signal] : [])]);
    signal.throwIfAborted();
    const turn: Turn = { controller, signal, chunks: [], updates: [] };
    this.active = turn;
    try {
      const response = await untilAborted(this.connection.agent.request(methods.agent.session.prompt, {
        sessionId: this.sessionId, prompt: [{ type: 'text', text }],
      }, { cancellationSignal: signal }), signal);
      turn.updates.push({ type: 'prompt_response', response });
      return { text: turn.chunks.join(''), stopReason: response.stopReason, updates: turn.updates };
    } catch (error) {
      await this.terminatePrompt(error);
      throw new AcpError(`ACP prompt failed: ${errorMessage(error)}`, {
        text: turn.chunks.join(''), updates: turn.updates,
      }, this.stderr, error);
    } finally {
      this.active = undefined;
    }
  }

  /**
   * Keeps every non-thought update of the active turn in order. Consecutive text chunks of the reply (an agent streams
   * it token by token) merge into one update, so the list carries the text once per run between other updates instead
   * of one JSON envelope per token; nothing is dropped.
   */
  private update(notification: SessionNotification): void {
    const turn = this.active;
    if (!turn || notification.sessionId !== this.sessionId || turn.signal.aborted || notification.update.sessionUpdate === 'agent_thought_chunk') return;
    const update = notification.update;
    if (update.sessionUpdate !== 'agent_message_chunk' || update.content.type !== 'text') {
      turn.updates.push(notification);
      return;
    }
    turn.chunks.push(update.content.text);
    const last = turn.updates.at(-1) as SessionNotification | undefined;
    const run = last?.update.sessionUpdate === 'agent_message_chunk' && last.update.content.type === 'text' ? last.update.content : undefined;
    // Only chunks that differ in nothing but their text merge, so no per-chunk field (an id, annotations, `_meta`) is lost.
    if (run && last && withoutText(last) === withoutText(notification)) run.text += update.content.text;
    else turn.updates.push({ ...notification, update: { ...update, content: { ...update.content } } });
  }

  private async requestPermission(request: RequestPermissionRequest, requestSignal: AbortSignal): Promise<RequestPermissionResponse> {
    const cancelled: RequestPermissionResponse = { outcome: { outcome: 'cancelled' } };
    const turn = this.active;
    if (!turn || request.sessionId !== this.sessionId || turn.signal.aborted) return cancelled;
    const signal = AbortSignal.any([requestSignal, turn.signal]);
    try {
      signal.throwIfAborted();
      const response = await untilAborted(this.permission(request, signal), signal);
      signal.throwIfAborted();
      const outcome = response.outcome;
      if (outcome.outcome === 'selected' && !request.options.some((option) => option.optionId === outcome.optionId)) {
        throw new Error('ACP permission callback selected an unadvertised option.');
      }
      return response;
    } catch (error) {
      if (!signal.aborted) void this.shutdown(error);
      return cancelled;
    }
  }

  async cancel(): Promise<void> {
    if (this.active) await this.terminatePrompt(new Error('ACP prompt cancelled'));
  }

  private terminatePrompt(reason: unknown): Promise<void> {
    if (this.cancelling) return this.cancelling;
    this.active?.controller.abort(reason);
    this.cancelling = Promise.resolve().then(async () => {
      try {
        if (this.connection && !this.connection.signal.aborted) {
          await untilAborted(this.connection.agent.notify(methods.agent.session.cancel, { sessionId: this.sessionId }), timeoutSignal(KILL_GRACE_MS));
        }
      } catch { /* Closing the owned process is authoritative when cooperative cancellation cannot be sent. */ }
      await this.shutdown(reason);
    });
    return this.cancelling;
  }

  close(): Promise<void> { return this.shutdown(new Error('ACP connection closed')); }

  private shutdown(reason: unknown): Promise<void> {
    if (this.closing) return this.closing;
    this.lifetime.abort(reason);
    this.closing = Promise.resolve().then(async () => {
      this.connection?.close(reason);
      await stopTree(this.child.pid);
      this.child.stdin.destroy();
      this.child.stdout.destroy();
      this.child.stderr.destroy();
    });
    return this.closing;
  }
}
