import { access, readFile } from 'node:fs/promises';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

const SOURCE_ROOT = path.resolve(import.meta.dirname, '../../src');
const CONTRACT_ROOT = path.join(SOURCE_ROOT, 'toolContract');

describe('tool-contract ownership', () => {
  it('does not keep a duplicate resources tree in tools-core', async () => {
    await expect(
      access(path.join(CONTRACT_ROOT, 'resources'))
    ).rejects.toThrow();
  });

  it('owns executable schemas and runtime validation locally', async () => {
    const schemas = await readFile(
      path.join(CONTRACT_ROOT, 'schemas.ts'),
      'utf8'
    );
    const runtime = await readFile(
      path.join(CONTRACT_ROOT, 'runtime.ts'),
      'utf8'
    );

    expect(schemas).toContain("from './input/resources/tools/");
    expect(schemas).not.toContain('@octocodeai/octocode-core/schemas');
    expect(runtime).toContain("from './schemas.js'");
    expect(runtime).not.toContain('@octocodeai/octocode-core/schemas');
  });

  it('uses octocode-core only for the shared system prompt metadata', async () => {
    const metadata = await readFile(
      path.join(CONTRACT_ROOT, 'metadata.ts'),
      'utf8'
    );

    expect(metadata).toContain("from '@octocodeai/octocode-core'");
    expect(metadata).toContain('DIRECT_TOOL_DISCOVERY_DEFINITIONS');
  });
});
