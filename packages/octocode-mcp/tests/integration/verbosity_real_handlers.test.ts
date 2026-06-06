import { describe, it, expect, vi } from 'vitest';
import { fileURLToPath } from 'url';
import path from 'path';

vi.unmock('child_process');
vi.doUnmock('child_process');

const { findFiles } =
  await import('../../src/tools/local_find_files/findFiles.js');
const { fetchContent } =
  await import('../../src/tools/local_fetch_content/fetchContent.js');
const { viewStructure } =
  await import('../../src/tools/local_view_structure/local_view_structure.js');

const { applyFindReferencesVerbosity } =
  await import('../../src/tools/lsp_find_references/lsp_find_references.js');
const { applyGotoDefinitionVerbosity } =
  await import('../../src/tools/lsp_goto_definition/execution.js');
const { applyCallHierarchyVerbosity } =
  await import('../../src/tools/lsp_call_hierarchy/callHierarchy.js');

const WORKSPACE = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../..'
);

describe('E2E: verbose boolean — data payload is preserved in both modes', () => {
  it('localFindFiles — verbose:false omits metadata; verbose:true includes it', async () => {
    const base: any = {
      id: 'e2e',
      researchGoal: 'sanity',
      reasoning: 'check',
      path: WORKSPACE,
      type: 'f',
      name: '*.ts',
    };
    const def: any = await findFiles({ ...base, verbose: false });
    const withMeta: any = await findFiles({ ...base, verbose: true });
    expect(def.status, JSON.stringify(def).slice(0, 400)).toBeUndefined();
    expect(withMeta.status).toBeUndefined();
    expect(def.files?.length).toBeGreaterThan(0);
    expect(withMeta.files?.length).toBe(def.files?.length);
    expect(def.files?.[0]?.path).toBeDefined();
    expect(withMeta.files?.[0]?.path).toBeDefined();
  });

  it('localViewStructure — verbose:false returns entries without metadata; verbose:true includes metadata', async () => {
    const base: any = {
      id: 'e2e',
      researchGoal: 'sanity',
      reasoning: 'check',
      path: `${WORKSPACE}/src/tools`,
    };
    const def = await viewStructure({ ...base, verbose: false });
    const withMeta = await viewStructure({ ...base, verbose: true });
    expect(def.status).toBeUndefined();
    expect(withMeta.status).toBeUndefined();
    expect(def.entries?.length).toBeGreaterThan(0);
    expect(withMeta.entries?.length).toBe(def.entries?.length);
    const defEntry = def.entries?.[0] as Record<string, unknown> | undefined;
    const metaEntry = withMeta.entries?.[0] as
      | Record<string, unknown>
      | undefined;
    expect(defEntry).toBeDefined();
    expect(metaEntry).toBeDefined();
    expect(defEntry?.['name']).toBeDefined();
    expect(metaEntry?.['name']).toBeDefined();
  });

  it('localGetFileContent — verbose:false and verbose:true return same content', async () => {
    const base: any = {
      id: 'e2e',
      researchGoal: 'sanity',
      reasoning: 'check',
      path: `${WORKSPACE}/src/tools/local_ripgrep/ripgrepResultBuilder.ts`,
    };
    const def = await fetchContent({ ...base, verbose: false });
    const withMeta = await fetchContent({ ...base, verbose: true });
    expect(def.status).toBeUndefined();
    expect(withMeta.status).toBeUndefined();
    expect(withMeta.content).toBe(def.content);
  });

  it('lspGotoDefinition (helper) — verbose:false preserves full location content', () => {
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
    const def = applyGotoDefinitionVerbosity(base, { verbose: false } as any);
    const withMeta = applyGotoDefinitionVerbosity(base, {
      verbose: true,
    } as any);
    expect(def.locations?.[0]?.content).toBe(base.locations[0].content);
    expect(withMeta.locations?.[0]?.content).toBe(base.locations[0].content);
  });

  it('lspFindReferences (helper) — verbose:false preserves full locations[]', () => {
    const locs = Array.from({ length: 50 }, (_, i) => ({
      uri: `/r/file${i % 4}.ts`,
      range: {
        start: { line: i, character: 0 },
        end: { line: i, character: 3 },
      },
    }));
    const result: any = { locations: locs };
    const def = applyFindReferencesVerbosity(result, {
      verbose: false,
    } as any);
    const withMeta = applyFindReferencesVerbosity(result, {
      verbose: true,
    } as any);
    expect(def.locations).toHaveLength(50);
    expect(withMeta.locations).toEqual(def.locations);
  });

  it('lspCallHierarchy (helper) — verbose:false preserves full calls[]', () => {
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
          ],
        },
        { from: { name: 'main' }, fromRanges: [{ start: { line: 1 } }] },
      ],
    };
    const def = applyCallHierarchyVerbosity(base, {
      direction: 'incoming',
      verbose: false,
    } as any);
    const withMeta = applyCallHierarchyVerbosity(base, {
      direction: 'incoming',
      verbose: true,
    } as any);
    expect(def.calls).toEqual(base.calls);
    expect(withMeta.calls).toEqual(base.calls);
  });
});
