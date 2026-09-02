import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { afterAll, beforeAll, bench, describe } from 'vitest';

type NativeAddon = typeof import('../index.js');

const fixtureRoots = new Map<number, string>();
let addon: NativeAddon;

function graphModule(index: number): string {
  const functions = Array.from(
    { length: 80 },
    (_, item) =>
      `export function value${index}_${item}(input: number) { return input + ${item}; }`
  );
  return functions.join('\n');
}

async function legacyPerFileScan(root: string, fileCount: number) {
  const query = await addon.queryFileSystem({
    path: root,
    recursive: true,
    showHidden: false,
    entryType: 'f',
    extensions: addon.getSupportedGraphFactExtensions(),
    stopAtLimit: true,
    limit: fileCount + 1,
  });
  let parsedFiles = 0;
  let referenceCount = 0;
  for (const entry of query.entries) {
    const content = readFileSync(entry.path, 'utf8');
    const factsJson = addon.extractGraphFacts(content, entry.relativePath);
    if (!factsJson) continue;
    const facts = JSON.parse(factsJson) as {
      declarations?: Array<{ name: string; exported?: boolean }>;
    };
    for (const declaration of facts.declarations ?? []) {
      if (!declaration.exported) continue;
      referenceCount +=
        content.match(new RegExp(`\\b${declaration.name}\\b`, 'g'))?.length ??
        0;
    }
    parsedFiles += 1;
  }
  return { parsedFiles, referenceCount };
}

beforeAll(async () => {
  addon = await import('../index.js');
  for (const fileCount of [1, 10, 50]) {
    const root = mkdtempSync(
      join(tmpdir(), `octocode-graph-bench-${fileCount}-`)
    );
    fixtureRoots.set(fileCount, root);
    for (let index = 0; index < fileCount; index++) {
      writeFileSync(join(root, `module-${index}.ts`), graphModule(index));
    }
  }
});

afterAll(() => {
  for (const root of fixtureRoots.values()) {
    rmSync(root, { recursive: true, force: true });
  }
});

describe('native graph fact scan latency', () => {
  for (const fileCount of [1, 10, 50]) {
    bench(
      `${fileCount} TypeScript files — legacy per-file boundary`,
      async () => {
        const result = await legacyPerFileScan(
          fixtureRoots.get(fileCount)!,
          fileCount
        );
        if (result.parsedFiles !== fileCount || result.referenceCount === 0) {
          throw new Error(
            `unexpected legacy scan result for ${fileCount} files`
          );
        }
      }
    );

    bench(`${fileCount} TypeScript files — native batch boundary`, async () => {
      const result = await addon.scanGraphFacts({
        path: fixtureRoots.get(fileCount)!,
        maxFiles: fileCount + 1,
      });
      if (result.entries.length !== fileCount || result.truncated) {
        throw new Error(`unexpected graph scan result for ${fileCount} files`);
      }
    });
  }
});
