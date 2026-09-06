import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

function sourceFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) return sourceFiles(path);
    return /\.[cm]?tsx?$/.test(entry.name) ? [path] : [];
  });
}

describe('executable tool contract ownership', () => {
  it('does not import executable schemas or MCP registry metadata from octocode-core', () => {
    const packageRoot = resolve(import.meta.dirname, '../..');
    const offenders = sourceFiles(resolve(packageRoot, 'src'))
      .filter(path =>
        /@octocodeai\/octocode-core\/(?:schemas|mcp)/.test(
          readFileSync(path, 'utf8')
        )
      )
      .map(path => path.slice(packageRoot.length + 1));
    expect(offenders).toEqual([]);
  });
});
