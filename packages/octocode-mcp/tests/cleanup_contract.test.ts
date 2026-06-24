import { access } from 'fs/promises';
import { resolve } from 'path';
import { describe, expect, it } from 'vitest';

const ROOT = process.cwd();
const CORE_ROOT = resolve(ROOT, '../octocode-tools-core');

async function fileExists(relative: string): Promise<boolean> {
  try {
    await access(`${ROOT}/${relative}`);
    return true;
  } catch {
    return false;
  }
}

const FILES_THAT_MUST_BE_GONE = [
  'src/commands/GrepCommandBuilder.ts',
  'tests/commands/GrepCommandBuilder.test.ts',
  'src/tools/local_ripgrep/ripgrepStreamExecutor.ts',
  'tests/tools/ripgrep_stream_executor.test.ts',
  'src/utils/exec/spawnStream.ts',
  'tests/utils/spawn_stream_lines.test.ts',
  'src/utils/response/gracefulDegradation.ts',
  'tests/utils/graceful_degradation.test.ts',
  'src/utils/file/types.ts',
  'src/tools/lsp_call_hierarchy/callHierarchyPatterns.ts',
  'src/tools/lsp_find_references/lspReferencesPatterns.ts',
  'src/tools/lsp_find_references/lspReferencesProcess.ts',
  'src/scheme/lspSchemaOverlay.ts',
  'src/scheme/lspOutputSchemaOverlay.ts',
  'src/tools/lsp_goto_definition/execution.ts',
  'src/tools/lsp_goto_definition/hints.ts',
  'src/tools/lsp_goto_definition/lsp_goto_definition.ts',
  'src/tools/lsp_find_references/execution.ts',
  'src/tools/lsp_find_references/hints.ts',
  'src/tools/lsp_find_references/lsp_find_references.ts',
  'src/tools/lsp_find_references/lspReferencesCore.ts',
  'src/tools/lsp_find_references/register.ts',
  'src/tools/lsp_call_hierarchy/callHierarchy.ts',
  'src/tools/lsp_call_hierarchy/callHierarchyLsp.ts',
  'src/tools/lsp_call_hierarchy/register.ts',
  'src/lsp',
  'tests/lsp',
];

// Files in octocode-tools-core removed when ripgrep moved in-process.
const CORE_FILES_THAT_MUST_BE_GONE = [
  'src/commands/RipgrepCommandBuilder.ts',
  'src/utils/exec/ripgrepBinary.ts',
  'src/utils/exec/commandAvailability.ts',
  'src/tools/local_ripgrep/grepFallbackExecutor.ts',
  'src/tools/local_ripgrep/ripgrepParser.ts',
  // Dead pagination engines removed: only two live pagination flows remain
  // (utils/pagination/core.ts + utils/response/bulk.ts).
  'src/utils/response/structuredPagination.ts',
  'src/utils/pagination/outputSizeLimit.ts',
];

const SOURCE_FILES_THAT_MUST_NOT_REFERENCE: Array<{
  file: string;
  banned: RegExp;
  reason: string;
}> = [
  {
    file: 'src/tools/local_ripgrep/ripgrepExecutor.ts',
    banned: /\bexecuteGrepSearch\b/,
    reason: 'grep fallback was removed',
  },
  {
    file: 'src/tools/local_ripgrep/ripgrepExecutor.ts',
    banned: /\bestimateDirectoryStats\b/,
    reason: 'pre-flight estimator was removed',
  },
  {
    file: 'src/tools/local_ripgrep/searchContentRipgrep.ts',
    banned: /\bexecuteGrepSearch\b/,
    reason: 'grep fallback was removed',
  },
  {
    file: 'src/tools/lsp/semantic_content/execution.ts',
    banned: /\bcreateClient\b/,
    reason: 'LSP tools must use acquirePooledClient, not createClient',
  },
];

describe('Cleanup contract — no fallbacks, no redundancy', () => {
  for (const path of FILES_THAT_MUST_BE_GONE) {
    it(`removes the dead/redundant file ${path}`, async () => {
      expect(await fileExists(path)).toBe(false);
    });
  }

  for (const target of SOURCE_FILES_THAT_MUST_NOT_REFERENCE) {
    it(`${target.file} no longer references the removed API (${target.reason})`, async () => {
      const { readFile } = await import('fs/promises');
      const source = await readFile(`${CORE_ROOT}/${target.file}`, 'utf-8');
      expect(source).not.toMatch(target.banned);
    });
  }

  it('MCP no longer owns an LSP runtime shim', async () => {
    expect(await fileExists('src/lsp')).toBe(false);
    expect(await fileExists('tests/lsp')).toBe(false);
  });

  // Ripgrep moved in-process into the native engine: the rg binary resolver,
  // the bundled-binary availability check, the command builder and the grep
  // fallback are all gone. Lock those deletions in.
  for (const path of CORE_FILES_THAT_MUST_BE_GONE) {
    it(`removes the obsolete ripgrep-binary file octocode-tools-core/${path}`, async () => {
      let exists = true;
      try {
        await access(`${CORE_ROOT}/${path}`);
      } catch {
        exists = false;
      }
      expect(exists).toBe(false);
    });
  }

  it('LSP tools never assign lspMode into result objects (LSP-only, absent ≡ semantic)', async () => {
    const { readFile } = await import('fs/promises');
    const files = ['src/tools/lsp/semantic_content/execution.ts'];
    for (const file of files) {
      const src = await readFile(`${CORE_ROOT}/${file}`, 'utf-8');
      const assignPattern = /lspMode\s*:\s*(?!_)/g;
      const matches = [...src.matchAll(assignPattern)].filter(
        m =>
          !src
            .slice(Math.max(0, (m.index ?? 0) - 10), m.index ?? 0)
            .includes('{')
      );
      expect(
        matches,
        `${file} must not emit lspMode into results`
      ).toHaveLength(0);
    }
  });

  it('searchContentRipgrep no longer checks rg availability or falls back to grep', async () => {
    const { readFile } = await import('fs/promises');
    const source = await readFile(
      `${CORE_ROOT}/src/tools/local_ripgrep/searchContentRipgrep.ts`,
      'utf-8'
    );
    expect(source).not.toMatch(/checkCommandAvailability/);
    expect(source).not.toMatch(/grepFallback/i);
  });

  it('the executor calls the native in-process ripgrep, not a spawned binary', async () => {
    const { readFile } = await import('fs/promises');
    const source = await readFile(
      `${CORE_ROOT}/src/tools/local_ripgrep/ripgrepExecutor.ts`,
      'utf-8'
    );
    expect(source).toMatch(/searchRipgrep/);
    expect(source).not.toMatch(/RipgrepCommandBuilder/);
    expect(source).not.toMatch(/resolveRipgrepBinary/);
  });
});
