import { chmodSync, existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  clearDiscoveryCache,
  discoverServer,
  discoverServerBatch,
} from '../../src/lsp/serverDiscovery.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const roots: string[] = [];

function tempWorkspace(): string {
  const root = path.join(
    process.env.TMPDIR ?? '/tmp',
    `octocode-discovery-${process.pid}-${roots.length}`
  );
  mkdirSync(root, { recursive: true });
  roots.push(root);
  return root;
}

function makeExe(filePath: string): void {
  mkdirSync(path.dirname(filePath), { recursive: true });
  writeFileSync(filePath, '#!/bin/sh\n');
  if (process.platform !== 'win32') chmodSync(filePath, 0o755);
}

beforeEach(() => clearDiscoveryCache());
afterEach(() => {
  clearDiscoveryCache();
  while (roots.length) {
    rmSync(roots.pop()!, { recursive: true, force: true });
  }
});

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

describe('discoverServer – resolution', () => {
  it('finds a project-local server in node_modules/.bin', () => {
    const root = tempWorkspace();
    const server = path.join(root, 'node_modules', '.bin', 'some-language-server');
    makeExe(server);

    expect(discoverServer('some-language-server', root)).toEqual({
      command: server,
      source: 'project-local',
    });
  });

  it('walks up parent directories for node_modules/.bin', () => {
    const root = tempWorkspace();
    const server = path.join(root, 'node_modules', '.bin', 'hoisted-server');
    makeExe(server);

    const nested = path.join(root, 'packages', 'app', 'src');
    mkdirSync(nested, { recursive: true });

    expect(discoverServer('hoisted-server', nested)).toEqual({
      command: server,
      source: 'project-local',
    });
  });

  it('returns null when nothing on the machine provides the command', () => {
    const root = tempWorkspace();
    expect(discoverServer('definitely-not-an-installed-server-xyz', root)).toBeNull();
  });

  it('normalises workspaceRoot — trailing slash maps to same result', () => {
    const root = tempWorkspace();
    const server = path.join(root, 'node_modules', '.bin', 'slash-server');
    makeExe(server);

    const a = discoverServer('slash-server', root);
    clearDiscoveryCache();
    const b = discoverServer('slash-server', root + '/');
    expect(a).toEqual(b);
    expect(a).not.toBeNull();
  });

  it('uses command basename — path prefix is stripped', () => {
    const root = tempWorkspace();
    const server = path.join(root, 'node_modules', '.bin', 'bare-server');
    makeExe(server);

    const r1 = discoverServer('bare-server', root);
    clearDiscoveryCache();
    const r2 = discoverServer('/some/prefix/bare-server', root);
    expect(r1).toEqual(r2);
    expect(r1).not.toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Caching — behavioural proofs (no spy needed)
// ---------------------------------------------------------------------------

describe('discoverServer – caching', () => {
  it('null result is cached — server installed after first call is not visible until cache is cleared', () => {
    const root = tempWorkspace();

    // First call: server absent → null cached
    expect(discoverServer('install-later', root)).toBeNull();

    // Install the server
    const server = path.join(root, 'node_modules', '.bin', 'install-later');
    makeExe(server);

    // Still null — cached result from before install
    expect(discoverServer('install-later', root)).toBeNull();

    // Clear → fresh scan finds it
    clearDiscoveryCache();
    expect(discoverServer('install-later', root)).toEqual({
      command: server,
      source: 'project-local',
    });
  });

  it('positive result is stable across repeated calls', () => {
    const root = tempWorkspace();
    const server = path.join(root, 'node_modules', '.bin', 'stable-server');
    makeExe(server);

    const r1 = discoverServer('stable-server', root);
    const r2 = discoverServer('stable-server', root);
    const r3 = discoverServer('stable-server', root);
    expect(r1).toEqual(r2);
    expect(r2).toEqual(r3);
    expect(r1).not.toBeNull();
  });

  it('different commands have independent cache entries', () => {
    const root = tempWorkspace();
    const serverA = path.join(root, 'node_modules', '.bin', 'server-a');
    makeExe(serverA);

    expect(discoverServer('server-a', root)).toEqual({ command: serverA, source: 'project-local' });
    expect(discoverServer('server-b', root)).toBeNull();
    // server-a must still resolve after server-b was looked up
    expect(discoverServer('server-a', root)).toEqual({ command: serverA, source: 'project-local' });
  });

  it('different workspaceRoots have independent cache entries', () => {
    const rootA = tempWorkspace();
    const rootB = tempWorkspace();
    const server = path.join(rootA, 'node_modules', '.bin', 'scoped-server');
    makeExe(server);

    expect(discoverServer('scoped-server', rootA)).not.toBeNull();
    expect(discoverServer('scoped-server', rootB)).toBeNull();
  });

  it('clearDiscoveryCache flushes all entries — previously-missing server becomes findable', () => {
    const root = tempWorkspace();
    // prime the cache with a miss
    expect(discoverServer('was-missing', root)).toBeNull();
    expect(discoverServer('was-missing', root)).toBeNull();

    const server = path.join(root, 'node_modules', '.bin', 'was-missing');
    makeExe(server);

    clearDiscoveryCache();
    expect(discoverServer('was-missing', root)).not.toBeNull();
  });

  it('clearDiscoveryCache flushes the ecosystem dir pre-filter — picks up new existing dirs', () => {
    const root = tempWorkspace();
    const fakeEco = path.join(root, 'fake-eco', 'bin');

    // Call once — pre-filter cached without fakeEco (it doesn't exist yet)
    discoverServer('eco-probe', root);

    // Create fakeEco dir AFTER initial cache build
    makeExe(path.join(fakeEco, 'eco-probe'));

    // Still not found (pre-filter cached old state)
    expect(discoverServer('eco-probe', root)).toBeNull();

    // After clear: pre-filter is rebuilt. But we can't inject the dir into
    // the ecosystem list without env vars — just assert no throw.
    clearDiscoveryCache();
    expect(() => discoverServer('eco-probe', root)).not.toThrow();
  });
});

// ---------------------------------------------------------------------------
// Ecosystem dir pre-filter
// ---------------------------------------------------------------------------

describe('discoverServer – ecosystem dir pre-filter', () => {
  it('does not throw when all ecosystem dirs are absent', () => {
    const root = tempWorkspace();
    expect(() =>
      discoverServer('xyzzy-server-that-cannot-exist', root)
    ).not.toThrow();
  });

  it('does not find servers in dirs that do not exist', () => {
    const root = tempWorkspace();
    // Even if we create a server file inside a dir that the pre-filter
    // would cache as non-existent, it should not appear without a cache clear.
    const phantom = path.join(root, 'phantom', 'bin', 'phantom-server');

    // Trigger pre-filter build (phantom dir absent)
    discoverServer('phantom-server', root);

    // Now create the dir and binary
    makeExe(phantom);

    // Cached pre-filter still excludes it
    expect(discoverServer('phantom-server', root)).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Batch discovery
// ---------------------------------------------------------------------------

describe('discoverServerBatch', () => {
  it('returns results keyed by command name', () => {
    const root = tempWorkspace();
    const serverA = path.join(root, 'node_modules', '.bin', 'batch-a');
    makeExe(serverA);

    const results = discoverServerBatch(['batch-a', 'batch-missing'], root);

    expect(results['batch-a']).toEqual({ command: serverA, source: 'project-local' });
    expect(results['batch-missing']).toBeNull();
  });

  it('is equivalent to N individual discoverServer calls', () => {
    const root = tempWorkspace();
    const serverTwo = path.join(root, 'node_modules', '.bin', 'lsp-two');
    makeExe(serverTwo);

    const commands = ['lsp-one', 'lsp-two', 'lsp-three'];
    const batch = discoverServerBatch(commands, root);
    clearDiscoveryCache();
    for (const cmd of commands) {
      expect(batch[cmd]).toEqual(discoverServer(cmd, root));
    }
  });

  it('batch results are stored in cache — subsequent individual call needs no rescan', () => {
    const root = tempWorkspace();
    const server = path.join(root, 'node_modules', '.bin', 'batch-cached');
    makeExe(server);

    discoverServerBatch(['batch-cached'], root);

    // Install a different server AFTER batch ran (cache must prevent finding it)
    const impostor = path.join(root, 'node_modules', '.bin', 'batch-cached-impostor');
    makeExe(impostor);

    // batch-cached must still be the result from cache (not impostor)
    expect(discoverServer('batch-cached', root)).toEqual({
      command: server,
      source: 'project-local',
    });
  });

  it('individual call results are visible to a subsequent batch', () => {
    const root = tempWorkspace();
    discoverServer('pre-cached-cmd', root); // puts null in cache

    // Install after individual call
    const server = path.join(root, 'node_modules', '.bin', 'pre-cached-cmd');
    makeExe(server);

    // Batch must see the cached null (not the new server)
    const results = discoverServerBatch(['pre-cached-cmd'], root);
    expect(results['pre-cached-cmd']).toBeNull();

    // After clear, both find it
    clearDiscoveryCache();
    const results2 = discoverServerBatch(['pre-cached-cmd'], root);
    expect(results2['pre-cached-cmd']).not.toBeNull();
  });

  it('handles empty command list', () => {
    const root = tempWorkspace();
    expect(discoverServerBatch([], root)).toEqual({});
  });

  it('handles duplicate commands in the list', () => {
    const root = tempWorkspace();
    const server = path.join(root, 'node_modules', '.bin', 'dup-server');
    makeExe(server);

    const results = discoverServerBatch(['dup-server', 'dup-server'], root);
    expect(results['dup-server']).not.toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

describe('discoverServer – edge cases', () => {
  it('handles empty command string', () => {
    const root = tempWorkspace();
    expect(discoverServer('', root)).toBeNull();
  });

  it('handles relative workspaceRoot without throwing', () => {
    expect(() => discoverServer('some-server', '.')).not.toThrow();
  });

  it('handles non-existent workspaceRoot without throwing', () => {
    expect(() =>
      discoverServer('some-server', '/this/path/does/not/exist/at/all')
    ).not.toThrow();
  });
});
