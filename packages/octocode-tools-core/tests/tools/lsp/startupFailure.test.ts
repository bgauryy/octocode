import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  isLanguageServerAvailable: vi.fn(),
  acquirePooledClientDetailed: vi.fn(),
  resolveWorkspaceRootForFile: vi.fn(),
  detectLanguageId: vi.fn(),
}));

vi.mock('@octocodeai/octocode-engine/lsp/manager', () => ({
  isLanguageServerAvailable: mocks.isLanguageServerAvailable,
  acquirePooledClientDetailed: mocks.acquirePooledClientDetailed,
  unavailableHintFor: () => 'Install a matching language server.',
}));

vi.mock('@octocodeai/octocode-engine/lsp/workspaceRoot', () => ({
  resolveWorkspaceRootForFile: mocks.resolveWorkspaceRootForFile,
}));

vi.mock('@octocodeai/octocode-engine/lsp/config', () => ({
  detectLanguageId: mocks.detectLanguageId,
}));

const { executeLspGetSemantics } = await import(
  '../../../src/tools/lsp/semantic_content/execution.js'
);

const tempDirs: string[] = [];

afterEach(async () => {
  vi.clearAllMocks();
  await Promise.all(
    tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true }))
  );
});

describe('lspGetSemantics startup failures', () => {
  it('surfaces detailed engine startup failures instead of generic unavailable', async () => {
    const dir = await mkdtemp(join(process.cwd(), '.tmp-octocode-lsp-fail-'));
    tempDirs.push(dir);
    const filePath = join(dir, 'fixture.ts');
    await writeFile(
      filePath,
      ['export function target() {', '  return 1;', '}'].join('\n')
    );

    mocks.resolveWorkspaceRootForFile.mockResolvedValue(dir);
    mocks.detectLanguageId.mockReturnValue('typescript');
    mocks.isLanguageServerAvailable.mockResolvedValue(true);
    mocks.acquirePooledClientDetailed.mockResolvedValue({
      ok: false,
      kind: 'startupFailed',
      message: 'Refusing to start language server: invalid server path',
      filePath,
      workspaceRoot: dir,
    });

    const result = await executeLspGetSemantics({
      queries: [
        {
          uri: filePath,
          type: 'definition',
          symbolName: 'target',
          lineHint: 1,
        },
      ],
    } as never);

    expect(result.isError).toBe(true);
    const structured = result.structuredContent as {
      results?: Array<{ status?: string; data?: { error?: string } }>;
    };
    const row = structured.results?.[0];
    expect(row?.status).toBe('error');
    expect(row?.data?.error).toContain('startupFailed');
    expect(row?.data?.error).toContain('invalid server path');
  });
});
