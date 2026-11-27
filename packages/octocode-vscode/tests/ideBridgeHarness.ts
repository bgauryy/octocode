import * as fs from 'fs/promises';
import * as http from 'http';
import * as os from 'os';
import * as path from 'path';
import { afterEach, beforeEach } from 'vitest';

import type { IdeBridge } from '../src/ideBridge';
import { host } from './ideBridgeMock';

export const posix = process.platform !== 'win32';
export const FILE = path.resolve('/work/src/main.ts');

/** Per-test state: a temp Octocode home, the output log and the bridge. */
export const ctx = {
  home: '',
  log: [] as string[],
  bridge: undefined as unknown as IdeBridge,
};

export type Reply = {
  status: number;
  headers: http.IncomingHttpHeaders;
  text: string;
  // eslint-disable-next-line typescript/no-explicit-any -- test payloads are checked by assertions
  json: any;
};

export type Lock = {
  version: number;
  pid: number;
  ide: string;
  port: number;
  token: string;
  workspaceFolders: string[];
};

export async function lock(): Promise<Lock> {
  return JSON.parse(
    await fs.readFile(path.join(ctx.home, 'ide', `${process.pid}.json`), 'utf8')
  );
}

export function send(
  port: number,
  options: {
    method?: string;
    url?: string;
    headers?: http.OutgoingHttpHeaders;
    body?: string | Buffer;
    chunks?: Buffer[];
  }
): Promise<Reply> {
  return new Promise((resolve, reject) => {
    const req = http.request(
      {
        host: '127.0.0.1',
        port,
        method: options.method ?? 'GET',
        path: options.url ?? '/health',
        headers: Object.fromEntries(
          Object.entries(options.headers ?? {}).filter(
            ([, value]) => value !== undefined
          )
        ),
        agent: false,
      },
      res => {
        const parts: Buffer[] = [];
        res.on('data', part => parts.push(part));
        res.on('end', () => {
          const text = Buffer.concat(parts).toString('utf8');
          resolve({
            status: res.statusCode!,
            headers: res.headers,
            text,
            json: text ? JSON.parse(text) : undefined,
          });
        });
      }
    );
    req.on('error', reject);
    for (const chunk of options.chunks ?? []) req.write(chunk);
    req.end(options.body);
  });
}

export async function authed(
  options: Parameters<typeof send>[1],
  overrides: http.OutgoingHttpHeaders = {}
): Promise<Reply> {
  const { port, token } = await lock();
  return send(port, {
    ...options,
    headers: {
      Host: `127.0.0.1:${port}`,
      Authorization: `Bearer ${token}`,
      ...options.headers,
      ...overrides,
    },
  });
}

export function lsp(body: unknown, overrides: http.OutgoingHttpHeaders = {}) {
  return authed(
    {
      method: 'POST',
      url: '/lsp',
      headers: { 'Content-Type': 'application/json' },
      body: typeof body === 'string' ? body : JSON.stringify(body),
    },
    overrides
  );
}

export const range = (a = 1, b = 2, c = 1, d = 5) => ({
  start: { line: a, character: b },
  end: { line: c, character: d },
});
export const at = (method: string, extra: object = {}) => ({
  method,
  path: FILE,
  line: 3,
  character: 7,
  ...extra,
});
export const vsUri = (p = '/work/src/other.ts') => ({
  scheme: 'file',
  fsPath: p,
  toString: () => `file://${p}`,
});

/** Registers a fresh temp home and an unstarted bridge for every test. */
export function useBridge(): void {
  beforeEach(async () => {
    const { IdeBridge: Bridge } = await import('../src/ideBridge');
    ctx.home = await fs.mkdtemp(path.join(os.tmpdir(), 'octocode-ide-bridge-'));
    ctx.log = [];
    host.execute.mockReset();
    host.diagnostics.mockReset().mockReturnValue([]);
    host.languages.mockReset().mockResolvedValue(['typescript', 'plaintext']);
    host.open
      .mockReset()
      .mockImplementation(async (uri: { fsPath: string }) => ({
        uri,
        languageId: 'typescript',
      }));
    host.folders = [{ uri: { scheme: 'file', fsPath: '/work' } }];
    host.foldersDispose.mockReset();
    host.configDispose.mockReset();
    host.appName = 'Test IDE';
    host.enabled = true;
    ctx.bridge = new Bridge({
      home: ctx.home,
      output: line => ctx.log.push(line),
    });
  });

  afterEach(async () => {
    await ctx.bridge.stop();
    await fs.rm(ctx.home, { recursive: true, force: true });
  });
}
