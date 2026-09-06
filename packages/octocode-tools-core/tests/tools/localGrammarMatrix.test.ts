import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import {
  extractGraphFacts,
  extractSignatures,
  getSupportedGraphFactExtensions,
  getSupportedSignatureExtensions,
  getSupportedStructuralExtensions,
  structuralSearchDetailed,
} from '@octocodeai/octocode-engine';
import { executeDirectTool } from '../../src/tools/directToolCatalog.exec.js';
import { findDirectToolDefinition } from '../../src/tools/directToolCatalog/toolCatalogDefinitions.js';

import { grammarFixtures as fixtures } from '../fixtures/grammarFixtures.js';
const structuralExtensions = getSupportedStructuralExtensions();
const signatureExtensions = new Set(getSupportedSignatureExtensions());
const graphExtensions = new Set(getSupportedGraphFactExtensions());
const cases = structuralExtensions.map(extension => ({
  extension,
  source: fixtures.find(fixture => fixture.extensions.includes(extension))
    ?.source,
}));

type Continuation = { tool: string; query: Record<string, unknown> };
type Row = {
  status?: string;
  meta?: { diagnostics?: { partial?: boolean } };
  data: {
    content?: string;
    files?: Array<{ path: string }>;
    results?: Array<{ file: string }>;
    summary?: { reachableCount: number; unreachableCount: number };
    coverage?: {
      diagnostics: Array<{ file: string; code: string }>;
      diagnosticsPagination?: { totalEntries: number };
    };
    terminalLimit?: boolean;
    partialReasons?: string[];
    stats?: {
      totalStructuralMatches?: number;
      totalOccurrences?: number;
      filesMatched?: number;
    };
    next?: Record<string, Continuation>;
  };
};
async function run(tool: string, query: Record<string, unknown>): Promise<Row> {
  const parsed = findDirectToolDefinition(tool)?.schema.safeParse(query);
  expect(
    parsed?.success,
    JSON.stringify({
      query,
      issues: parsed?.success ? [] : parsed?.error.issues,
    })
  ).toBe(true);
  const result = await executeDirectTool(tool, { queries: [query] });
  const row = (result.structuredContent as { results: Row[] }).results[0]!;
  expect(row.status, JSON.stringify(row)).not.toBe('error');
  return row;
}

describe('production grammar matrix through the native and public tool boundaries', () => {
  let root = '';
  beforeAll(async () => {
    const parent = join(process.cwd(), '.octocode', 'tmp');
    await mkdir(parent, { recursive: true });
    root = await mkdtemp(join(parent, 'grammar-acceptance-'));
    for (const { extension, source } of cases) {
      expect(source, `missing .${extension} fixture`).toBeDefined();
      await writeFile(join(root, `fixture.${extension}`), source!);
    }
  });
  afterAll(async () => {
    if (root) await rm(root, { recursive: true, force: true });
  });

  it('covers the complete runtime capability inventory without duplicate extensions', () => {
    expect(structuralExtensions.length).toBeGreaterThan(0);
    expect(new Set(structuralExtensions).size).toBe(
      structuralExtensions.length
    );
    for (const extension of [...signatureExtensions, ...graphExtensions])
      expect(structuralExtensions).toContain(extension);
  });

  describe.each(cases)('.$extension', ({ extension, source }) => {
    it('matches real syntax through the native AST boundary', () => {
      const result = structuralSearchDetailed(
        source!,
        `fixture.${extension}`,
        source!.trim(),
        null
      );
      expect(result.status, JSON.stringify(result)).toBe('ok');
      expect(result.diagnostics).toEqual([]);
      expect(result.matches.length).toBeGreaterThan(0);
      expect(result.matches.some(match => match.text.includes('target'))).toBe(
        true
      );
    });

    it('honors advertised outline and graph support', () => {
      const path = `fixture.${extension}`;
      const outline = extractSignatures(source!, path);
      if (signatureExtensions.has(extension)) {
        expect(outline, `.${extension} outline`).toContain('target');
        expect(outline).not.toContain('body_marker');
      } else expect(outline).toBeNull();
      const graph = extractGraphFacts(source!, path);
      if (graphExtensions.has(extension)) {
        expect(graph).not.toBeNull();
        const facts = JSON.parse(graph!);
        expect(
          facts.declarations.some(
            (declaration: { name: string }) => declaration.name === 'target'
          ),
          graph!
        ).toBe(true);
      } else expect(graph).toBeNull();
    });

    if (signatureExtensions.has(extension)) {
      it('captures declaration identity with a structural metavariable', async () => {
        const pattern = source!.trim().replace('target', '$NAME');
        const native = structuralSearchDetailed(
          source!,
          `fixture.${extension}`,
          pattern,
          null
        );
        expect(native.status, JSON.stringify(native)).toBe('ok');
        expect(
          native.matches.some(match => match.metavars.NAME?.includes('target')),
          JSON.stringify(native)
        ).toBe(true);
        const row = await run('localSearch', {
          operation: 'structural',
          path: join(root, `fixture.${extension}`),
          pattern,
          captureText: true,
        });
        expect(JSON.stringify(row.data)).toContain('"NAME":["target"]');
      });
    }

    it('searches AST and text then reads exact, standard and symbol content', async () => {
      const path = join(root, `fixture.${extension}`);
      for (const operation of ['text', 'structural']) {
        const query =
          operation === 'text'
            ? { operation, path, searchText: 'target', regex: 'fixed' }
            : { operation, path, pattern: source!.trim() };
        const row = await run('localSearch', query);
        expect(row.status, JSON.stringify(row)).not.toBe('empty');
        expect(JSON.stringify(row.data)).toContain('target');
      }
      for (const minify of ['none', 'standard', 'symbols']) {
        const row = await run('localGetFileContent', {
          path,
          fullContent: true,
          minify,
        });
        expect(row.data.content).toContain('target');
        if (minify === 'none') expect(row.data.content).toBe(source);
        if (minify === 'symbols' && signatureExtensions.has(extension))
          expect(row.data.content).not.toContain('body_marker');
      }
    });

    it.each(['content', 'files', 'countMatches'])(
      'preserves AST matches with resultView:%s',
      async resultView => {
        const row = await run('localSearch', {
          operation: 'structural',
          path: join(root, `fixture.${extension}`),
          pattern: source!.trim(),
          resultView,
        });
        expect(row.data.stats?.totalStructuralMatches).toBeGreaterThan(0);
        expect(row.data.files).toHaveLength(1);
      }
    );

    it.each([
      'paginated',
      'discovery',
      'detailed',
      'content',
      'files',
      'countLines',
      'countMatches',
      'matchOnly',
    ])('preserves lexical matches with resultView:%s', async resultView => {
      const row = await run('localSearch', {
        operation: 'text',
        path: join(root, `fixture.${extension}`),
        searchText: 'target',
        regex: 'fixed',
        resultView,
      });
      expect(row.data.stats?.totalOccurrences).toBe(1);
      expect(row.data.stats?.filesMatched).toBe(1);
    });

    it('completes exact reads by executing every character continuation', async () => {
      let query: Record<string, unknown> | undefined = {
        path: join(root, `fixture.${extension}`),
        fullContent: true,
        minify: 'none',
        charLength: 20,
      };
      let content = '';
      let pages = 0;
      while (query) {
        expect(++pages).toBeLessThan(30);
        const row = await run('localGetFileContent', query);
        content += row.data.content;
        query = row.data.next?.continueChars?.query;
      }
      expect(content).toBe(source);
    });
  });

  it('recovers all grammar fixture files by executing public file pagination', async () => {
    let query: Record<string, unknown> | undefined = {
      operation: 'files',
      path: root,
      entryType: 'f',
      pageSize: 7,
      sort: 'path',
    };
    const paths: string[] = [];
    let pages = 0;
    while (query) {
      expect(++pages).toBeLessThan(20);
      const row = await run('localSearch', query);
      paths.push(...row.data.files!.map(file => file.path));
      const next = row.data.next?.nextPage;
      if (next) expect(row.meta?.diagnostics?.partial).toBe(true);
      query = next?.query;
    }
    expect(paths.length).toBe(structuralExtensions.length);
    expect(new Set(paths).size).toBe(structuralExtensions.length);
  });

  it.each(['text', 'structural'])(
    'recovers every grammar in a mixed directory via %s continuations',
    async operation => {
      let query: Record<string, unknown> | undefined = {
        operation,
        path: root,
        ...(operation === 'text'
          ? { searchText: 'target', regex: 'fixed' }
          : { rule: 'rule:\n  regex: target' }),
        resultView: 'files',
        noIgnore: true,
        hidden: true,
        pageSize: 7,
        maxFiles: 7,
        sort: 'path',
      };
      const paths = new Set<string>();
      let pages = 0;
      while (query) {
        expect(++pages).toBeLessThan(30);
        const row = await run('localSearch', query);
        for (const file of row.data.files ?? []) paths.add(file.path);
        const next = row.data.next?.nextPage ?? row.data.next?.expandScan;
        if (next) expect(row.meta?.diagnostics?.partial).toBe(true);
        query = next?.query;
      }
      expect([...paths].sort()).toEqual(
        structuralExtensions.map(extension => `fixture.${extension}`).sort()
      );
    }
  );

  it('exposes every graph-capable extension as a reachable public graph node', async () => {
    const entrypoints = [...graphExtensions].map(
      extension => `fixture.${extension}`
    );
    const row = await run('localAnalyzeGraph', {
      operation: 'reachability',
      path: root,
      entrypoints,
      pageSize: 50,
    });
    expect(row.data.results, JSON.stringify(row)).toBeDefined();
    expect(row.data.results!.map(result => result.file).sort()).toEqual(
      entrypoints.sort()
    );
    expect(row.data.summary).toMatchObject({
      reachableCount: graphExtensions.size,
      unreachableCount: 0,
    });
    // A parser capability is not an import linker. Unsupported languages stay
    // explicit even when every file is a caller-supplied root.
    expect(row.data.terminalLimit).toBe(true);
    expect(row.data.partialReasons).toContain('unsupportedLinking');
    const diagnostics = [...row.data.coverage!.diagnostics];
    let next = row.data.next?.nextDiagnostics;
    let pages = 1;
    while (next) {
      expect(++pages).toBeLessThan(10);
      const page = await run(next.tool, next.query);
      diagnostics.push(...page.data.coverage!.diagnostics);
      next = page.data.next?.nextDiagnostics;
    }
    expect(diagnostics.length).toBe(
      row.data.coverage!.diagnosticsPagination!.totalEntries
    );
    expect(
      new Set(diagnostics.map(item => `${item.file}:${item.code}`)).size
    ).toBe(diagnostics.length);
  });
});
