import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { executeAstSearch } from '../../src/tools/ast_search/execution.js';
import { contextUtils } from '../../src/utils/contextUtils.js';
import { AstSearchQuerySchema } from '@octocodeai/octocode-core/schema';

let root: string;
async function run(query: Record<string, unknown>) {
  const parsed = AstSearchQuerySchema.parse({
    reasoning: 'Exercise astSearch integration behavior and continuations.',
    debug: true,
    ...query,
  });
  const response = await executeAstSearch({ queries: [parsed] });
  const row = (response.structuredContent as any).results[0];
  return { ...row.data, status: row.status ?? row.data.status, meta: row.meta };
}
async function pages(query: Record<string, unknown>, field: string) {
  const values: any[] = [];
  for (let count = 0; count < 100; count++) {
    const result = await run(query);
    expect(result.status).not.toBe('error');
    expect(result.meta.diagnostics?.codes ?? []).not.toContain(
      'continuationMissing'
    );
    values.push(...(result[field] ?? []));
    const next = result.next?.nextPage;
    if (!next) return values;
    expect(next.tool).toBe('astSearch');
    expect(AstSearchQuerySchema.safeParse(next.query).success).toBe(true);
    query = next.query;
  }
  throw new Error('Continuation failed to terminate');
}

beforeAll(async () => {
  root = await mkdtemp(join(process.cwd(), '.octocode-ast-migration-'));
  await writeFile(
    join(root, 'a.ts'),
    'import { b } from "./b.js";\nexport function a() { return b(); }\nexport const emoji = "😀";\n'
  );
  await writeFile(
    join(root, 'b.ts'),
    'export function b() { return 1; }\nexport function c() { return b(); }\n'
  );
  await writeFile(join(root, 'unused.ts'), 'export const unused = 0;\n');
});
afterAll(async () => {
  await rm(root, { recursive: true, force: true });
});

describe('astSearch native contracts and executable continuations', () => {
  it('rejects mixed operations and ambiguous structural patterns', () => {
    expect(
      AstSearchQuerySchema.safeParse({
        operation: 'match',
        path: root,
        pattern: '$A',
        rule: 'kind: identifier',
      }).success
    ).toBe(false);
    expect(
      AstSearchQuerySchema.safeParse({
        operation: 'match',
        path: root,
        searchText: 'foo',
      }).success
    ).toBe(false);
    expect(
      AstSearchQuerySchema.safeParse({
        operation: 'tree',
        treeKind: 'syntax',
        path: root,
        names: ['*.ts'],
      }).success
    ).toBe(false);
  });
  it('pages file discovery without losing paths', async () => {
    const files = await pages(
      {
        operation: 'files',
        path: root,
        entryType: 'f',
        names: ['*.ts'],
        pageSize: 1,
        sort: 'path',
      },
      'files'
    );
    expect(files.map(file => file.path)).toHaveLength(3);
    expect(new Set(files.map(file => file.path)).size).toBe(3);
  });
  it('pages syntax nodes with stable ids, parents, and complete coverage', async () => {
    const query = {
      operation: 'tree',
      treeKind: 'syntax',
      path: join(root, 'a.ts'),
    };
    const full = await run({ ...query, nodeLimit: 1000 });
    const nodes = await pages({ ...query, nodeLimit: 3 }, 'nodes');
    expect(nodes).toEqual(full.nodes);
    expect(nodes.some(node => node.parentId !== undefined)).toBe(true);
  });
  it('restarts syntax pagination after source changes', async () => {
    const path = join(root, 'mutable.ts');
    await writeFile(path, 'export const one = 1;\n');
    const first = await run({
      operation: 'tree',
      treeKind: 'syntax',
      path,
      nodeLimit: 2,
    });
    await writeFile(path, 'export const changed = 2;\n');
    const changed = await run(first.next.nextPage.query);
    expect(changed.errorCode).toBe('ast.snapshot.changed');
    expect(changed.nodes).toBeUndefined();
    expect(
      AstSearchQuerySchema.safeParse(changed.next.restart.query).success
    ).toBe(true);
    await rm(path);
  });
  it('pages symbol outlines with stable declaration identity', async () => {
    const query = { operation: 'symbols', path: root };
    const full = await run({ ...query, pageSize: 100 });
    const declarations = await pages({ ...query, pageSize: 1 }, 'declarations');
    expect(declarations).toEqual(full.declarations);
    expect(declarations.map(declaration => declaration.name)).toContain(
      'unused'
    );
    expect(full.complete).toBe(true);
    expect(full.terminalLimit).toBeUndefined();
  });

  it('does not treat syntax-only graph metadata as parser incompleteness', async () => {
    const path = join(root, 'syntax-only.rs');
    await writeFile(path, 'pub fn syntax_only() {}\n');
    const result = await run({ operation: 'symbols', path });
    expect(result.complete).toBe(true);
    expect(result.terminalLimit).toBeUndefined();
    expect(result.diagnostics).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          path: 'syntax-only.rs',
          message: expect.stringContaining('syntax-only'),
        }),
      ])
    );
  });
  it('reports structured native skips for directory symbol scans', async () => {
    const path = join(root, 'oversized-symbols.ts');
    await writeFile(path, Buffer.alloc(1_000_001, 'x'));

    const result = await run({ operation: 'symbols', path: root });

    expect(result.filesSkipped).toBe(1);
    expect(result.complete).toBe(false);
    expect(result.terminalLimit).toBe(true);
    expect(result.diagnostics).toContainEqual(
      expect.objectContaining({
        path: expect.stringMatching(/oversized-symbols\.ts$/),
        message:
          'graph.scan.fileTooLarge: file exceeds the graph scan byte limit',
      })
    );
    await rm(path);
  });
  it('bounds malformed directory fact payloads instead of throwing', async () => {
    const scan = vi
      .spyOn(contextUtils, 'scanGraphFacts')
      .mockResolvedValueOnce({
        schemaVersion: 1,
        candidatePaths: ['malformed.ts'],
        filesSkipped: 0,
        truncated: false,
        skipped: [],
        entries: [
          {
            relativePath: 'malformed.ts',
            factsJson: '{not-json',
            referenceCounts: [],
          },
        ],
      });

    const result = await run({ operation: 'symbols', path: root });

    expect(result.filesSkipped).toBe(1);
    expect(result.terminalLimit).toBe(true);
    expect(result.diagnostics).toContainEqual(
      expect.objectContaining({
        path: expect.stringMatching(/malformed\.ts$/),
        message: 'facts-decode-failed: native graph facts could not be decoded',
      })
    );
    scan.mockRestore();
  });
  it('rejects unsupported schemas for single-file symbol inspection', async () => {
    const extract = vi
      .spyOn(contextUtils, 'extractGraphFacts')
      .mockReturnValueOnce(JSON.stringify({ schemaVersion: 2 }));

    const result = await run({
      operation: 'symbols',
      path: join(root, 'a.ts'),
    });

    expect(result.filesSkipped).toBe(1);
    expect(result.terminalLimit).toBe(true);
    expect(result.diagnostics).toContainEqual(
      expect.objectContaining({
        path: expect.stringMatching(/a\.ts$/),
        message:
          'facts-schema-unsupported: unsupported graph-fact schema version: 2',
      })
    );
    extract.mockRestore();
  });
  it('restarts symbol pagination after source changes', async () => {
    const path = join(root, 'mutable-symbols.ts');
    await writeFile(
      path,
      'export function first() { return 1; }\nexport function second() { return 2; }\n'
    );
    const first = await run({ operation: 'symbols', path, pageSize: 1 });
    await writeFile(path, 'export function changed() { return 3; }\n');
    const changed = await run(first.next.nextPage.query);
    expect(changed.errorCode).toBe('ast.snapshot.changed');
    expect(changed.declarations).toBeUndefined();
    expect(
      AstSearchQuerySchema.safeParse(changed.next.restart.query).success
    ).toBe(true);
    await rm(path);
  });
  it('does not emit a continuation for unsupported direct symbol files', async () => {
    const path = join(root, 'unsupported.toml');
    await writeFile(path, 'name = "value"\n');
    const result = await run({ operation: 'symbols', path });
    expect(result.status).toBe('error');
    expect(result.errorCode).toBe('ast.symbols.unsupported');
    expect(result.next).toBeUndefined();
    expect(result.meta.diagnostics).toEqual({
      codes: ['ast.symbols.unsupported'],
    });
    expect(result.complete).toBeUndefined();
    await rm(path);
  });
  it('reports unsupported syntax as a capability error, not a terminal result limit', async () => {
    const path = join(root, 'unsupported.octocode_unknown');
    await writeFile(path, 'unknown source');
    const result = await run({ operation: 'tree', treeKind: 'syntax', path });
    expect(result.status).toBe('error');
    expect(result.diagnostics).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ code: 'syntaxTree.language.unsupported' }),
      ])
    );
    expect(result.meta.diagnostics).toEqual({ codes: ['ast.syntax.error'] });
    expect(result.complete).toBeUndefined();
    expect(result.terminalLimit).toBeUndefined();
    await rm(path);
  });
  it('requires an explicit grammar for directory matches', async () => {
    const result = await run({
      operation: 'match',
      path: root,
      pattern: '$A($$$B)',
    });
    expect(result.errorCode).toBe('ast.language.required');
  });
  it('keeps AST matching separate from lexical search', async () => {
    const result = await run({
      operation: 'match',
      path: root,
      langType: 'ts',
      pattern: 'b()',
      captureText: true,
    });
    expect(result.searchEngine).toBe('structural');
    expect(result.files).toHaveLength(2);
  });
  for (const analysis of [
    'dependencies',
    'dependents',
    'path',
    'cycles',
    'reachability',
    'deadCode',
  ]) {
    it(`preserves topology ${analysis}`, async () => {
      const query = {
        operation: 'topology',
        analysis,
        path: root,
        ...(['dependencies', 'dependents', 'path'].includes(analysis)
          ? { file: 'a.ts' }
          : {}),
        ...(analysis === 'path' ? { target: 'b.ts' } : {}),
        ...(['reachability', 'deadCode'].includes(analysis)
          ? { entrypoints: ['a.ts'], includeTests: false }
          : {}),
      };
      const result = await run(query);
      expect(result.status).not.toBe('error');
      expect(result.operation).toBe('topology');
      expect(result.analysis).toBe(analysis);
      expect(result.meta.evidence.kind).toBe('syntactic');
      for (const next of Object.values(result.next ?? {}) as any[]) {
        if (next.tool === 'astSearch')
          expect(AstSearchQuerySchema.safeParse(next.query).success).toBe(true);
      }
    });
  }
  it('preserves topology scope and replays the public page union', async () => {
    const query = {
      operation: 'topology',
      analysis: 'reachability',
      path: root,
      entrypoints: ['a.ts'],
      includeTests: false,
      excludeDir: ['ignored'],
      maxFiles: 50,
      limit: 50,
      pageSize: 1,
      diagnosticPage: 1,
      diagnosticPageSize: 1,
      rustWorkspace: 'syntax',
    };
    const first = await run(query);
    expect(first.status).not.toBe('error');
    expect(first.results).toBeDefined();
    expect(first.next?.nextPage?.tool).toBe('astSearch');
    expect(
      AstSearchQuerySchema.safeParse(first.next.nextPage.query).success
    ).toBe(true);
    expect(first.next.nextPage.query).toMatchObject({
      operation: 'topology',
      analysis: 'reachability',
      entrypoints: ['a.ts'],
      includeTests: false,
      excludeDir: ['ignored'],
      maxFiles: 50,
      limit: 50,
      rustWorkspace: 'syntax',
    });
    const results = await pages(query, 'results');
    expect(results.length).toBeGreaterThan(0);
    expect(new Set(results.map(result => result.file)).size).toBe(
      results.length
    );
  });
});
