import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { buildFileGraph } from '../../../src/graph/buildFileGraph.js';
import { analyzeGraph } from '../../../src/tools/local_analyze_graph/analyzeGraph.js';

const fixtures: string[] = [];
afterEach(async () => {
  await Promise.all(
    fixtures.splice(0).map(path => rm(path, { recursive: true, force: true }))
  );
});
async function fixture(sources: Record<string, string>) {
  const root = await mkdtemp(join(process.cwd(), '.tmp-rust-precision-'));
  fixtures.push(root);
  await mkdir(join(root, 'src'));
  await Promise.all(
    Object.entries(sources).map(([path, source]) =>
      writeFile(join(root, 'src', path), source)
    )
  );
  return root;
}

describe('native Rust file topology precision', () => {
  it('retains exact declared edges and cycles while reporting alias prefixes as gaps', async () => {
    const root = await fixture({
      'lib.rs':
        'mod implementation; mod consumer; pub use implementation as api;',
      'implementation.rs':
        'use crate::consumer::consume; pub struct Thing; pub fn call() { consume(); }',
      'consumer.rs':
        'use crate::implementation::Thing; use crate::api::Thing as Alias; pub fn consume() {}',
    });
    const built = await buildFileGraph(root, [], 20);
    const edges = [...built.fileGraph]
      .flatMap(([file, node]) =>
        [...node.importsFiles].map(target => `${file}->${target}`)
      )
      .sort();
    expect(edges).toEqual([
      'src/consumer.rs->src/implementation.rs',
      'src/implementation.rs->src/consumer.rs',
      'src/lib.rs->src/consumer.rs',
      'src/lib.rs->src/implementation.rs',
    ]);
    expect(built.coverage?.diagnostics).toContainEqual(
      expect.objectContaining({
        file: 'src/consumer.rs',
        code: 'unsupported-linking',
        message: expect.stringContaining('crate::api::Thing'),
      })
    );
    const output = await analyzeGraph(
      { operation: 'cycles', path: root },
      { getGraph: () => built }
    );
    expect(output.results).toContainEqual(
      expect.objectContaining({
        files: ['src/consumer.rs', 'src/implementation.rs'],
      })
    );
    expect(output.partialReasons).toContain('unsupportedLinking');
  });

  it('never links conditional path alternatives or generated module includes to coincidental files', async () => {
    const root = await fixture({
      'lib.rs':
        '#[cfg(feature = "active")]\n// attached attribute\n#[path = "actual.rs"] mod optional;\ninclude!(concat!(env!("OUT_DIR"), "/generated.rs"));',
      'actual.rs': 'pub struct Actual;',
      'optional.rs': 'pub struct Coincidence;',
      'generated.rs': 'pub struct Generated;',
    });
    const built = await buildFileGraph(root, [], 20);
    expect([...built.fileGraph.get('src/lib.rs')!.importsFiles]).toEqual([]);
    expect(
      built.coverage?.diagnostics.filter(
        item => item.code === 'unsupported-linking'
      ).length
    ).toBeGreaterThan(0);
  });
});
