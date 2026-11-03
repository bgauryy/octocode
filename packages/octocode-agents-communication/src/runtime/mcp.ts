/** Bounded JSON-RPC stdio server with optional owned agent presence. */
import { createHash, randomUUID } from 'node:crypto';
import type { Readable } from 'node:stream';
import * as catalog from './catalog.js';
import { Presence, sleep, stopEvent } from './loop-control.js';
import { ValidationError, describe } from './errors.js';
import { isObject, wellFormed, type JsonObject } from './validation.js';
import { VERSION } from './version.js';
import { MAX_FRAME } from './wire.js';
import { compact, compactAscii, parseJsonBytes } from './json-text.js';
import { McpBinding, initTool, type McpArgs } from './mcp-binding.js';

const ERRORS = { parse: -32700, request: -32600, method: -32601 } as const;
const IDLE_TICK_MS = 100;
const BLANK = /^[ \t\n\r\v\f]*$/;

export async function run(args: McpArgs, emit: (value: unknown) => void): Promise<void> {
  catalog.selectedTools(args.tools);
  const binding = new McpBinding(args);
  try {
    await binding.start();
    await serve(binding, args.tools, emit);
  } finally {
    await binding.close();
  }
}

type Frame = Buffer | Error;

/**
 * Newline-delimited frames from `stream`, at most one frame ahead of the consumer. A frame over the limit becomes one
 * ValidationError and its remaining bytes are discarded; an empty buffer marks end of input.
 */
class Frames {
  private readonly queue: Frame[] = [];
  private waiter: ((frame: Frame) => void) | null = null;
  private pending: Buffer[] = [];
  private size = 0;
  private oversized = false;

  constructor(private readonly stream: Readable) {
    stream.on('data', (chunk: Buffer) => this.read(chunk));
    stream.on('end', () => {
      if (this.size && !this.oversized) this.push(Buffer.concat(this.pending));
      this.push(Buffer.alloc(0));
    });
    stream.on('error', error => this.push(error));
  }

  private push(frame: Frame): void {
    if (this.waiter) {
      const waiter = this.waiter;
      this.waiter = null;
      waiter(frame);
    } else this.queue.push(frame);
  }

  private read(chunk: Buffer): void {
    let start = 0;
    while (start <= chunk.length) {
      const newline = chunk.indexOf(10, start);
      const complete = newline >= 0;
      const part = chunk.subarray(start, complete ? newline : chunk.length);
      if (!this.oversized) {
        if (this.size + part.length + Number(complete) > MAX_FRAME) {
          this.push(new ValidationError('JSON frame exceeds 8 MiB'));
          this.pending = [];
          this.size = 0;
          this.oversized = true;
        } else {
          this.pending.push(part);
          this.size += part.length;
          if (complete) {
            this.pending.push(Buffer.from([10]));
            this.push(Buffer.concat(this.pending));
            this.pending = [];
            this.size = 0;
          }
        }
      }
      if (!complete) break;
      this.oversized = false;
      start = newline + 1;
    }
    if (this.queue.length) this.stream.pause();
  }

  next(): Promise<Frame> {
    const frame = this.queue.shift();
    if (frame !== undefined) {
      if (!this.queue.length) this.stream.resume();
      return Promise.resolve(frame);
    }
    this.stream.resume();
    return new Promise(resolve => {
      this.waiter = resolve;
    });
  }

  close(): void {
    this.stream.destroy();
  }
}


/** A JSON-RPC request as `validRequest` accepts it. */
interface Request { jsonrpc: '2.0'; method: string; id?: number | string; params?: JsonObject }

function validRequest(request: unknown): request is Request {
  return isObject(request) && request.jsonrpc === '2.0' && typeof request.method === 'string'
    && (!('id' in request) || typeof request.id === 'number' || typeof request.id === 'string')
    && (!('params' in request) || isObject(request.params));
}

/** Dispatch one tool without letting validation failures terminate the connection. */
async function callTool(binding: McpBinding, selection: string | undefined, connection: string, request: Request, onInit: () => void) {
  const params = request.params ?? {};
  const name = 'name' in params ? params.name : '';
  let args: unknown = 'arguments' in params ? params.arguments : {};
  try {
    if (!wellFormed(args)) throw new ValidationError('Invalid tool arguments: lone surrogate in a string');
    if (!binding.session) {
      if (name !== 'init') throw new ValidationError('Call init before using communication tools');
      try {
        await binding.init(args);
        onInit();
      } catch (failure) {
        await binding.close();
        throw failure;
      }
      return { changed: true, result: { content: [{ type: 'text', text: compact({ ...binding.describe(), managed: binding.managed, instructions: binding.instructions() }) }] } };
    }
    if (typeof name !== 'string') throw new ValidationError(`Unknown tool: ${String(name)}`);
    args = catalog.keyedInput(name, args, 'mcp:' + createHash('sha256').update(`${connection}:${compact(request.id)}`).digest('hex'));
    const call = catalog.toolCommand(name, args, selection);
    const value = await binding.store!.call(binding.session, call.name, call.input);
    return { changed: false, result: { content: [{ type: 'text', text: compact(catalog.toolResult(call.name, value)) }] } };
  } catch (failure) {
    return { changed: false, result: { isError: true, content: [{ type: 'text', text: describe(failure) }] } };
  }
}

async function serve(binding: McpBinding, selection: string | undefined, emit: (value: unknown) => void): Promise<void> {
  const tools = catalog.selectedTools(selection);
  const connection = randomUUID();
  const canInitialize = !binding.session;
  const ready = (): Presence | null => {
    if (!binding.managed) return null;
    const presence = new Presence(() => binding.renew());
    presence.reset();
    process.stderr.write(compactAscii({ type: 'mcp_ready', session: binding.session, presence: 'managed', leases: 'managed', delivery: 'manual-inbox', automaticWake: false }) + '\n');
    return presence;
  };
  const stop = stopEvent();
  const frames = new Frames(process.stdin);
  const error = (id: unknown, code: number, message: string): void => emit({ jsonrpc: '2.0', id, error: { code, message } });
  let presence = ready();
  let next: Promise<Frame> | null = null;
  try {
    while (!stop.signal.aborted) {
      await presence?.tick();
      next ??= frames.next();
      const frame = await Promise.race([next, sleep(IDLE_TICK_MS, stop.signal).then(() => undefined)]);
      if (frame === undefined) continue;
      next = null;
      if (frame instanceof ValidationError) {
        error(null, ERRORS.request, frame.message);
        continue;
      }
      if (frame instanceof Error) throw frame;
      if (!frame.length) break;
      if (BLANK.test(frame.toString('latin1'))) continue;
      let request: unknown;
      try {
        request = parseJsonBytes(frame);
      } catch {
        error(null, ERRORS.parse, 'Invalid JSON');
        continue;
      }
      if (!validRequest(request)) {
        error(null, ERRORS.request, 'Invalid request');
        continue;
      }
      if (!('id' in request)) continue;
      const { id, method } = request;
      // JSON.parse rounds an integer beyond 2^53, so the reply would carry a different id than the request.
      if (typeof id === 'number' && Number.isInteger(id) && !Number.isSafeInteger(id)) {
        error(null, ERRORS.request, 'Invalid request: an integer id must be within ±(2^53−1) to be echoed exactly');
        continue;
      }
      let result: unknown;
      let changed = false;
      if (method === 'initialize') {
        result = {
          instructions: binding.instructions(),
          protocolVersion: '2024-11-05',
          capabilities: { tools: canInitialize ? { listChanged: true } : {} },
          serverInfo: { name: 'octocode-agents-communication', version: VERSION },
        };
      } else if (method === 'ping') result = {};
      else if (method === 'tools/list') result = { tools: binding.session ? tools : [initTool()] };
      else if (method === 'tools/call') {
        ({ result, changed } = await callTool(binding, selection, connection, request, () => { presence = ready(); }));
      } else {
        error(id, ERRORS.method, 'Method not found');
        continue;
      }
      emit({ jsonrpc: '2.0', id, result });
      if (changed) emit({ jsonrpc: '2.0', method: 'notifications/tools/list_changed' });
    }
  } finally {
    stop.dispose();
    frames.close();
  }
}
