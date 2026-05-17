/**
 * E2E sanity script for verbosity:"ultra" on the LIVE octocode workspace.
 *
 * Imports each tool's real handler from src/ (the code on disk that the
 * MCP server will run after `yarn build`), calls it against this very
 * monorepo, and prints the byte payload for `default` vs `ultra`.
 *
 * Run from the package directory:
 *   yarn vitest run scripts/check_verbosity_e2e.ts --no-coverage
 *
 * (Run via vitest so we inherit the ts/esm pipeline already in place.)
 */

import { describe, it, expect, vi } from 'vitest';

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
// exhaustively covered in tests/scheme/verbosity_ultra.test.ts. Here we just
// confirm the public handler accepts `verbosity` without rejection.
const { applyFindReferencesVerbosity } =
  await import('../../src/tools/lsp_find_references/lsp_find_references.js');
const { applyGotoDefinitionVerbosity } =
  await import('../../src/tools/lsp_goto_definition/execution.js');
const { applyCallHierarchyVerbosity } =
  await import('../../src/tools/lsp_call_hierarchy/callHierarchy.js');

const WORKSPACE = '/Users/guybary/Documents/octocode-mcp/packages/octocode-mcp';

function payload(obj: unknown): string {
  return JSON.stringify(obj);
}

function reportSavings(name: string, before: unknown, after: unknown): void {
  const b = payload(before).length;
  const a = payload(after).length;
  const saved = ((1 - a / b) * 100).toFixed(1);
  console.log(
    `  ${name.padEnd(28)}  default=${b.toString().padStart(7)}B  ultra=${a.toString().padStart(6)}B  saved=${saved}%`
  );
}

describe('E2E: verbosity:"ultra" on real handlers', () => {
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
    const ultra: any = await findFiles({ ...base, verbosity: 'ultra' });
    reportSavings('localFindFiles', def, ultra);
    expect(def.status, JSON.stringify(def).slice(0, 400)).toBe('hasResults');
    expect(ultra.status).toBe('hasResults');
    expect(ultra.files).toEqual([]);
    expect((ultra.hints ?? []).join('\n')).toMatch(/files in \d+ dirs/);
    expect((ultra.hints ?? []).join('\n').toLowerCase()).toMatch(/drill-back/);
  });

  it('localViewStructure — drops entries[], emits summary', async () => {
    const base: any = {
      id: 'e2e',
      researchGoal: 'sanity',
      reasoning: 'check',
      path: `${WORKSPACE}/src/tools`,
    };
    const def = await viewStructure(base);
    const ultra = await viewStructure({ ...base, verbosity: 'ultra' });
    reportSavings('localViewStructure', def, ultra);
    expect(def.status).toBe('hasResults');
    expect(ultra.status).toBe('hasResults');
    expect(ultra.entries).toEqual([]);
    expect((ultra.hints ?? []).join('\n').toLowerCase()).toMatch(/drill-back/);
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
    const ultra = await fetchContent({ ...base, verbosity: 'ultra' });
    reportSavings('localGetFileContent', def, ultra);
    expect(def.status).toBe('hasResults');
    expect(ultra.status).toBe('hasResults');
    expect(ultra.content).toBe('');
    const blob = (ultra.hints ?? []).join('\n');
    expect(blob).toMatch(/ripgrepResultBuilder\.ts:/);
    expect(blob).toMatch(/~\d+ tokens raw/);
    expect(blob.toLowerCase()).toMatch(/drill-back/);
  });

  it('lspGotoDefinition (helper) — collapses location to file:line:col', () => {
    const base: any = {
      status: 'hasResults',
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
    const ultra = applyGotoDefinitionVerbosity(base, {
      verbosity: 'ultra',
    } as any);
    reportSavings('lspGotoDefinition', def, ultra);
    expect(def.locations?.[0]?.content).toBe(base.locations[0].content);
    expect(ultra.locations?.[0]?.content).toBe('');
    expect((ultra.hints ?? []).join('\n')).toMatch(/file:.*\d+:\d+|\/repo/);
  });

  it('lspFindReferences (helper) — flat refs<500 and topFiles rollup>=500', () => {
    const small: any = {
      status: 'hasResults',
      locations: Array.from({ length: 50 }, (_, i) => ({
        uri: `/r/file${i % 4}.ts`,
        range: {
          start: { line: i, character: 0 },
          end: { line: i, character: 3 },
        },
      })),
    };
    const big: any = {
      status: 'hasResults',
      locations: Array.from({ length: 1000 }, (_, i) => ({
        uri: `/r/file${i % 8}.ts`,
        range: {
          start: { line: i, character: 0 },
          end: { line: i, character: 3 },
        },
      })),
    };
    const flatUltra = applyFindReferencesVerbosity(small, {
      verbosity: 'ultra',
    } as any);
    const rollupUltra = applyFindReferencesVerbosity(big, {
      verbosity: 'ultra',
    } as any);
    reportSavings('lspFindReferences(<500)', small, flatUltra);
    reportSavings('lspFindReferences(>=500)', big, rollupUltra);
    expect(flatUltra.locations).toEqual([]);
    expect((flatUltra.hints ?? []).join('\n')).toMatch(/50 refs in 4 files/);
    expect(rollupUltra.locations).toEqual([]);
    expect((rollupUltra.hints ?? []).join('\n')).toMatch(/top-20:/);
  });

  it('lspCallHierarchy (helper) — emits edges only', () => {
    const base: any = {
      status: 'hasResults',
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
    const ultra = applyCallHierarchyVerbosity(base, {
      direction: 'incoming',
      verbosity: 'ultra',
    } as any);
    reportSavings('lspCallHierarchy', def, ultra);
    expect(def.calls).toEqual(base.calls);
    expect(ultra.calls).toEqual([]);
    expect((ultra.hints ?? []).join('\n')).toMatch(/serve → doWork \(×2\)/);
  });

  it('localSearchCode — drops files[], emits "N matches in M files (top: …)"', async () => {
    const base: any = {
      id: 'e2e',
      researchGoal: 'sanity',
      reasoning: 'check',
      pattern: 'applyRipgrepVerbosity',
      path: `${WORKSPACE}/src`,
    };
    const def = await searchContentRipgrep(base);
    const ultra = await searchContentRipgrep({ ...base, verbosity: 'ultra' });
    reportSavings('localSearchCode', def, ultra);
    expect(def.status).toBe('hasResults');
    expect(ultra.status).toBe('hasResults');
    expect(ultra.files).toEqual([]);
    const blob = (ultra.hints ?? []).join('\n');
    expect(blob).toMatch(/\d+ matches in \d+ files/);
    expect(blob).toMatch(/top: .*ripgrepResultBuilder\.ts:/);
    expect(blob.toLowerCase()).toMatch(/drill-back/);
  });
});
