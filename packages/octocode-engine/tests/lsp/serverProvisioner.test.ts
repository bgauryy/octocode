import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import path from 'node:path';
import { tmpdir } from 'node:os';
import { gzipSync } from 'node:zlib';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { detectPlatformId } from '../../src/lsp/platform.js';

const PLATFORM = detectPlatformId();
const BIN_BYTES = Buffer.from('#!/bin/sh\necho fake-server\n');
const GZ_ASSET = gzipSync(BIN_BYTES);
const GZ_SHA = createHash('sha256').update(GZ_ASSET).digest('hex');

let cacheDir: string;

function mockManifest(sha256: string | null) {
  vi.doMock('../../src/lsp/serverManifest.js', () => {
    const binPathFor = (serverName: string) =>
      path.join(cacheDir, serverName, 'v1', 'fake-server');
    return {
      provisionMode: () => 'auto',
      managedCacheRoot: () => cacheDir,
      manifestServer: (name: string) =>
        name === 'fake-server'
          ? {
              languageId: 'fake',
              repo: 'acme/fake',
              releaseTag: 'v1',
              platforms: {
                [PLATFORM]: {
                  url: 'https://github.com/acme/fake/releases/download/v1/fake.gz',
                  archive: 'gz',
                  binName: 'fake-server',
                  sha256,
                },
              },
            }
          : null,
      cachedServerBinPath: (name: string) => binPathFor(name),
      resolveCachedServer: (name: string) => {
        const p = binPathFor(name);
        return existsSync(p) && existsSync(`${p}.ok`) ? p : null;
      },
    };
  });
}

beforeEach(() => {
  cacheDir = mkdtempSync(path.join(tmpdir(), 'octocode-provision-'));
});

afterEach(() => {
  rmSync(cacheDir, { recursive: true, force: true });
  vi.doUnmock('../../src/lsp/serverManifest.js');
  vi.resetModules();
  vi.unstubAllGlobals();
});

function stubFetch(body: Buffer, status = 200) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => ({
      ok: status >= 200 && status < 300,
      status,
      headers: new Map(),
      arrayBuffer: async () =>
        body.buffer.slice(body.byteOffset, body.byteOffset + body.byteLength),
    }))
  );
}

describe('provisionServer', () => {
  it('downloads, verifies, gunzips, and writes a binary + .ok marker', async () => {
    mockManifest(GZ_SHA);
    stubFetch(GZ_ASSET);
    const { provisionServer } = await import('../../src/lsp/serverProvisioner.js');

    const result = await provisionServer('fake-server', { mode: 'auto' });
    expect(result.ok).toBe(true);
    expect(result.source).toBe('downloaded');
    expect(readFileSync(result.path!)).toEqual(BIN_BYTES);
    expect(existsSync(`${result.path}.ok`)).toBe(true);
  });

  it('refuses when auto-install mode is off', async () => {
    mockManifest(GZ_SHA);
    stubFetch(GZ_ASSET);
    const { provisionServer } = await import('../../src/lsp/serverProvisioner.js');

    const result = await provisionServer('fake-server', { mode: 'off' });
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/Auto-install is off/);
  });

  it('refuses to download when sha256 is not pinned', async () => {
    mockManifest(null);
    stubFetch(GZ_ASSET);
    const { provisionServer } = await import('../../src/lsp/serverProvisioner.js');

    const result = await provisionServer('fake-server', { mode: 'auto' });
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/no pinned sha256/);
  });

  it('rejects a checksum mismatch and writes no binary', async () => {
    mockManifest('0'.repeat(64));
    stubFetch(GZ_ASSET);
    const { provisionServer } = await import('../../src/lsp/serverProvisioner.js');

    const result = await provisionServer('fake-server', { mode: 'auto' });
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/Checksum mismatch/);
    expect(existsSync(path.join(cacheDir, 'fake-server', 'v1', 'fake-server'))).toBe(false);
  });

  it('is idempotent — a second call returns the already-present copy without re-fetching', async () => {
    mockManifest(GZ_SHA);
    stubFetch(GZ_ASSET);
    const { provisionServer } = await import('../../src/lsp/serverProvisioner.js');

    await provisionServer('fake-server', { mode: 'auto' });
    const second = await provisionServer('fake-server', { mode: 'auto' });
    expect(second.ok).toBe(true);
    expect(second.source).toBe('already-present');
  });

  it('reports a clear error for an unknown server', async () => {
    mockManifest(GZ_SHA);
    stubFetch(GZ_ASSET);
    const { provisionServer } = await import('../../src/lsp/serverProvisioner.js');

    const result = await provisionServer('not-a-server', { mode: 'auto' });
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/not an auto-downloadable server/);
  });
});
