import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

const SOURCE_ROOT = path.resolve(import.meta.dirname, '../../src');
const EXTERNAL_SCHEMA_SPECIFIER = '@octocodeai/' + 'octocode-core/' + 'schemas';
const EXTERNAL_ROOT_SPECIFIER = '@octocodeai/' + 'octocode-core';

async function typescriptFiles(root: string): Promise<string[]> {
  const entries = await readdir(root, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map(async entry => {
      const target = path.join(root, entry.name);
      if (entry.isDirectory()) return typescriptFiles(target);
      return entry.isFile() && entry.name.endsWith('.ts') ? [target] : [];
    })
  );
  return nested.flat();
}

describe('local tool-contract ownership', () => {
  it('has no source import from the external schema package', async () => {
    const offenders: string[] = [];
    for (const file of await typescriptFiles(SOURCE_ROOT)) {
      const content = await readFile(file, 'utf8');
      if (content.includes(EXTERNAL_SCHEMA_SPECIFIER)) {
        offenders.push(path.relative(SOURCE_ROOT, file));
      }
    }
    expect(offenders).toEqual([]);
  });

  it('uses external completeMetadata only for the shared system prompt', async () => {
    const offenders: string[] = [];
    for (const file of await typescriptFiles(SOURCE_ROOT)) {
      const content = await readFile(file, 'utf8');
      if (
        content.includes(`from '${EXTERNAL_ROOT_SPECIFIER}'`) &&
        !file.endsWith(path.join('toolContract', 'metadata.ts'))
      ) {
        offenders.push(path.relative(SOURCE_ROOT, file));
      }
    }
    expect(offenders).toEqual([]);
  });
});
