import { describe, it, onTestFinished } from 'vitest';
import assert from 'node:assert/strict';
import { request as httpRequest } from 'node:http';
import {
  startConfigView,
  type ConfigViewOptions,
  type ConfigViewRequest,
} from '../../src/cli/config-view/server.js';

async function fixture(
  backend: ConfigViewRequest = async payload => ({
    operation: payload.operation,
    ok: true,
  }),
  options: Partial<ConfigViewOptions> = {}
) {
  const view = await startConfigView({ request: backend, ...options });
  onTestFinished(() => view.close());
  const bootstrap = view.url.split('#')[1]!;
  async function post(
    path: string,
    payload: unknown = {},
    credential = bootstrap,
    headers: Record<string, string> = {}
  ): Promise<Response> {
    return fetch(`${view.origin}${path}`, {
      method: 'POST',
      headers: {
        Origin: view.origin,
        'Content-Type': 'application/json',
        'X-Octocode-Session': credential,
        ...headers,
      },
      body: typeof payload === 'string' ? payload : JSON.stringify(payload),
    });
  }
  const authenticate = async (): Promise<string> => {
    const response = await post('/api/session');
    assert.equal(response.status, 200);
    return ((await response.json()) as { token: string }).token;
  };
  return { view, bootstrap, post, authenticate };
}

function rawPost(
  url: string,
  headers: Record<string, string>,
  write: (outgoing: ReturnType<typeof httpRequest>) => void
): Promise<number | undefined> {
  return new Promise((resolve, reject) => {
    const outgoing = httpRequest(url, { method: 'POST', headers }, response => {
      response.resume();
      resolve(response.statusCode);
    });
    outgoing.on('error', reject);
    write(outgoing);
  });
}

describe('config view server', () => {
  it('offline shell has strict headers and contains no session credential', async () => {
    const { view, bootstrap } = await fixture();
    const response = await fetch(view.origin);
    const html = await response.text();
    assert.equal(response.status, 200);
    assert.match(
      response.headers.get('content-security-policy')!,
      /frame-ancestors 'none'/
    );
    assert.match(
      response.headers.get('content-security-policy')!,
      /default-src 'none'/
    );
    assert.equal(response.headers.get('access-control-allow-origin'), null);
    assert.equal(response.headers.get('cache-control'), 'no-store');
    assert.equal(response.headers.get('referrer-policy'), 'no-referrer');
    assert.ok(!html.includes(bootstrap));
    assert.ok(!/https?:\/\//.test(html));
    assert.ok(!/onclick|<script[^>]*>[^<]+/.test(html));
    assert.match(
      await (await fetch(`${view.origin}/app.js`)).text(),
      /history.replaceState/
    );
    assert.match(
      (await fetch(`${view.origin}/style.css`)).headers.get('content-type')!,
      /text\/css/
    );
  });

  it('all reads require session, bootstrap is one use and distinct from session', async () => {
    let calls = 0;
    const { post, bootstrap, authenticate } = await fixture(async () => {
      calls++;
      return { settings: [] };
    });
    assert.equal(
      (await post('/api/request', { operation: 'inspect' })).status,
      401
    );
    const session = await authenticate();
    assert.notEqual(session, bootstrap);
    assert.equal((await post('/api/session')).status, 401);
    assert.equal(
      (await post('/api/request', { operation: 'inspect' }, bootstrap)).status,
      401
    );
    assert.equal(
      (await post('/api/request', { operation: 'inspect' }, session)).status,
      200
    );
    assert.equal(calls, 1);
  });

  it('foreign/null/missing origins, rebinding hosts and cross-site metadata are rejected', async () => {
    const { view, post, authenticate } = await fixture();
    const session = await authenticate();
    for (const Origin of [
      'null',
      'https://attacker.example',
      'http://localhost',
    ]) {
      assert.equal(
        (
          await post('/api/request', { operation: 'inspect' }, session, {
            Origin,
          })
        ).status,
        403
      );
    }
    const rebindingStatus = await rawPost(
      `${view.origin}/api/request`,
      {
        Host: 'attacker.example',
        Origin: view.origin,
        'Content-Type': 'application/json',
        'X-Octocode-Session': session,
      },
      outgoing => outgoing.end('{"operation":"inspect"}')
    );
    assert.equal(rebindingStatus, 403);
    assert.equal(
      (
        await post('/api/request', { operation: 'inspect' }, session, {
          'Sec-Fetch-Site': 'cross-site',
        })
      ).status,
      403
    );
    const response = await fetch(`${view.origin}/api/request`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Octocode-Session': session,
      },
      body: '{"operation":"inspect"}',
    });
    assert.equal(response.status, 403);
  });

  it('only exact API routes and allowlisted JSON operations are admitted', async () => {
    let calls = 0;
    const { view, post, authenticate } = await fixture(async () => {
      calls++;
      return undefined;
    });
    const session = await authenticate();
    assert.equal((await fetch(`${view.origin}/api/request`)).status, 405);
    assert.equal(
      (await post('/api/request?operation=setEnv', {}, session)).status,
      404
    );
    assert.equal(
      (
        await post(
          '/api/request',
          { operation: 'readFile', path: '/etc/passwd' },
          session
        )
      ).status,
      400
    );
    assert.equal(
      (
        await post(
          '/api/request',
          '{"operation":"inspect","__proto__":{}}',
          session
        )
      ).status,
      400
    );
    assert.equal(
      (
        await post('/api/request', { operation: 'inspect' }, session, {
          'Content-Type': 'application/x-www-form-urlencoded',
        })
      ).status,
      415
    );
    assert.equal(
      (await post('/api/request', 'invalid JSON', session)).status,
      400
    );
    assert.equal(calls, 0);
  });

  it('streams are bounded including chunked bodies', async () => {
    const { view, authenticate } = await fixture();
    const session = await authenticate();
    const status = await rawPost(
      `${view.origin}/api/request`,
      {
        Origin: view.origin,
        'Content-Type': 'application/json',
        'X-Octocode-Session': session,
      },
      outgoing => {
        outgoing.write('{"operation":"setEnv","value":"');
        outgoing.write('x'.repeat(70000));
        outgoing.end('"}');
      }
    );
    assert.equal(status, 413);
  });

  it('secret replacement is delivered only to backend; unsafe errors omit values', async () => {
    const key = 'synthetic_secret_do_not_print_123';
    let saved: Record<string, unknown> | undefined;
    const { post, authenticate } = await fixture(async payload => {
      saved = payload;
      if (payload.operation === 'setEnv')
        throw new Error(`Failed writing ${String(payload.value)}`);
      return { keys: [{ key: 'CUSTOM_KEY', set: true }] };
    });
    const session = await authenticate();
    const response = await post(
      '/api/request',
      {
        operation: 'setEnv',
        key: 'CUSTOM_KEY',
        value: key,
        scope: 'home',
        revision: 'r1',
      },
      session
    );
    assert.equal(saved?.value, key);
    assert.ok(!(await response.text()).includes(key));
    const inspect = await post(
      '/api/request',
      { operation: 'inspect' },
      session
    );
    assert.deepEqual(await inspect.json(), {
      keys: [{ key: 'CUSTOM_KEY', set: true }],
    });
  });

  it('safe native failures retain actionable text and revision conflict status', async () => {
    const { post, authenticate } = await fixture(async () => {
      throw Object.assign(new Error('Refresh the changed file.'), {
        code: 'CONFLICT',
        safe: true,
      });
    });
    const session = await authenticate();
    const response = await post(
      '/api/request',
      {
        operation: 'setSetting',
        key: 'output.format',
        value: 'json',
        revision: 'old',
      },
      session
    );
    assert.equal(response.status, 409);
    assert.deepEqual(await response.json(), {
      error: { code: 'CONFLICT', message: 'Refresh the changed file.' },
    });
  });

  it('bounded admission rejects a fifth simultaneous backend operation', async () => {
    let release!: () => void;
    let count = 0;
    let allStarted!: () => void;
    const started = new Promise<void>(resolve => {
      allStarted = resolve;
    });
    const blocked = new Promise<void>(resolve => {
      release = resolve;
    });
    const { post, authenticate } = await fixture(async () => {
      if (++count === 4) allStarted();
      await blocked;
      return {};
    });
    const session = await authenticate();
    const requests = Array.from({ length: 4 }, () =>
      post('/api/request', { operation: 'inspect' }, session)
    );
    await started;
    assert.equal(
      (await post('/api/request', { operation: 'inspect' }, session)).status,
      429
    );
    release();
    assert.ok(
      (await Promise.all(requests)).every(response => response.status === 200)
    );
  });

  it('idle timeout and explicit authenticated close terminate listener', async () => {
    const { view } = await fixture(undefined, { idleTimeoutMs: 25 });
    await view.closed;
    await assert.rejects(fetch(view.origin));
    const second = await fixture();
    const session = await second.authenticate();
    assert.equal((await second.post('/api/close', {}, session)).status, 200);
    await second.view.closed;
    await assert.rejects(fetch(second.view.origin));
  });

  it('abort and opener failure release server', async () => {
    const controller = new AbortController();
    const { view } = await fixture(undefined, { signal: controller.signal });
    controller.abort();
    await view.closed;
    await assert.rejects(fetch(view.origin));
    let origin = '';
    await assert.rejects(
      startConfigView({
        request: async () => ({}),
        onReady: ready => {
          origin = ready.origin;
        },
        open: () => {
          throw new Error('Cannot open');
        },
      })
    );
    await assert.rejects(fetch(origin));
  });
});
