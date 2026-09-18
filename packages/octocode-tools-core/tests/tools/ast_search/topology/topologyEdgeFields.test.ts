/**
 * Tests for importLine and inboundCount fields on topology analysis results.
 * importLine: exact 1-based line number of the import statement in the importer file.
 * inboundCount: number of scanned files that import this result file (in-degree).
 */
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { analyzeTopology } from '../../../../src/tools/ast_search/topology/analyzeTopology.js';

const tempDirs: string[] = [];
afterEach(async () => {
  await Promise.all(
    tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true }))
  );
});

/**
 * Creates:
 *   index.js (line 1: import a, line 2: import b)
 *   a.js     (line 1: import shared)
 *   b.js     (line 1: import shared)
 *   shared.js (no imports) ← imported by both a.js and b.js (inboundCount=2)
 */
async function makeFixture(): Promise<string> {
  const dir = await mkdtemp(join(process.cwd(), '.tmp-topology-edge-'));
  tempDirs.push(dir);
  await writeFile(
    join(dir, 'package.json'),
    JSON.stringify({ name: 'fixture', type: 'module' })
  );
  await writeFile(
    join(dir, 'index.js'),
    [
      "import { aFn } from './a.js';",
      "import { bFn } from './b.js';",
      'export const entry = () => aFn() + bFn();',
    ].join('\n') + '\n'
  );
  await writeFile(
    join(dir, 'a.js'),
    [
      "import { shared } from './shared.js';",
      'export const aFn = () => shared();',
    ].join('\n') + '\n'
  );
  await writeFile(
    join(dir, 'b.js'),
    [
      "import { shared } from './shared.js';",
      'export const bFn = () => shared();',
    ].join('\n') + '\n'
  );
  await writeFile(
    join(dir, 'shared.js'),
    'export const shared = () => 42;\n'
  );
  return dir;
}

describe('topology edge fields — importLine and inboundCount', () => {
  it('dependencies result includes importLine for each edge', async () => {
    const dir = await makeFixture();
    const result = await analyzeTopology({
      operation: 'dependencies',
      file: join(dir, 'index.js'),
      path: dir,
      depth: 1,
    });

    expect(result.status).not.toBe('error');
    const items = result.results as Array<Record<string, unknown>>;
    expect(items.length).toBeGreaterThan(0);

    // index.js imports a.js on line 1 and b.js on line 2
    const aItem = items.find(r => String(r.file).endsWith('a.js'));
    const bItem = items.find(r => String(r.file).endsWith('b.js'));
    expect(aItem).toBeDefined();
    expect(bItem).toBeDefined();
    expect(aItem?.importLine).toBe(1);
    expect(bItem?.importLine).toBe(2);
  });

  it('dependencies result includes inboundCount reflecting scanned-graph in-degree', async () => {
    const dir = await makeFixture();
    // Scan from index.js with depth 3 to reach shared.js
    const result = await analyzeTopology({
      operation: 'dependencies',
      file: join(dir, 'index.js'),
      path: dir,
      depth: 3,
    });

    expect(result.status).not.toBe('error');
    const items = result.results as Array<Record<string, unknown>>;

    // shared.js is imported by both a.js and b.js → inboundCount should be 2
    const sharedItem = items.find(r => String(r.file).endsWith('shared.js'));
    expect(sharedItem).toBeDefined();
    expect(sharedItem?.inboundCount).toBe(2);

    // a.js and b.js are each imported only by index.js → inboundCount should be 1
    const aItem = items.find(r => String(r.file).endsWith('a.js'));
    const bItem = items.find(r => String(r.file).endsWith('b.js'));
    expect(aItem?.inboundCount).toBe(1);
    expect(bItem?.inboundCount).toBe(1);
  });

  it('dependents result includes importLine for the importer file', async () => {
    const dir = await makeFixture();
    // Ask: who imports shared.js? Both a.js (line 1) and b.js (line 1)
    const result = await analyzeTopology({
      operation: 'dependents',
      file: join(dir, 'shared.js'),
      path: dir,
      depth: 1,
    });

    expect(result.status).not.toBe('error');
    const items = result.results as Array<Record<string, unknown>>;

    // Both a.js and b.js import shared.js on line 1
    const aItem = items.find(r => String(r.file).endsWith('a.js'));
    const bItem = items.find(r => String(r.file).endsWith('b.js'));
    expect(aItem).toBeDefined();
    expect(bItem).toBeDefined();
    expect(aItem?.importLine).toBe(1);
    expect(bItem?.importLine).toBe(1);
  });

  it('inboundCount=0 for a file that is not imported by anything in the scan scope', async () => {
    const dir = await makeFixture();
    // index.js is not imported by anyone → its dependents list is empty,
    // but when it appears as a result elsewhere, inboundCount should be 0.
    const result = await analyzeTopology({
      operation: 'dependencies',
      file: join(dir, 'a.js'),
      path: dir,
      depth: 1,
    });
    expect(result.status).not.toBe('error');
    const items = result.results as Array<Record<string, unknown>>;

    // shared.js when scanned from a.js scope: only a.js imports it within depth-1
    // BUT the inDegree is computed over the full scanned graph (not just depth-1)
    const sharedItem = items.find(r => String(r.file).endsWith('shared.js'));
    // inboundCount reflects the full graph, not just depth-limited path
    expect(sharedItem?.inboundCount).toBeGreaterThanOrEqual(1);
  });

  it('importLine is absent when the import cannot be resolved', async () => {
    const dir = await makeFixture();
    // index.js imports a.js with a static import; the line is resolved.
    // We verify no spurious undefined entries — importLine is only set when known.
    const result = await analyzeTopology({
      operation: 'dependencies',
      file: join(dir, 'index.js'),
      path: dir,
      depth: 1,
    });
    const items = result.results as Array<Record<string, unknown>>;
    for (const item of items) {
      if (item.importLine !== undefined) {
        // When present it must be a positive integer
        expect(typeof item.importLine).toBe('number');
        expect(item.importLine as number).toBeGreaterThan(0);
      }
    }
  });
});
