import http from 'node:http';
import net from 'node:net';
import { MAX_FRAME_BYTES, type ApiEvent, type RpcResponse } from './protocol.js';
import type { InstanceRecord } from './registry.js';

/** A failed call, carrying the JSON-RPC code. */
export class ApiCallError extends Error {
  constructor(
    message: string,
    readonly code: number,
    readonly data?: unknown,
  ) {
    super(message);
  }
}

const unwrap = (response: RpcResponse): unknown => {
  if ('error' in response) throw new ApiCallError(response.error.message, response.error.code, response.error.data);
  return response.result;
};

/** A `tool.start`/`tool.end` for a call another tool made (it carries `parentId`). */
export const isNestedToolEvent = (event: ApiEvent): boolean => (event.type === 'tool.start' || event.type === 'tool.end') && typeof event.data['parentId'] === 'string';

/**
 * A client for one instance: prefers the Unix socket, falls back to loopback HTTP. Reference implementation for
 * integrations, and what the `octocode-pi-api` command and the tests use.
 */
export class ApiClient {
  private next = 0;

  constructor(private readonly instance: Pick<InstanceRecord, 'socket' | 'http'>) {}

  async call<T = unknown>(method: string, params: Record<string, unknown> = {}): Promise<T> {
    const id = ++this.next;
    const request = { jsonrpc: '2.0', id, method, params };
    const response = this.instance.socket ? await this.overSocket(request) : await this.overHttp(request);
    return unwrap(response) as T;
  }

  private overSocket(request: object): Promise<RpcResponse> {
    return new Promise((resolve, reject) => {
      const socket = net.connect(this.instance.socket!);
      let buffer = '';
      socket.setEncoding('utf8');
      socket.once('error', reject);
      socket.on('connect', () => socket.write(`${JSON.stringify(request)}\n`));
      socket.on('data', (chunk: string) => {
        buffer += chunk;
        const newline = buffer.indexOf('\n');
        if (newline < 0) return;
        socket.end();
        try {
          resolve(JSON.parse(buffer.slice(0, newline)) as RpcResponse);
        } catch (error) {
          reject(error);
        }
      });
    });
  }

  private overHttp(request: object): Promise<RpcResponse> {
    const target = this.instance.http;
    if (!target) return Promise.reject(new Error('The instance has neither a socket nor an HTTP endpoint'));
    return new Promise((resolve, reject) => {
      const req = http.request(`${target.url}/rpc`, { method: 'POST', headers: { authorization: `Bearer ${target.token}`, 'content-type': 'application/json' } }, (res) => {
        let body = '';
        res.setEncoding('utf8');
        res.on('data', (chunk: string) => (body += chunk));
        res.on('end', () => {
          try {
            resolve(JSON.parse(body) as RpcResponse);
          } catch (error) {
            reject(error);
          }
        });
      });
      req.on('error', reject);
      req.end(JSON.stringify(request));
    });
  }

  /**
   * Follow events until `signal` aborts. Uses one socket connection: subscribe, then read notifications.
   * `onSubscribed` runs once the subscription is live, with the current sequence and whether replay lost events.
   * `topLevel` drops tool events of nested calls (those with `parentId`), keeping the agent's own calls.
   */
  async watch(
    onEvent: (event: ApiEvent) => void,
    options: { since?: number; types?: string[]; topLevel?: boolean; signal?: AbortSignal; onSubscribed?: (info: { seq: number; gap: boolean }) => void } = {},
  ): Promise<void> {
    if (options.signal?.aborted) return;
    const { socket: file } = this.instance;
    if (!file) throw new Error('watch needs the socket transport (over HTTP, read GET /events)');
    await new Promise<void>((resolve, reject) => {
      const socket = net.connect(file);
      let buffer = '';
      socket.setEncoding('utf8');
      socket.once('error', reject);
      const abort = () => socket.destroy();
      socket.on('close', () => {
        options.signal?.removeEventListener('abort', abort);
        resolve();
      });
      options.signal?.addEventListener('abort', abort, { once: true });
      socket.on('connect', () => {
        const { since, types } = options;
        socket.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'events.subscribe', params: { ...(since !== undefined ? { since } : {}), ...(types ? { types } : {}) } })}\n`);
      });
      socket.on('data', (chunk: string) => {
        buffer += chunk;
        if (buffer.length > MAX_FRAME_BYTES * 4) return void socket.destroy();
        let newline: number;
        while ((newline = buffer.indexOf('\n')) >= 0) {
          const line = buffer.slice(0, newline);
          buffer = buffer.slice(newline + 1);
          try {
            const message = JSON.parse(line) as { id?: unknown; method?: string; params?: ApiEvent; result?: { seq: number; gap: boolean } };
            if (message.method === 'event' && message.params) {
              if (!(options.topLevel && isNestedToolEvent(message.params))) onEvent(message.params);
            }
            else if (message.id === 1 && message.result) options.onSubscribed?.(message.result);
          } catch (error) {
            socket.destroy();
            reject(error);
            return;
          }
        }
      });
    });
  }
}
