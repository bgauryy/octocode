/** The Codex app-server adapter: a strict RFC 6455 client over a local socket and its JSON-RPC thread operations. */
import { createHash, randomBytes } from 'node:crypto';
import { ValidationError } from './errors.js';
import { decodeUtf8, dumps, encode } from './json-text.js';
import { type Channel, EndOfStream, SOCKET_IO_MS, TCP_CONNECT_MS, UNIX_PREFIX, connect, isOneOf, loopback, object, unix, validate } from './native-io.js';
import { realpath } from './paths.js';
import { monotonic } from './timing.js';
import { isObject, objectOrEmpty, type JsonObject } from './validation.js';
import { VERSION } from './version.js';
import { refusal } from './wire.js';

export function codexPeerTurn(session: string, content: string): JsonObject {
  return { threadId: session, input: [], toolOutput: { name: 'octocode_peer_messages', output: content } };
}

export function codexPeerItems(session: string, content: string): JsonObject {
  return { threadId: session, items: [{ type: 'function_call_output', name: 'octocode_peer_messages', output: content }] };
}

const WEBSOCKET_GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11';
const WEBSOCKET_LIMIT = 1024 * 1024;

/** RFC 6455 client for local plaintext sockets, with strict bounded framing. */
class WebSocket {
  deadline: number;

  private constructor(private readonly channel: Channel) {
    this.deadline = monotonic() + SOCKET_IO_MS;
  }

  static async open(endpoint: string): Promise<WebSocket> {
    validate('codex', endpoint);
    const local = endpoint.startsWith(UNIX_PREFIX);
    const channel = local ? await unix(endpoint.slice(UNIX_PREFIX.length), 'Codex endpoint closed') : await connect(loopback(endpoint, 'ws', true), TCP_CONNECT_MS, 'Codex endpoint closed');
    const socket = new WebSocket(channel);
    try {
      await socket.upgrade(local ? 'localhost' : endpoint.replace(/^[^:]*:\/\//, '').replace(/\/$/, ''));
    } catch (error) {
      channel.close();
      throw error;
    }
    return socket;
  }

  private async upgrade(host: string): Promise<void> {
    const key = randomBytes(16).toString('base64');
    await this.write(Buffer.from(`GET / HTTP/1.1\r\nHost: ${host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: ${key}\r\nSec-WebSocket-Version: 13\r\n\r\n`));
    let end = -1;
    while ((end = this.channel.peek(this.channel.length).indexOf('\r\n\r\n')) < 0) {
      if (this.channel.length > 16384) throw new ValidationError('WebSocket response headers too large');
      await this.more();
    }
    const headers = this.channel.take(end + 4).subarray(0, end).toString('latin1');
    const lines = headers.split('\r\n');
    const fields: Record<string, string> = {};
    for (const line of lines.slice(1)) {
      const colon = line.indexOf(':');
      if (colon < 0) throw new ValidationError('Invalid WebSocket upgrade response');
      fields[line.slice(0, colon).toLowerCase()] = line.slice(colon + 1).trim();
    }
    const status = lines[0].split(/\s+/).filter(Boolean);
    const expected = createHash('sha1').update(key + WEBSOCKET_GUID).digest('base64');
    if (status.length < 2 || status[1] !== '101' || fields['sec-websocket-accept'] !== expected || (fields.upgrade ?? '').toLowerCase() !== 'websocket' || !(fields.connection ?? '').toLowerCase().includes('upgrade')) throw new ValidationError('Invalid WebSocket upgrade response');
  }

  private async more(): Promise<void> {
    if (!(await this.channel.fill(this.deadline))) throw new Error('Codex I/O timed out');
  }

  private async exact(size: number): Promise<Buffer> {
    while (this.channel.length < size) await this.more();
    return this.channel.take(size);
  }

  private write(data: Uint8Array): Promise<void> {
    return this.channel.write(data, this.deadline, 'Codex I/O timed out');
  }

  private async sendFrame(opcode: number, payload: Buffer): Promise<void> {
    if (payload.length > WEBSOCKET_LIMIT) throw new ValidationError('Codex frame exceeds 1 MiB');
    const mask = randomBytes(4);
    const size = payload.length;
    const header = size < 126 ? Buffer.from([0x80 | opcode, 0x80 | size]) : size <= 65535 ? Buffer.from([0x80 | opcode, 0xfe, size >> 8, size & 255]) : Buffer.concat([Buffer.from([0x80 | opcode, 0xff]), Buffer.alloc(8)]);
    if (size > 65535) header.writeBigUInt64BE(BigInt(size), 2);
    const masked = Buffer.alloc(size);
    for (let index = 0; index < size; index += 1) masked[index] = payload[index] ^ mask[index % 4];
    await this.write(Buffer.concat([header, mask, masked]));
  }

  send(value: unknown): Promise<void> {
    return this.sendFrame(1, encode(value));
  }

  /** The next frame header: opcode, FIN bit and payload size, checked against the 1 MiB message bound (`total` bytes already assembled). */
  private async header(total: number): Promise<{ opcode: number; final: boolean; size: number }> {
    const bytes = await this.exact(2);
    const first = bytes[0];
    const second = bytes[1];
    const opcode = first & 15;
    const final = Boolean(first & 128);
    if (first & 112 || second & 128) throw new ValidationError('Invalid WebSocket server frame');
    let size = second & 127;
    if (size === 126) size = (await this.exact(2)).readUInt16BE();
    else if (size === 127) {
      const wide = (await this.exact(8)).readBigUInt64BE();
      if (wide > BigInt(WEBSOCKET_LIMIT)) throw new ValidationError('Codex frame exceeds 1 MiB');
      size = Number(wide);
    }
    if (size > WEBSOCKET_LIMIT || total + size > WEBSOCKET_LIMIT) throw new ValidationError('Codex frame exceeds 1 MiB');
    if (opcode >= 8 && (!final || size > 125)) throw new ValidationError('Invalid WebSocket control frame');
    return { opcode, final, size };
  }

  async read(): Promise<unknown> {
    const chunks: Buffer[] = [];
    let total = 0;
    let active: number | null = null;
    for (;;) {
      const { opcode, final, size } = await this.header(total);
      const payload = await this.exact(size);
      if (opcode === 8) throw new EndOfStream('Codex endpoint closed');
      if (opcode === 9) {
        await this.sendFrame(10, payload);
        continue;
      }
      if (opcode === 10) continue;
      if (opcode === 1 || opcode === 2) {
        if (active !== null) throw new ValidationError('Unexpected WebSocket message start');
        active = opcode;
      } else if (opcode !== 0 || active === null) throw new ValidationError('Unexpected WebSocket continuation');
      chunks.push(payload);
      total += size;
      if (final) {
        if (active === 1) return JSON.parse(decodeUtf8(Buffer.concat(chunks)));
        chunks.length = 0;
        total = 0;
        active = null;
      }
    }
  }

  close(): void {
    this.channel.close();
  }
}

export class Codex {
  private sequence = 0;

  private constructor(private readonly socket: WebSocket) {}

  static async open(endpoint: string): Promise<Codex> {
    const codex = new Codex(await WebSocket.open(endpoint));
    try {
      await codex.request('initialize', { clientInfo: { name: 'octocode-communication-dispatch', version: VERSION }, capabilities: { experimentalApi: true } });
      await codex.socket.send({ method: 'initialized', params: {} });
    } catch (error) {
      codex.close();
      throw error;
    }
    return codex;
  }

  async request(method: string, params: JsonObject): Promise<JsonObject | null> {
    this.sequence += 1;
    const identity = this.sequence;
    this.socket.deadline = monotonic() + SOCKET_IO_MS;
    await this.socket.send({ id: identity, method, params });
    for (;;) {
      const value = object(await this.socket.read(), 'Codex frame');
      if (value.id === identity && !('method' in value)) {
        if (value.error !== undefined && value.error !== null) throw new Error(`Codex ${method}: ${dumps(value.error)}`);
        return isObject(value.result) ? value.result : null;
      }
      if ('id' in value && 'method' in value) await this.socket.send(refusal(value.id));
    }
  }

  async ready(session: string, workspace: string): Promise<boolean> {
    const thread = objectOrEmpty((await this.request('thread/read', { threadId: session, includeTurns: false }))?.thread);
    if (thread.id !== session) throw new ValidationError('Codex thread/read returned a different or missing thread identity');
    if (typeof thread.cwd !== 'string' || !thread.cwd || realpath(thread.cwd, true) !== workspace) throw new ValidationError('Codex thread/read returned a different workspace');
    const status = objectOrEmpty(thread.status).type;
    if (!isOneOf(status, ['idle', 'active', 'notLoaded', 'systemError'])) throw new ValidationError('Codex thread/read returned an unknown runtime status');
    return status === 'idle' || status === 'active';
  }

  async startTurn(session: string, content: string): Promise<void> {
    const turn = objectOrEmpty((await this.request('turn/start', codexPeerTurn(session, content)))?.turn);
    if (!turn.id || !isOneOf(turn.status, ['inProgress', 'completed', 'failed', 'interrupted'])) throw new ValidationError('Codex turn/start returned no valid acceptance receipt; inspect before retrying');
  }

  async inject(session: string, content: string): Promise<void> {
    await this.request('thread/inject_items', codexPeerItems(session, content));
  }

  close(): void {
    this.socket.close();
  }
}
