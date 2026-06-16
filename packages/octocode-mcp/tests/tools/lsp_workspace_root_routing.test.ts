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

  it.each([['definition'], ['references'], ['callers']] as const)(
    'passes the inferred root to lspGetSemantics type=%s',
    async type => {
      const managerModule = await import('octocode-lsp/manager');

      vi.spyOn(managerModule, 'isLanguageServerAvailable').mockResolvedValue(
        true
      );
      vi.spyOn(managerModule, 'acquirePooledClient').mockResolvedValue(null);

      const { executeLspGetSemantics } =
        await import('../../../octocode-tools-core/src/tools/lsp/semantic_content/execution.js');

      await executeLspGetSemantics({
        queries: [
          {
            uri: externalFile,
            type,
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
    }
  );
});
