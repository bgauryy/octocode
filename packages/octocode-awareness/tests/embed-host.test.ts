import { describe, expect, it } from 'vitest';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { resolveEmbedCommand, runHostEmbedder } from '@octocodeai/agent-contracts/embed';

function withEmbedScript(source: string, fn: (command: string) => void): void {
  const dir = mkdtempSync(join(tmpdir(), 'oc-embed-'));
  const script = join(dir, 'embed.mjs');
  writeFileSync(script, source, 'utf8');
  try {
    fn(`${process.execPath} ${script}`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

describe('embed-host', () => {
  it('resolves OCTOCODE_EMBED_CMD from env', () => {
    expect(resolveEmbedCommand({ OCTOCODE_EMBED_CMD: '  node ./x.mjs  ' })).toBe('node ./x.mjs');
    expect(resolveEmbedCommand({ OCTOCODE_EMBED_CMD: '   ' })).toBeNull();
    expect(resolveEmbedCommand({})).toBeNull();
  });

  it('runs a host embedder command and parses embedding JSON', () => {
    withEmbedScript(`#!/usr/bin/env node
import { readFileSync } from 'node:fs';
const text = readFileSync(0, 'utf8');
process.stdout.write(JSON.stringify({ embedding: [1, 2, 3], model: 'unit', echo: text.trim() }));
`, (command) => {
      const result = runHostEmbedder('hello', { command });
      expect([...result.embedding]).toEqual([1, 2, 3]);
      expect(result.model).toBe('unit');
    });
  });

  it('uses the default model when the host omits model', () => {
    withEmbedScript(`process.stdout.write(JSON.stringify({ embedding: [4, 5] }));`, (command) => {
      const result = runHostEmbedder('hello', { command });
      expect([...result.embedding]).toEqual([4, 5]);
      expect(result.model).toBe('host-embed');
    });
  });

  it('reports host embedder configuration and process failures', () => {
    expect(() => runHostEmbedder('hello', { command: '' })).toThrow('OCTOCODE_EMBED_CMD is not set');
    withEmbedScript(`process.stderr.write('bad key'); process.exit(7);`, (command) => {
      expect(() => runHostEmbedder('hello', { command })).toThrow('exited 7: bad key');
    });
    expect(() => runHostEmbedder('hello', {
      command: `${process.execPath} -e "setTimeout(() => {}, 1000)"`,
      timeoutMs: 1,
    })).toThrow('OCTOCODE_EMBED_CMD failed to start');
  });

  it('validates host embedder stdout shape', () => {
    withEmbedScript(``, (command) => {
      expect(() => runHostEmbedder('hello', { command })).toThrow('returned empty stdout');
    });
    withEmbedScript(`process.stdout.write('not json');`, (command) => {
      expect(() => runHostEmbedder('hello', { command })).toThrow('stdout is not JSON');
    });
    withEmbedScript(`process.stdout.write(JSON.stringify([1, 2]));`, (command) => {
      expect(() => runHostEmbedder('hello', { command })).toThrow('JSON must be an object');
    });
    withEmbedScript(`process.stdout.write(JSON.stringify({ embedding: [1, "bad"] }));`, (command) => {
      expect(() => runHostEmbedder('hello', { command })).toThrow('embedding must be a non-empty number[]');
    });
  });
});
