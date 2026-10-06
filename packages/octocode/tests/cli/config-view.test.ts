import { afterEach, describe, expect, it, vi } from 'vitest';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { getPublicToolCatalogWithAddons } from '@octocodeai/config/schema';
import {
  requestNativeConfig,
  configViewCommand,
} from '../../src/cli/commands/config-view.js';

const dirs: string[] = [];
function backend(body: string): string {
  const dir = mkdtempSync(join(tmpdir(), 'config-bridge-'));
  dirs.push(dir);
  const file = join(dir, 'native.mjs');
  writeFileSync(
    file,
    `let body='';for await(const chunk of process.stdin)body+=chunk;const request=JSON.parse(body);${body}`
  );
  return file;
}
const fingerprint = getPublicToolCatalogWithAddons({
  availableTools: [],
}).fingerprint;
afterEach(() => {
  for (const dir of dirs.splice(0))
    rmSync(dir, { recursive: true, force: true });
});
describe('native config bridge', () => {
  it('carries the trusted fingerprint in stdin and adds generated editor metadata', async () => {
    const bin = backend(
      `console.log(JSON.stringify({success:true,apiVersion:1,fingerprint:request.expectedFingerprint,data:{settings:[{key:'network.timeout',value:6000}],operation:request.operation,args:process.argv.slice(2)}}));`
    );
    const data = await requestNativeConfig(bin, {
      operation: 'inspect',
      expectedFingerprint: 'untrusted',
    });
    expect(data.args).toEqual(['config', '--manage']);
    expect(data.settings).toEqual([
      expect.objectContaining({
        key: 'network.timeout',
        type: 'number',
        minimum: 5000,
        value: 6000,
      }),
    ]);
  });
  it('rejects incompatible native responses', async () => {
    const bin = backend(
      "console.log(JSON.stringify({success:true,apiVersion:1,fingerprint:'old',data:{}}));"
    );
    await expect(
      requestNativeConfig(bin, { operation: 'inspect' })
    ).rejects.toMatchObject({ code: 'UNAVAILABLE' });
  });
  it('does not echo raw output or diagnostics on protocol failure', async () => {
    const bin = backend(
      "console.log('secret-marker');console.error('another-secret');"
    );
    await expect(
      requestNativeConfig(bin, { operation: 'inspect' })
    ).rejects.toThrow('unavailable or incompatible');
  });
  it('reports sanitized native conflicts', async () => {
    const bin = backend(
      `console.log(JSON.stringify({success:false,apiVersion:1,error:{code:'CONFLICT',message:'Configuration changed; refresh before saving.'},fingerprint:${JSON.stringify(fingerprint)}}));process.exitCode=2;`
    );
    await expect(
      requestNativeConfig(bin, { operation: 'setSetting' })
    ).rejects.toMatchObject({ code: 'CONFLICT', safe: true });
  });
  it('aborts without launching a backend when the session has ended', async () => {
    await expect(
      requestNativeConfig(
        '/missing',
        { operation: 'inspect' },
        { signal: AbortSignal.abort() }
      )
    ).rejects.toMatchObject({ code: 'CLOSED' });
  });
  it('rejects conflicting native flags before starting a browser session', async () => {
    await expect(
      configViewCommand('/missing', ['config', 'view', '--add', 'KEY'])
    ).resolves.toBe(2);
  });
});

describe('config view lifecycle', () => {
  it('runs an authenticated session and closes without leaving signal handlers', async () => {
    const bin = backend(
      `console.log(JSON.stringify({success:true,apiVersion:1,fingerprint:request.expectedFingerprint,data:{settings:[],keys:[],files:{},agents:[]}}));`
    );
    const listeners = process.listeners('SIGTERM');
    let ready: (url: string) => void = () => {};
    const url = new Promise<string>(resolve => {
      ready = resolve;
    });
    const output = vi.spyOn(console, 'log').mockImplementation(message => {
      const match = String(message).match(/http:\/\/127\.0\.0\.1:\d+\/#[\w-]+/);
      if (match) ready(match[0]);
    });
    const running = configViewCommand(bin, [
      '--no-color',
      'config',
      'view',
      '--no-open',
      '--idle-timeout=30',
    ]);
    try {
      const link = new URL(await url);
      const headers = {
        Origin: link.origin,
        'Content-Type': 'application/json',
        'X-Octocode-Session': link.hash.slice(1),
      };
      const response = await fetch(link.origin + '/api/session', {
        method: 'POST',
        headers,
        body: '{}',
      });
      expect(response.status).toBe(200);
      const session = (await response.json()) as { token: string };
      headers['X-Octocode-Session'] = session.token;
      const read = await fetch(link.origin + '/api/request', {
        method: 'POST',
        headers,
        body: JSON.stringify({ operation: 'inspect' }),
      });
      expect(read.status).toBe(200);
      const close = await fetch(link.origin + '/api/close', {
        method: 'POST',
        headers,
        body: '{}',
      });
      expect(close.status).toBe(200);
      await expect(running).resolves.toBe(0);
      expect(process.listeners('SIGTERM')).toEqual(listeners);
      await expect(fetch(link.origin)).rejects.toThrow();
    } finally {
      output.mockRestore();
    }
  });
});
