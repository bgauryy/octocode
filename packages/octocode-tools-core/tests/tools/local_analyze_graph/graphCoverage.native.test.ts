import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { analyzeGraph } from '../../../src/tools/local_analyze_graph/analyzeGraph.js';

const fixtures: string[] = [];
afterEach(async () => {
  await Promise.all(
    fixtures.splice(0).map(path => rm(path, { recursive: true, force: true }))
  );
});

describe('native Rust facts through the public graph analyzer', () => {
  it('links multiline grouped imports and module declarations with explicit unsupported coverage', async () => {
    const root = await mkdtemp(join(process.cwd(), '.tmp-rust-graph-'));
    fixtures.push(root);
    await mkdir(join(root, 'src', 'structural'), { recursive: true });
    const sources = {
      'src/lib.rs': 'mod structural;\n',
      'src/structural/mod.rs':
        'mod files; mod language; mod query; mod types;\n',
      'src/structural/files.rs':
        'use std::fs;\nuse super::{\n language::AgLanguage,\n query::{Prefilter, compile as compile_query},\n types::Options,\n};\nuse crate::missing::Thing;\n',
      'src/structural/language.rs': 'pub struct AgLanguage;\n',
      'src/structural/query.rs': 'pub struct Prefilter; pub fn compile() {}\n',
      'src/structural/types.rs': 'pub struct Options;\n',
    };
    await Promise.all(
      Object.entries(sources).map(([file, source]) =>
        writeFile(join(root, file), source)
      )
    );
    const output = await analyzeGraph({
      operation: 'dependencies',
      path: root,
      file: 'src/structural/files.rs',
    });
    expect(output.results.map(result => result.file).sort()).toEqual([
      'src/structural/language.rs',
      'src/structural/query.rs',
      'src/structural/types.rs',
    ]);
    expect(output.coverage?.imports).toEqual({
      resolved: 9,
      external: 1,
      unresolvedInternal: 1,
      unsupported: 0,
    });
    expect(output.partialReasons).toEqual(['unresolvedImports']);
    expect(output.terminalLimit).toBe(true);
    const modules = await analyzeGraph({
      operation: 'dependencies',
      path: root,
      file: 'src/lib.rs',
      depth: 2,
    });
    expect(modules.results.map(result => result.file)).toContain(
      'src/structural/files.rs'
    );
  });
});
