import { describe, expect, it } from 'vitest';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { resolveEmbedCommand, runHostEmbedder } from '../src/embed-host.js';

describe('embed-host', () => {
  it('resolves OCTOCODE_EMBED_CMD from env', () => {
    expect(resolveEmbedCommand({ OCTOCODE_EMBED_CMD: '  node ./x.mjs  ' })).toBe('node ./x.mjs');
    expect(resolveEmbedCommand({ OCTOCODE_EMBED_CMD: '   ' })).toBeNull();
    expect(resolveEmbedCommand({})).toBeNull();
  });

  it('runs a host embedder command and parses embedding JSON', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-embed-'));
    const script = join(dir, 'embed.mjs');
    writeFileSync(script, `#!/usr/bin/env node
import { readFileSync } from 'node:fs';
const text = readFileSync(0, 'utf8');
process.stdout.write(JSON.stringify({ embedding: [1, 2, 3], model: 'unit', echo: text.trim() }));
`, 'utf8');
    try {
      const result = runHostEmbedder('hello', { command: `node ${script}` });
      expect([...result.embedding]).toEqual([1, 2, 3]);
      expect(result.model).toBe('unit');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
