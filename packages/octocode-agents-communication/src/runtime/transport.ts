/** Local native adapters. Transport acceptance never means model acknowledgement. */
import { Agent, request as httpRequest } from 'node:http';
import type { CommandResults, HostInboundData, RecordUsageInput } from './catalog-types.generated.js';
import { ValidationError } from './errors.js';
import { dumps, encode, parseJsonBytes } from './json-text.js';
import { type Channel, SOCKET_IO_MS, isOneOf, loopback, object, socketPath, unix, validate, validateSession } from './native-io.js';
import { realpath } from './paths.js';
import { codePointLength } from './text.js';
import { monotonic } from './timing.js';
import { Codex } from './transport-codex.js';
import { isObject, isUuid, objectOrEmpty, type JsonObject } from './validation.js';
import { VERSION } from './version.js';
import { MAX_FRAME, refusal } from './wire.js';

const RECEIPTS: Record<string, string> = { claude: 'socket-write-only', codex: 'jsonrpc', grok: 'acp-turn-completion', opencode: 'http' };

/** The receipt a native offer produced. Acceptance only: the recipient's own `complete` is the acknowledgement. */
export interface Receipt {
  kind: string;
  turnRequested: boolean;
  stopReason?: string;
  usage?: RecordUsageInput;
}

/** The token counters of a `record_usage` input. */
export type UsageCounter = Exclude<keyof RecordUsageInput, 'key' | 'scope' | 'model'>;

const HTTP_CONNECT_MS = 2000;
const HTTP_EXCHANGE_MS = 5000;
const GROK_REGISTRATION_MS = 10000;
const GROK_PROMPT_MS = 300000;
const GROK_POLL_MS = 1;
const GROK_REQUEST_POLL_MS = 5;
const HTTP_LIMIT = 1024 * 1024;

export function requiresAction(mode: string): boolean {
  return mode === 'claude' || mode === 'grok';
}

export function capabilities(mode: string): CommandResults['attach']['capabilities'] {
  if (!(mode in RECEIPTS)) return { nativeInput: false, acceptanceReceipt: 'host-confirmation', readReceipt: false };
  return {
    nativeInput: true, passiveInjection: !requiresAction(mode), actionWake: true, acceptanceReceipt: RECEIPTS[mode]!, readReceipt: false,
    ...(mode === 'codex' || mode === 'opencode' ? { wakePrerequisite: mode === 'codex' ? 'loaded-thread' : 'existing-idle-session-in-workspace' } : {}),
    ...(mode === 'codex' ? { activeInjection: true } : {}),
  };
}


const CLAUDE_PROMPTING_MODES = ['default', 'acceptEdits', 'auto', 'dontAsk'];

/** Claude Code verifies a sender that presents the receiver's exported session token. */
export function claudeOwnChild(endpoint: string): boolean {
  return process.env.CLAUDE_CODE_MESSAGING_SOCKET === socketPath(endpoint) && 'CLAUDE_CODE_MESSAGING_TOKEN' in process.env;
}

/**
 * Predict Claude Code's documented inbound decision (delivered, held, refused, unknown) from the receiver's
 * hook-reported permission mode and crossSessionInbound setting. A --settings flag on the receiver is invisible to
 * hooks, so 'accept' is never certain.
 */
export function claudeInbound(state: Pick<HostInboundData, 'permissionMode' | 'crossSessionInbound'> | null | undefined, ownChild: boolean): [string, string] {
  if (!state) return ['unknown', 'Receiver has not reported its permission mode through hooks'];
  const setting = state.crossSessionInbound;
  if (setting === 'refuse') return ['refused', 'Receiver crossSessionInbound is refuse; Claude Code drops peer messages'];
  if (setting === 'hold') return ['held', 'Receiver crossSessionInbound is hold; Claude Code shows a notice and does not deliver'];
  if (setting === 'accept') return ['delivered', 'Receiver crossSessionInbound is accept'];
  if (setting !== undefined && setting !== null) return ['unknown', 'Receiver crossSessionInbound value is not recognized'];
  if (ownChild) return ['delivered', "Sender presents the receiver's session token (verified own child)"];
  const mode = state.permissionMode;
  if (mode === 'bypassPermissions') return ['held', 'Receiver bypasses permission prompts; Claude Code holds peer messages for approval and drops them after dialogExpiry'];
  if (CLAUDE_PROMPTING_MODES.includes(mode)) return ['delivered', 'Receiver prompts for permissions; Claude Code delivers peer messages'];
  return ['unknown', 'Receiver permission mode does not determine the inbound decision'];
}

/** Writes one peer message to Claude Code's messaging socket and closes it; the receipt is the completed write only. */
export async function claude(endpoint: string, session: string, token: string, content: string): Promise<void> {
  const channel = await unix(socketPath(endpoint), 'Claude socket closed');
  try {
    const deadline = monotonic() + SOCKET_IO_MS;
    const frames: Buffer[] = [];
    const line = (value: unknown): Buffer => Buffer.concat([encode(value), Buffer.from('\n')]);
    if (claudeOwnChild(endpoint)) frames.push(line({ type: 'auth', token: process.env.CLAUDE_CODE_MESSAGING_TOKEN }));
    frames.push(line({ type: 'user', session_id: session, from: 'octocode-communication', priority: 'next', uuid: token, msg_id: token, message: { role: 'user', content } }));
    await channel.write(Buffer.concat(frames), deadline, 'Native I/O timed out');
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('Native I/O timed out')), Math.max(0, deadline - monotonic()));
      channel.socket.end(() => {
        clearTimeout(timer);
        resolve();
      });
    });
  } finally {
    channel.close();
  }
}

/** Percent-encoding with `+` for spaces (form encoding). */
function quotePlus(value: string): string {
  return encodeURIComponent(value).replace(/[!'()*]/g, character => '%' + character.charCodeAt(0).toString(16).toUpperCase()).replaceAll('%20', '+');
}

class OpenCode {
  private readonly host: string;
  private readonly port: number;
  private readonly headers: Record<string, string> = {};
  private readonly agent = new Agent({ keepAlive: true, maxSockets: 1 });

  constructor(endpoint: string, private readonly session: string, private readonly workspace: string) {
    validate('opencode', endpoint);
    validateSession(session);
    ({ host: this.host, port: this.port } = loopback(endpoint, 'http', false));
    const password = process.env.OPENCODE_SERVER_PASSWORD ?? '';
    if (password) {
      if (process.env.OCTOCODE_OPENCODE_AUTH_ENDPOINT !== endpoint) throw new ValidationError('Set OCTOCODE_OPENCODE_AUTH_ENDPOINT to the exact attached endpoint before using OPENCODE_SERVER_PASSWORD');
      const username = process.env.OPENCODE_SERVER_USERNAME ?? 'opencode';
      const credentials = username + password;
      const control = [...credentials].some(character => {
        const code = character.codePointAt(0)!;
        return code < 32 || (code >= 127 && code <= 159);
      });
      if (!username || username.includes(':') || control || Buffer.byteLength(credentials) > 8192) throw new ValidationError('Invalid OpenCode authentication configuration');
      this.headers.Authorization = 'Basic ' + Buffer.from(`${username}:${password}`).toString('base64');
    }
  }

  request(method: string, path: string, body?: unknown): Promise<{ status: number; content: Buffer }> {
    const data = body === undefined ? undefined : encode(body);
    const headers: Record<string, string | number> = { ...this.headers };
    if (data) {
      headers['Content-Type'] = 'application/json';
      headers['Content-Length'] = data.length;
    }
    return new Promise((resolve, reject) => {
      const chunks: Buffer[] = [];
      let size = 0;
      let finished = false;
      const finish = (error: Error | null, value?: { status: number; content: Buffer }): void => {
        if (finished) return;
        finished = true;
        clearTimeout(timer);
        if (error) {
          request.destroy();
          reject(error);
        } else resolve(value!);
      };
      // One deadline bounds the whole exchange, including a peer that trickles headers or body.
      const timer = setTimeout(() => finish(new Error('Native I/O timed out')), HTTP_EXCHANGE_MS);
      const request = httpRequest({ agent: this.agent, host: this.host, port: this.port, method, path: `${path}?directory=${quotePlus(this.workspace)}`, headers }, response => {
        let headerBytes = 0;
        for (let index = 0; index + 1 < response.rawHeaders.length; index += 2) headerBytes += response.rawHeaders[index]!.length + response.rawHeaders[index + 1]!.length + 4;
        if (headerBytes > 16384) return finish(new ValidationError('OpenCode response headers too large'));
        response.on('data', (chunk: Buffer) => {
          size += chunk.length;
          if (size > HTTP_LIMIT) return finish(new ValidationError('OpenCode response exceeds 1 MiB'));
          chunks.push(chunk);
        });
        response.on('end', () => finish(null, { status: response.statusCode ?? 0, content: Buffer.concat(chunks) }));
        response.on('error', error => finish(error));
        response.on('close', () => finish(new Error('OpenCode connection closed before the response completed')));
      });
      request.on('socket', socket => {
        if (!socket.connecting) return;
        const connectTimer = setTimeout(() => finish(new Error('Native I/O timed out')), HTTP_CONNECT_MS);
        socket.once('connect', () => clearTimeout(connectTimer));
        socket.once('close', () => clearTimeout(connectTimer));
      });
      request.on('error', error => finish(error));
      request.end(data);
    });
  }

  async get(path: string): Promise<unknown> {
    const { status, content } = await this.request('GET', path);
    if (status !== 200) throw new Error(`OpenCode preflight returned HTTP ${status}; no message submitted`);
    return parseJsonBytes(content);
  }

  async idle(): Promise<boolean> {
    const session = object(await this.get('/session/' + this.session), 'OpenCode session');
    if (session.id !== this.session || typeof session.directory !== 'string' || !session.directory || realpath(session.directory, true) !== this.workspace) throw new ValidationError('OpenCode session/workspace mismatch; no message submitted');
    const statuses = await this.get('/session/status');
    if (!isObject(statuses)) throw new ValidationError('Invalid OpenCode status map; no message submitted');
    // An absent entry means idle; a present entry that is not an object is a malformed map, not idle.
    const entry = Object.hasOwn(statuses, this.session) ? statuses[this.session] : { type: 'idle' };
    if (!isObject(entry)) throw new ValidationError('Invalid OpenCode status entry; no message submitted');
    const status = entry.type;
    if (!isOneOf(status, ['idle', 'busy', 'retry'])) throw new ValidationError('Unknown OpenCode runtime status; no message submitted');
    return status === 'idle';
  }

  async submit(content: string, action: boolean): Promise<void> {
    const { status, content: body } = await this.request('POST', `/session/${this.session}/${action ? 'prompt_async' : 'message'}`, { noReply: !action, parts: [{ type: 'text', text: content }] });
    if (status !== (action ? 204 : 200)) throw new Error(`OpenCode submission returned HTTP ${status}; inspect before retrying`);
    if (!action) {
      const receipt = object(parseJsonBytes(body), 'OpenCode receipt');
      const info = objectOrEmpty(receipt.info);
      const parts: unknown[] = Array.isArray(receipt.parts) ? receipt.parts : [];
      if (info.sessionID !== this.session || info.role !== 'user' || !String(info.id ?? '').startsWith('msg_') || !parts.some(part => isObject(part) && part.type === 'text' && part.text === content)) throw new ValidationError('OpenCode receipt did not match submitted message/session; inspect before retrying');
    }
  }

  close(): void {
    this.agent.destroy();
  }
}

class Grok {
  private sequence = 0;
  private pending: { identity: number; deadline: number } | null = null;

  private constructor(private readonly channel: Channel, private readonly session: string) {}

  static async open(endpoint: string, session: string, workspace: string): Promise<Grok> {
    validate('grok', endpoint);
    if (!isUuid(session)) throw new ValidationError('badly formed hexadecimal UUID string');
    const grok = new Grok(await unix(socketPath(endpoint), 'Grok leader closed; delivery may have succeeded; inspect before retrying'), session);
    try {
      await grok.register(workspace);
    } catch (error) {
      grok.close();
      throw error;
    }
    return grok;
  }

  private async register(workspace: string): Promise<void> {
    const deadline = monotonic() + GROK_REGISTRATION_MS;
    await this.sendFrame({ type: 'register', client_type: 'octocode-communication', mode: 'stdio', capabilities: {} });
    let registered = false;
    for (;;) {
      if (monotonic() >= deadline) throw new Error('Grok leader registration timed out');
      const frame = await this.frame(GROK_REQUEST_POLL_MS);
      if (frame === null) continue;
      if (frame.type === 'registered') {
        if (frame.leader_protocol_version !== 1) throw new ValidationError('Unsupported Grok leader protocol; version 1 is required');
        registered = true;
        if (frame.ready !== false) break;
      } else if (frame.type === 'leader_ready' && registered) break;
      else throw new ValidationError('Unexpected Grok leader registration response');
    }
    const result = object(await this.request('initialize', { protocolVersion: 1, clientInfo: { name: 'octocode-communication', version: VERSION }, clientCapabilities: {} }, deadline), 'Grok initialize result');
    if (result.protocolVersion !== 1) throw new ValidationError('Unsupported Grok ACP protocol; version 1 is required');
    const info = objectOrEmpty(object(await this.request('_x.ai/session/info', { sessionId: this.session }, deadline), 'Grok session info').result);
    if (info.sessionId !== this.session || typeof info.cwd !== 'string' || !info.cwd || realpath(info.cwd, true) !== workspace) throw new ValidationError('Grok session metadata does not match identity/workspace; no message staged');
  }

  private async sendFrame(value: unknown): Promise<void> {
    const data = encode(value);
    if (data.length > MAX_FRAME) throw new ValidationError('Grok frame exceeds 8 MiB');
    const header = Buffer.alloc(4);
    header.writeUInt32BE(data.length);
    await this.channel.write(Buffer.concat([header, data]), monotonic() + SOCKET_IO_MS, 'Native I/O timed out');
  }

  private sendRpc(value: unknown): Promise<void> {
    return this.sendFrame({ type: 'acp', payload: encode(value).toString('utf8') });
  }

  /** The next complete length-prefixed frame, or null when none arrives within `waitMs`. */
  private async frame(waitMs: number): Promise<JsonObject | null> {
    for (;;) {
      if (this.channel.length >= 4) {
        const size = this.channel.peek(4).readUInt32BE();
        if (size > MAX_FRAME) throw new ValidationError('Grok frame exceeds 8 MiB');
        if (this.channel.length >= size + 4) return object(parseJsonBytes(this.channel.take(size + 4).subarray(4)), 'Grok frame');
      }
      if (!(await this.channel.fill(monotonic() + waitMs))) return null;
    }
  }

  private async response(identity: number, waitMs: number): Promise<unknown> {
    for (let count = 0; count < 64; count += 1) {
      const frame = await this.frame(waitMs);
      if (frame === null) return null;
      if (frame.type === 'acp') {
        const rpc = object(JSON.parse(String(frame.payload)), 'Grok ACP payload');
        if ('method' in rpc && 'id' in rpc) await this.sendRpc({ ...refusal(rpc.id), jsonrpc: '2.0' });
        else if (rpc.id === identity) {
          if ('error' in rpc) throw new Error('Grok ACP request failed: ' + dumps(rpc.error));
          if (!('result' in rpc)) throw new ValidationError('Grok ACP response lacks result');
          return rpc.result;
        }
      } else if (frame.type !== 'pong') throw new ValidationError('Unsupported Grok leader frame');
    }
    return null;
  }

  private async request(method: string, params: JsonObject, deadline: number): Promise<unknown> {
    this.sequence += 1;
    const identity = this.sequence;
    await this.sendRpc({ jsonrpc: '2.0', id: identity, method, params });
    while (monotonic() < deadline) {
      const result = await this.response(identity, GROK_REQUEST_POLL_MS);
      if (result !== null && result !== undefined) return result;
    }
    throw new Error(`Grok ${method} timed out`);
  }

  async submit(content: string, token: string): Promise<void> {
    if (this.pending) throw new Error('Grok already has an in-flight delivery');
    this.sequence += 1;
    this.pending = { identity: this.sequence, deadline: monotonic() + GROK_PROMPT_MS };
    await this.sendRpc({ jsonrpc: '2.0', id: this.sequence, method: 'session/prompt', params: { sessionId: this.session, prompt: [{ type: 'text', text: content }], _meta: { verbatim: true, promptId: token } } });
  }

  /** The turn's stop reason and its `_meta`, once the prompt response arrives. */
  async poll(): Promise<{ stopReason: string; meta: JsonObject } | null> {
    if (!this.pending) return null;
    const { identity, deadline } = this.pending;
    const value = await this.response(identity, GROK_POLL_MS);
    if (value !== null && value !== undefined) {
      if (!isObject(value) || typeof value.stopReason !== 'string') throw new ValidationError('Grok prompt response lacks stopReason');
      this.pending = null;
      return { stopReason: value.stopReason, meta: objectOrEmpty(value._meta) };
    }
    if (monotonic() >= deadline) throw new Error('Grok prompt timed out; delivery may have succeeded; inspect before retrying');
    return null;
  }

  close(): void {
    this.channel.close();
  }
}

const USAGE_SOURCES: [UsageCounter, string][] = [['inputTokens', 'inputTokens'], ['outputTokens', 'outputTokens'], ['cachedInputTokens', 'cachedReadTokens'], ['cacheWriteTokens', 'cacheCreationTokens']];

type Backend = Codex | Grok | OpenCode | null;

/** One native receiver: its adapter, binding identity and receipt semantics. */
export class NativeDelivery {
  private constructor(readonly mode: string, readonly endpoint: string | undefined, readonly session: string | undefined, readonly workspace: string, private readonly backend: Backend) {}

  static async open(mode: string, endpoint: string | undefined, session: string | undefined, workspace: string): Promise<NativeDelivery> {
    if (!(mode in RECEIPTS)) throw new ValidationError('Unknown native transport: ' + mode);
    const backend: Backend = mode === 'codex' ? await Codex.open(endpoint!) : mode === 'grok' ? await Grok.open(endpoint!, session!, workspace) : mode === 'opencode' ? new OpenCode(endpoint!, session!, workspace) : null;
    return new NativeDelivery(mode, endpoint, session, workspace, backend);
  }

  matches(mode: string, endpoint: string | undefined, session: string | undefined, workspace: string): boolean {
    return this.mode === mode && this.endpoint === endpoint && this.session === session && this.workspace === workspace;
  }

  async prepare(): Promise<boolean> {
    if (this.backend instanceof Codex) return this.backend.ready(this.session!, this.workspace);
    if (this.backend instanceof OpenCode) return this.backend.idle();
    return true;
  }

  /** Offers one batch; resolves to the acceptance receipt, or null while Grok's turn completion is awaited through `poll`. */
  async offer(content: string, token: string, action: boolean): Promise<Receipt | null> {
    if (requiresAction(this.mode) && !action) throw new ValidationError(this.mode + ' cannot inject passive-only context');
    if (this.mode === 'claude') await claude(this.endpoint!, this.session!, token, content);
    else if (this.backend instanceof Codex) await (action ? this.backend.startTurn(this.session!, content) : this.backend.inject(this.session!, content));
    else if (this.backend instanceof OpenCode) await this.backend.submit(content, action);
    else {
      await (this.backend as Grok).submit(content, token);
      return null;
    }
    return { kind: RECEIPTS[this.mode]!, turnRequested: action };
  }

  async poll(token: string): Promise<Receipt | null> {
    if (!(this.backend instanceof Grok)) return null;
    const receipt = await this.backend.poll();
    if (receipt === null) return null;
    const result: Receipt = { kind: RECEIPTS[this.mode]!, turnRequested: true, stopReason: receipt.stopReason };
    const meta = receipt.meta;
    const record: RecordUsageInput = { key: 'grok-' + token, scope: 'turn' };
    for (const [field, source] of USAGE_SOURCES) {
      const value = objectOrEmpty(meta.usage)[source];
      if (typeof value === 'number' && Number.isInteger(value) && value >= 0) record[field] = value;
    }
    if (Object.keys(record).length > 2) {
      const model = meta.modelId;
      if (typeof model === 'string' && model.length > 0 && codePointLength(model) <= 256) record.model = model;
      result.usage = record;
    }
    return result;
  }

  close(): void {
    this.backend?.close();
  }
}
