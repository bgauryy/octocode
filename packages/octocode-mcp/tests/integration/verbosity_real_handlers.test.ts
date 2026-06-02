/**
 * E2E sanity script for verbosity:"concise" on the LIVE octocode workspace.
 *
 * Imports each tool's real handler from src/ (the code on disk that the
 * MCP server will run after `yarn build`), calls it against this very
 * monorepo, and prints the byte payload for `default` vs `concise`.
 *
 * Run from the package directory:
 *   yarn vitest run scripts/check_verbosity_e2e.ts --no-coverage
 *
 * (Run via vitest so we inherit the ts/esm pipeline already in place.)
 */

import { describe, it, expect, vi } from 'vitest';
import { fileURLToPath } from 'url';
import path from 'path';

// Vitest setup mocks child_process globally. For this e2e suite we need the
// real `spawn` so `find`, `rg`, etc. actually run against this workspace.
vi.unmock('child_process');
vi.doUnmock('child_process');

const { findFiles } =
  await import('../../src/tools/local_find_files/findFiles.js');
const { fetchContent } =
  await import('../../src/tools/local_fetch_content/fetchContent.js');
const { viewStructure } =
  await import('../../src/tools/local_view_structure/local_view_structure.js');
const { searchContentRipgrep } =
  await import('../../src/tools/local_ripgrep/searchContentRipgrep.js');

// LSP handlers — the helper functions we wired with verbosity. We can't run a
// real LSP server in vitest, but the verbosity transformers are pure and
// exhaustively covered in tests/scheme/verbosity_concise.test.ts. Here we just
// confirm the public handler accepts `verbosity` without rejection.
const { applyFindReferencesVerbosity } =
  await import('../../src/tools/lsp_find_references/lsp_find_references.js');
const { applyGotoDefinitionVerbosity } =
  await import('../../src/tools/lsp_goto_definition/execution.js');
const { applyCallHierarchyVerbosity } =
  await import('../../src/tools/lsp_call_hierarchy/callHierarchy.js');

// Derive WORKSPACE dynamically so the test works on any machine / CI runner.
// This file lives at tests/integration/, two levels below the package root.
const WORKSPACE = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../..'
);

function payload(obj: unknown): string {
  return JSON.stringify(obj);
}

function reportSavings(name: string, before: unknown, after: unknown): void {
  const b = payload(before).length;
  const a = payload(after).length;
  const saved = ((1 - a / b) * 100).toFixed(1);
  console.log(
    `  ${name.padEnd(28)}  default=${b.toString().padStart(7)}B  concise=${a.toString().padStart(6)}B  saved=${saved}%`
  );
}

describe('E2E: verbosity:"concise" on real handlers', () => {
  it('localFindFiles — drops files[], emits summary', async () => {
    const base: any = {
      id: 'e2e',
      researchGoal: 'sanity',
      reasoning: 'check',
      path: WORKSPACE,
      type: 'f',
      name: '*.ts',
    };
    const def: any = await findFiles(base);
    const concise: any = await findFiles({ ...base, verbosity: 'concise' });
    reportSavings('localFindFiles', def, concise);
    expect(def.status, JSON.stringify(def).slice(0, 400)).toBeUndefined();
    expect(concise.status).toBeUndefined();
    expect(concise.files).toEqual([]);
    expect((concise.hints ?? []).join('\n')).toMatch(/files in \d+ dirs/);
    expect((concise.hints ?? []).join('\n').toLowerCase()).not.toMatch(
      /drill-back|detail dropped/
    );
  });

  it('localViewStructure — drops entries[], emits summary', async () => {
    const base: any = {
      id: 'e2e',
      researchGoal: 'sanity',
      reasoning: 'check',
      path: `${WORKSPACE}/src/tools`,
    };
    const def = await viewStructure(base);
    const concise = await viewStructure({ ...base, verbosity: 'concise' });
    reportSavings('localViewStructure', def, concise);
    expect(def.status).toBeUndefined();
    expect(concise.status).toBeUndefined();
    expect(concise.entries).toEqual([]);
    expect((concise.hints ?? []).join('\n').toLowerCase()).not.toMatch(
      /drill-back|detail dropped/
    );
  });

  it('localGetFileContent — drops content (early-return path too), emits summary', async () => {
    // ripgrepResultBuilder.ts is ~8.8K → triggers the auto-pagination
    // earlyResult branch. This pins the fix that wraps the early-return path
    // with applyFetchContentVerbosity as well as the buildSuccessResult path.
    const base: any = {
      id: 'e2e',
      researchGoal: 'sanity',
      reasoning: 'check',
      path: `${WORKSPACE}/src/tools/local_ripgrep/ripgrepResultBuilder.ts`,
    };
    const def = await fetchContent(base);
    const concise = await fetchContent({ ...base, verbosity: 'concise' });
    reportSavings('localGetFileContent', def, concise);
    expect(def.status).toBeUndefined();
    expect(concise.status).toBeUndefined();
    // concise minifies (does not blank) — content kept but ≤ verbatim size.
    expect(concise.content).not.toBe('');
    expect((concise.content ?? '').length).toBeLessThanOrEqual(
      (def.content ?? '').length
    );
    const blob = (concise.hints ?? []).join('\n');
    expect(blob).toMatch(/ripgrepResultBuilder\.ts:/);
    expect(blob).toMatch(/tokens \(minified\)/);
    expect(blob.toLowerCase()).not.toMatch(/drill-back|detail dropped/);
  });

  it('lspGotoDefinition (helper) — collapses location to file:line:col', () => {
    const base: any = {
      locations: [
        {
          uri: '/repo/src/foo.ts',
          range: {
            start: { line: 11, character: 9 },
            end: { line: 11, character: 12 },
          },
          content: ' 12| export function foo() {}',
        },
      ],
      hints: ['baseline'],
    };
    const def = applyGotoDefinitionVerbosity(base, {} as any);
    const concise = applyGotoDefinitionVerbosity(base, {
      verbosity: 'concise',
    } as any);
    reportSavings('lspGotoDefinition', def, concise);
    expect(def.locations?.[0]?.content).toBe(base.locations[0].content);
    expect(concise.locations?.[0]?.content).toBe('');
    expect((concise.hints ?? []).join('\n')).toMatch(/file:.*\d+:\d+|\/repo/);
  });

  it('lspFindReferences (helper) — flat refs<500 and topFiles rollup>=500', () => {
    const small: any = {
      locations: Array.from({ length: 50 }, (_, i) => ({
        uri: `/r/file${i % 4}.ts`,
        range: {
          start: { line: i, character: 0 },
          end: { line: i, character: 3 },
        },
      })),
    };
    const big: any = {
      locations: Array.from({ length: 1000 }, (_, i) => ({
        uri: `/r/file${i % 8}.ts`,
        range: {
          start: { line: i, character: 0 },
          end: { line: i, character: 3 },
        },
      })),
    };
    const flatConcise = applyFindReferencesVerbosity(small, {
      verbosity: 'concise',
    } as any);
    const rollupConcise = applyFindReferencesVerbosity(big, {
      verbosity: 'concise',
    } as any);
    reportSavings('lspFindReferences(<500)', small, flatConcise);
    reportSavings('lspFindReferences(>=500)', big, rollupConcise);
    expect(flatConcise.locations).toEqual([]);
    expect((flatConcise.hints ?? []).join('\n')).toMatch(/50 refs in 4 files/);
    expect(rollupConcise.locations).toEqual([]);
    expect((rollupConcise.hints ?? []).join('\n')).toMatch(/top-20:/);
  });

  it('lspCallHierarchy (helper) — emits edges only', () => {
    const base: any = {
      direction: 'incoming',
      depth: 1,
      root: { symbol: { name: 'doWork' } },
      calls: [
        {
          from: { name: 'serve' },
          fromRanges: [
            {
              start: { line: 14, character: 0 },
              end: { line: 14, character: 5 },
            },
            {
              start: { line: 20, character: 0 },
              end: { line: 20, character: 5 },
            },
          ],
        },
        { from: { name: 'main' }, fromRanges: [{ start: { line: 1 } }] },
      ],
    };
    const def = applyCallHierarchyVerbosity(base, {
      direction: 'incoming',
    } as any);
    const concise = applyCallHierarchyVerbosity(base, {
      direction: 'incoming',
      verbosity: 'concise',
    } as any);
    reportSavings('lspCallHierarchy', def, concise);
    expect(def.calls).toEqual(base.calls);
    expect(concise.calls).toEqual([]);
    expect((concise.hints ?? []).join('\n')).toMatch(/serve → doWork \(×2\)/);
  });
});
