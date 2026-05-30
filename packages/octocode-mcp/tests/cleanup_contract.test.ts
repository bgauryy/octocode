/**
 * Cleanup contract — pins the no-fallback, no-redundancy invariants
 * agreed in the May-2026 audit:
 *
 *  - No grep fallback (bundled ripgrep is the only engine).
 *  - No dead estimator left behind (`estimateDirectoryStats`).
 *  - LSP tools use the pool, not the legacy spawn-per-request `createClient`.
 *  - No dangling prototype modules (`ripgrepStreamExecutor`,
 *    `spawnStream`, `gracefulDegradation`) with zero production consumers.
 *
 * If any of these come back, this test breaks loudly so the regression
 * gets a name in the failure trace.
 */
import { access } from 'fs/promises';
import { describe, expect, it } from 'vitest';

const ROOT = process.cwd();

async function fileExists(relative: string): Promise<boolean> {
  try {
    await access(`${ROOT}/${relative}`);
    return true;
  } catch {
    return false;
  }
}

const FILES_THAT_MUST_BE_GONE = [
  // Grep fallback was deleted — bundled ripgrep is the only engine.
  'src/commands/GrepCommandBuilder.ts',
  'tests/commands/GrepCommandBuilder.test.ts',
  // Dead prototype with zero production consumers.
  'src/tools/local_ripgrep/ripgrepStreamExecutor.ts',
  'tests/tools/ripgrep_stream_executor.test.ts',
  'src/utils/exec/spawnStream.ts',
  'tests/utils/spawn_stream_lines.test.ts',
  'src/utils/response/gracefulDegradation.ts',
  'tests/utils/graceful_degradation.test.ts',
  // Stranded TYPE_TO_EXTENSIONS lookup — only consumer was GrepCommandBuilder.
  'src/utils/file/types.ts',
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
    file: 'src/tools/lsp_find_references/lspReferencesCore.ts',
    banned: /\bcreateClient\b/,
    reason: 'LSP tools must use acquirePooledClient, not createClient',
  },
  {
    file: 'src/tools/lsp_call_hierarchy/callHierarchyLsp.ts',
    banned: /\bcreateClient\b/,
    reason: 'LSP tools must use acquirePooledClient, not createClient',
  },
  {
    file: 'src/tools/lsp_goto_definition/execution.ts',
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
      const source = await readFile(`${ROOT}/${target.file}`, 'utf-8');
      expect(source).not.toMatch(target.banned);
    });
  }

  it('LSP pool is the only client factory exported from manager.ts', async () => {
    const { readFile } = await import('fs/promises');
    const source = await readFile(`${ROOT}/src/lsp/manager.ts`, 'utf-8');
    expect(source).toMatch(/acquirePooledClient/);
    expect(source).not.toMatch(/^export\s+(async\s+)?function\s+createClient/m);
  });

  it("REQUIRED_COMMANDS no longer includes 'grep'", async () => {
    const { REQUIRED_COMMANDS } =
      await import('../src/utils/exec/commandAvailability.js');
    expect(
      Object.prototype.hasOwnProperty.call(REQUIRED_COMMANDS, 'grep')
    ).toBe(false);
  });

  it('callHierarchyPatterns ripgrep args stay inside the security allow-list', async () => {
    // Regression: searchWithRipgrep used `--line-number` and `-e` which
    // are NOT in RG_ALLOWED_FLAGS, so every pattern-fallback call hierarchy
    // failed with "rg option '--line-number' is not allowed".
    const { readFile } = await import('fs/promises');
    const source = await readFile(
      `${ROOT}/src/tools/lsp_call_hierarchy/callHierarchyPatterns.ts`,
      'utf-8'
    );
    expect(
      source,
      'searchWithRipgrep must use -n (short form is the only one on the allow-list)'
    ).not.toMatch(/['"]--line-number['"]/);
    expect(
      source,
      'searchWithRipgrep must pass pattern positionally (-e is not allow-listed)'
    ).not.toMatch(/['"]-e['"]/);
  });

  it('callHierarchyPatterns error response stays inside the closed error.data schema', async () => {
    // Regression: error responses leaked item/direction/depth/lspMode,
    // which violate the closed `error.data` schema and surface as MCP
    // -32602 "Structured content does not match the tool's output schema".
    const { readFile } = await import('fs/promises');
    const source = await readFile(
      `${ROOT}/src/tools/lsp_call_hierarchy/callHierarchyPatterns.ts`,
      'utf-8'
    );
    // Bound the slice to the single object literal that owns the
    // `status: 'error'` line: from "{" up to the matching closing "};".
    // Greedy ${[\s\S]*?\};} hits the nearest close, which is exactly the
    // error-response object literal we want to inspect.
    const errorObjectMatch = source.match(
      /return\s*\{[\s\S]*?status:\s*['"]error['"][\s\S]*?\};/
    );
    expect(errorObjectMatch, 'error response object not found').toBeTruthy();
    const block = errorObjectMatch![0];
    expect(
      block,
      'error.data must not carry hasResults context fields'
    ).not.toMatch(/^\s*item:/m);
    expect(block).not.toMatch(/^\s*direction:/m);
    expect(block).not.toMatch(/^\s*depth,/m);
    expect(block).not.toMatch(/^\s*lspMode:/m);
  });

  it('LSP tools never tag lspMode on error responses (closed error.data schema)', async () => {
    // Regression: every LSP tool unconditionally appended `lspMode: 'fallback'`
    // to the result of its pattern-fallback path. When that path returned
    // status: 'error' (e.g. ripgrep flag rejected), MCP failed validation
    // with "Structured content does not match the tool's output schema".
    //
    // Contract: each tool MUST guard the lspMode injection with a status check.
    const { readFile } = await import('fs/promises');
    const files = [
      'src/tools/lsp_call_hierarchy/callHierarchy.ts',
      'src/tools/lsp_goto_definition/execution.ts',
      'src/tools/lsp_find_references/lsp_find_references.ts',
    ];
    for (const file of files) {
      const src = await readFile(`${ROOT}/${file}`, 'utf-8');
      // Must reference `status === 'error'` near where `lspMode: 'fallback'`
      // is built, proving the guard exists.
      expect(
        src,
        `${file} must guard lspMode injection on status==='error'`
      ).toMatch(/status\s*===\s*['"]error['"]/);
    }
  });

  it('structured pagination never injects outputPagination into error/empty data', async () => {
    // Regression: lspCallHierarchy (and any other tool) fails MCP output
    // validation with `unrecognized_keys: ["outputPagination"]` whenever a
    // big error/empty response triggers the pagination wrapper. The error
    // and empty branches use strict schemas without `outputPagination`.
    //
    // Under the lean contract, success is signaled by ABSENT status — so
    // both pagination entrypoints MUST guard via `status !== undefined`
    // (i.e. skip empty/error branches).
    const { readFile } = await import('fs/promises');
    const source = await readFile(
      `${ROOT}/src/utils/response/structuredPagination.ts`,
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
    // Regression guard for: "Command '/.../bin/rg' is not allowed".
    // Without this, every real MCP localSearchCode call 500s while
    // unit tests pass (because they mock safeExec).
    const { validateCommand } =
      await import('octocode-security-utils/commandValidator');
    const { resolveRipgrepBinary } =
      await import('../src/utils/exec/ripgrepBinary.js');
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
