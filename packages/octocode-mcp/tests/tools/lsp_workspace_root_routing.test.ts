import { mkdtemp, mkdir, rm, writeFile } from 'fs/promises';
import os from 'os';
import path from 'path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

describe('LSP workspace root routing', () => {
  let tempDir: string;
  let configuredWorkspaceRoot: string;
  let inferredWorkspaceRoot: string;
  let externalFile: string;

  beforeEach(async () => {
    vi.resetModules();

    tempDir = await mkdtemp(path.join(os.homedir(), '.octocode-lsp-routing-'));
    configuredWorkspaceRoot = path.join(tempDir, 'configured-workspace');
    inferredWorkspaceRoot = path.join(tempDir, 'repo', 'node_modules', 'pkg');
    externalFile = path.join(inferredWorkspaceRoot, 'cli.js');

    await mkdir(configuredWorkspaceRoot, { recursive: true });
    await mkdir(inferredWorkspaceRoot, { recursive: true });
    await writeFile(path.join(inferredWorkspaceRoot, 'package.json'), '{}');
    await writeFile(externalFile, 'export const run = () => 1;\n');

    process.env.WORKSPACE_ROOT = configuredWorkspaceRoot;
  });

  afterEach(async () => {
    delete process.env.WORKSPACE_ROOT;
    vi.restoreAllMocks();
    await rm(tempDir, { recursive: true, force: true });
  });

  it('passes the inferred root to goto definition LSP checks and client creation', async () => {
    const managerModule = await import('../../src/lsp/manager.js');

    vi.spyOn(managerModule, 'isLanguageServerAvailable').mockResolvedValue(
      true
    );
    vi.spyOn(managerModule, 'acquirePooledClient').mockResolvedValue(null);

    const { executeGotoDefinition } =
      await import('../../src/tools/lsp_goto_definition/execution.js');

    await executeGotoDefinition({
      queries: [
        {
          uri: externalFile,
          symbolName: 'run',
          lineHint: 1,
          researchGoal: 'Find bundled definition',
          reasoning: 'Verify root routing',
        },
      ],
    });

    expect(managerModule.isLanguageServerAvailable).toHaveBeenCalledWith(
      externalFile,
      inferredWorkspaceRoot
    );
    expect(managerModule.acquirePooledClient).toHaveBeenCalledWith(
      inferredWorkspaceRoot,
      externalFile
    );
  });

  it('passes the inferred root through reference lookup and fallback search', async () => {
    const managerModule = await import('../../src/lsp/manager.js');
    const referencePatternsModule =
      await import('../../src/tools/lsp_find_references/lspReferencesPatterns.js');

    vi.spyOn(managerModule, 'isLanguageServerAvailable').mockResolvedValue(
      true
    );
    vi.spyOn(managerModule, 'acquirePooledClient').mockResolvedValue(null);
    vi.spyOn(
      referencePatternsModule,
      'findReferencesWithPatternMatching'
    ).mockResolvedValue({
      status: 'empty',
      locations: [],
      hints: [],
    });

    const { findReferences } =
      await import('../../src/tools/lsp_find_references/lsp_find_references.js');

    await findReferences({
      uri: externalFile,
      symbolName: 'run',
      lineHint: 1,
      researchGoal: 'Find bundled references',
      reasoning: 'Verify root routing',
    });

    expect(managerModule.isLanguageServerAvailable).toHaveBeenCalledWith(
      externalFile,
      inferredWorkspaceRoot
    );
    expect(managerModule.acquirePooledClient).toHaveBeenCalledWith(
      inferredWorkspaceRoot,
      externalFile
    );
    expect(
      referencePatternsModule.findReferencesWithPatternMatching
    ).toHaveBeenCalledWith(
      externalFile,
      inferredWorkspaceRoot,
      expect.objectContaining({ symbolName: 'run' })
    );
  });

  it('passes the inferred root to call hierarchy LSP and fallback paths', async () => {
    const managerModule = await import('../../src/lsp/manager.js');
    const callHierarchyPatternsModule =
      await import('../../src/tools/lsp_call_hierarchy/callHierarchyPatterns.js');

    vi.spyOn(managerModule, 'isLanguageServerAvailable').mockResolvedValue(
      true
    );
    vi.spyOn(managerModule, 'acquirePooledClient').mockResolvedValue(null);
    vi.spyOn(
      callHierarchyPatternsModule,
      'callHierarchyWithPatternMatching'
    ).mockResolvedValue({
      status: 'empty',
      direction: 'incoming',
      depth: 1,
      incomingCalls: [],
      hints: [],
    });

    const { processCallHierarchy } =
      await import('../../src/tools/lsp_call_hierarchy/callHierarchy.js');

    await processCallHierarchy({
      uri: externalFile,
      symbolName: 'run',
      lineHint: 1,
      direction: 'incoming',
      researchGoal: 'Find bundled callers',
      reasoning: 'Verify root routing',
    });

    expect(managerModule.isLanguageServerAvailable).toHaveBeenCalledWith(
      externalFile,
      inferredWorkspaceRoot
    );
    expect(managerModule.acquirePooledClient).toHaveBeenCalledWith(
      inferredWorkspaceRoot,
      externalFile
    );
    expect(
      callHierarchyPatternsModule.callHierarchyWithPatternMatching
    ).toHaveBeenCalledWith(
      expect.objectContaining({ symbolName: 'run', direction: 'incoming' }),
      externalFile,
      inferredWorkspaceRoot,
      expect.any(String),
      1,
      expect.any(Object)
    );
  });
});
