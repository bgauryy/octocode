import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { request as httpRequest } from 'node:http';
import { describe, expect, it } from 'vitest';
import { binary, launcher, ready } from './native-fixture.js';

const entries = [
  { name: 'native', command: binary ?? '', args: [] as string[] },
  { name: 'Node', command: process.execPath, args: [launcher] },
];

function postWithHost(url: URL, host: string, token: string): Promise<number> {
  return new Promise((resolve, reject) => {
    const request = httpRequest(
      new URL('/api/request', url),
      {
        method: 'POST',
        headers: {
          Host: host,
          Origin: url.origin,
          'Content-Type': 'application/json',
          'X-Octocode-Session': token,
        },
      },
      response => {
        response.resume();
        response.once('end', () => resolve(response.statusCode ?? 0));
      }
    );
    request.on('error', reject);
    request.setTimeout(15_000, () =>
      request.destroy(new Error('HTTP deadline exceeded'))
    );
    request.end('{}');
  });
}

async function start(entry: (typeof entries)[number]) {
  const child = spawn(
    entry.command,
    [...entry.args, 'config', 'view', '--no-open', '--idle-timeout=30'],
    {
      stdio: ['ignore', 'pipe', 'pipe'],
      env: { ...process.env, PATH: '', NO_COLOR: '1' },
    }
  );
  const exited = once(child, 'exit');
  let stderr = '';
  child.stderr.setEncoding('utf8').on('data', chunk => {
    stderr += chunk;
  });
  let output = '';
  const url = await new Promise<URL>((resolve, reject) => {
    const timer = setTimeout(() => {
      child.kill('SIGKILL');
      reject(new Error(`Viewer did not start: ${stderr}`));
    }, 15_000);
    child.once('error', error => {
      clearTimeout(timer);
      reject(error);
    });
    child.once('exit', code => {
      clearTimeout(timer);
      reject(new Error(`Viewer exited ${code}: ${stderr}`));
    });
    child.stdout.setEncoding('utf8').on('data', chunk => {
      output += chunk;
      const match = output.match(/http:\/\/127\.0\.0\.1:\d+\/#[-\w]+/);
      if (match) {
        clearTimeout(timer);
        resolve(new URL(match[0]));
      }
    });
  });
  return {
    child,
    exited,
    url,
    async stop() {
      if (child.exitCode === null && child.signalCode === null)
        child.kill('SIGKILL');
      await exited;
    },
  };
}

describe.skipIf(!ready)('config viewer through both CLI entry points', () => {
  it.each(entries)(
    '$name serves an authenticated native session without Node on PATH',
    async entry => {
      const view = await start(entry);
      try {
        const post = (path: string, token: string, body: unknown = {}) =>
          fetch(view.url.origin + path, {
            method: 'POST',
            headers: {
              Origin: view.url.origin,
              'Content-Type': 'application/json',
              'X-Octocode-Session': token,
            },
            body: JSON.stringify(body),
          });
        const shell = await fetch(view.url.origin);
        expect(shell.status).toBe(200);
        expect(shell.headers.get('cache-control')).toBe('no-store');
        expect(await shell.text()).not.toContain(view.url.hash.slice(1));
        const bootstrap = view.url.hash.slice(1);
        const session = await post('/api/session', bootstrap);
        expect(session.status).toBe(200);
        const { token } = (await session.json()) as { token: string };
        expect(token).not.toBe(bootstrap);
        expect((await post('/api/session', bootstrap)).status).toBe(401);
        const inspect = await post('/api/request', token, {
          operation: 'inspect',
        });
        expect(inspect.status).toBe(200);
        const data = (await inspect.json()) as {
          settings: { key: string; type: string }[];
        };
        expect(data.settings).toContainEqual(
          expect.objectContaining({ key: 'lsp.prewarm', type: 'enum' })
        );
        const headers: Record<string, string> = {
          Origin: view.url.origin,
          'Content-Type': 'application/json',
          'X-Octocode-Session': token,
        };
        const request = (
          body: string,
          overrides: Record<string, string> = {}
        ) =>
          fetch(`${view.url.origin}/api/request`, {
            method: 'POST',
            headers: { ...headers, ...overrides },
            body,
          });
        for (const origin of ['null', 'https://other.example', '']) {
          expect((await request('{}', { Origin: origin })).status).toBe(403);
        }
        expect(await postWithHost(view.url, 'other.example', token)).toBe(403);
        expect(
          (await request('{}', { 'Sec-Fetch-Site': 'cross-site' })).status
        ).toBe(403);
        expect(
          (await request('{}', { 'X-Octocode-Session': bootstrap })).status
        ).toBe(401);
        expect(
          (await request('{}', { 'Content-Type': 'text/plain' })).status
        ).toBe(415);
        expect((await post('/api/request?extra=1', token)).status).toBe(404);
        expect(
          (await post('/api/request', token, { operation: 'exec' })).status
        ).toBe(400);
        for (const invalid of [
          '{',
          '[]',
          '{"__proto__":{}}',
          '{"constructor":{}}',
        ]) {
          expect((await request(invalid)).status).toBe(400);
        }
        expect(
          (
            await request(
              JSON.stringify({ operation: 'inspect', value: 'x'.repeat(65536) })
            )
          ).status
        ).toBe(413);
        const rejected = await post('/api/request', token, {
          operation: 'setEnv',
          key: 'OCTOCODE_VIEW_TEST_KEY',
          value: 'secret-for-rejected-edit',
          scope: 'home',
        });
        expect(rejected.status).toBe(400);
        expect(await rejected.text()).not.toContain('secret-for-rejected-edit');
        expect((await post('/api/close', token)).status).toBe(200);
        expect((await view.exited)[0]).toBe(0);
        await expect(fetch(view.url.origin)).rejects.toThrow();
      } finally {
        await view.stop();
      }
    }
  );

  it.skipIf(process.platform === 'win32').each(entries)(
    '$name closes its listener when only the entry process receives SIGTERM',
    async entry => {
      const view = await start(entry);
      try {
        view.child.kill('SIGTERM');
        expect((await view.exited)[0]).toBe(143);
        await expect(fetch(view.url.origin)).rejects.toThrow();
      } finally {
        await view.stop();
      }
    }
  );
});
