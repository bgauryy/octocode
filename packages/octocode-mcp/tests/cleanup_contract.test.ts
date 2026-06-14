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

  it("REQUIRED_COMMANDS no longer includes 'grep'", async () => {
    const { REQUIRED_COMMANDS } =
      await import('../../octocode-tools-core/src/utils/exec/commandAvailability.js');
    expect(
      Object.prototype.hasOwnProperty.call(REQUIRED_COMMANDS, 'grep')
    ).toBe(false);
  });

  it('ripgrep resolver does not fall back to PATH rg', async () => {
    const source = await import('fs/promises').then(fs =>
      fs.readFile(`${CORE_ROOT}/src/utils/exec/ripgrepBinary.ts`, 'utf-8')
    );
    expect(source).not.toMatch(/RIPGREP_PATH_FALLBACK/);
    expect(source).not.toMatch(/return ['"]rg['"]/);
  });

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

  it('structured pagination never injects outputPagination into error/empty data', async () => {
    const { readFile } = await import('fs/promises');
    const source = await readFile(
      `${CORE_ROOT}/src/utils/response/structuredPagination.ts`,
      'utf-8'
    );

    const applyMatch = source.match(
      /export\s+function\s+applyQueryOutputPagination\b[\s\S]*?\n\}/
    );
    expect(applyMatch, 'applyQueryOutputPagination not found').toBeTruthy();
    expect(
      applyMatch![0],
      'applyQueryOutputPagination must guard on success (status === undefined)'
    ).toMatch(/status\s*!==\s*undefined/);

    const flatMatch = source.match(
      /function\s+paginateFlatQueryResult\b[\s\S]*?\n\}/
    );
    expect(flatMatch, 'paginateFlatQueryResult not found').toBeTruthy();
    expect(
      flatMatch![0],
      'paginateFlatQueryResult must guard on success (status === undefined)'
    ).toMatch(/status\s*!==\s*undefined/);
  });

  it('security validator accepts the bundled rg absolute path', async () => {
    const { validateCommand } =
      await import('octocode-security/commandValidator');
    const { resolveRipgrepBinary } =
      await import('../../octocode-tools-core/src/utils/exec/ripgrepBinary.js');
    const binary = resolveRipgrepBinary();
    const validation = validateCommand(binary, [
      '-n',
      '--column',
      '-S',
      '--color',
      'never',
      '--sort',
      'path',
      '--',
      'pattern',
      '/tmp',
    ]);
    expect(validation).toEqual({ isValid: true });
  });
});
