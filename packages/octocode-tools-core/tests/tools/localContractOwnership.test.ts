import { access, readFile } from 'node:fs/promises';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

const SOURCE_ROOT = path.resolve(import.meta.dirname, '../../src');
const CONTRACT_ROOT = path.join(SOURCE_ROOT, 'toolContract');

describe('shared tool-contract ownership', () => {
  it('does not keep a duplicate resources tree in tools-core', async () => {
    await expect(
      access(path.join(CONTRACT_ROOT, 'resources'))
    ).rejects.toThrow();
  });

  it('routes executable schemas through octocode-core', async () => {
    const schemas = await readFile(
      path.join(CONTRACT_ROOT, 'schemas.ts'),
      'utf8'
    );
    const runtime = await readFile(
      path.join(CONTRACT_ROOT, 'runtime.ts'),
      'utf8'
    );

    expect(schemas).toContain("from '@octocodeai/octocode-core/schemas'");
    expect(runtime).toContain(
      "from '@octocodeai/octocode-core/schemas/runtime'"
    );
  });

  it('routes descriptions and metadata through octocode-core', async () => {
    const metadata = await readFile(
      path.join(CONTRACT_ROOT, 'metadata.ts'),
      'utf8'
    );

    expect(metadata).toContain("from '@octocodeai/octocode-core'");
  });
});
