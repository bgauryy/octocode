import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const executeDirectTool = vi.fn();

vi.mock('@octocodeai/octocode-tools-core/direct', () => ({
  executeDirectTool: (...args: unknown[]) => executeDirectTool(...args),
}));

vi.mock('../../../src/utils/colors.js', () => ({
  c: (_color: string, s: string) => s,
  dim: (s: string) => s,
}));

import { lspCommand } from '../../../src/cli/commands/lsp.js';
import { EXIT } from '../../../src/cli/exit-codes.js';
import type { ParsedArgs } from '../../../src/cli/types.js';

function run(options: Record<string, string | boolean> = {}) {
  const parsed: ParsedArgs = {
    command: 'lsp',
    args: ['src/index.ts'],
    options: {
      type: 'definition',
      symbol: 'runCLI',
      line: '10',
      json: true,
      ...options,
    },
  };
  return lspCommand.handler(parsed);
}

function lspEnvelope(payload: Record<string, unknown>) {
  return {
    isError: false,
    content: [],
    structuredContent: {
      results: [
        {
          id: 'q1',
          data: {
            type: 'definition',
            uri: 'index.ts',
            resolvedSymbol: { name: 'runCLI', foundAtLine: 10 },
            lsp: { serverAvailable: true },
            payload,
          },
        },
      ],
    },
  };
}

function materializedFileEnvelope(
  localPath = '/tmp/octocode/tmp/tree/facebook/react/main/src/index.ts'
) {
  return {
    isError: false,
    content: [],
    structuredContent: {
      results: [
        {
          id: 'facebook/react',
          files: [
            {
              content: 'export function runCLI() {}',
              localPath,
              repoRoot: '/tmp/octocode/tmp/tree/facebook/react/main',
              resolvedBranch: 'main',
            },
          ],
        },
      ],
    },
  };
}

describe('lsp command', () => {
  beforeEach(() => {
    executeDirectTool.mockReset();
    process.exitCode = undefined;
    vi.spyOn(console, 'log').mockImplementation(() => undefined);
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    process.exitCode = undefined;
  });

  it('sets NOT_FOUND for semantic empty payloads', async () => {
    executeDirectTool.mockResolvedValue(
      lspEnvelope({
        kind: 'empty',
        category: 'symbolNotFound',
        reason: 'Could not find symbol',
      })
    );

    await run();

    expect(process.exitCode).toBe(EXIT.NOT_FOUND);
  });

  it('runs file-level documentSymbols without requiring --symbol or --line', async () => {
    executeDirectTool.mockResolvedValue({
      isError: false,
      content: [],
      structuredContent: {
        results: [
          {
            data: {
              type: 'documentSymbols',
              uri: 'index.ts',
              payload: {
                kind: 'documentSymbols',
                symbols: [{ name: 'runCLI', kind: 'function', line: 10 }],
              },
            },
          },
        ],
      },
    });
    const parsed: ParsedArgs = {
      command: 'lsp',
      args: ['src/index.ts'],
      options: { type: 'documentSymbols', json: true },
    };
    await lspCommand.handler(parsed);

    expect(executeDirectTool).toHaveBeenCalledTimes(1);
    expect(executeDirectTool).toHaveBeenCalledWith(
      'lspGetSemantics',
      expect.objectContaining({
        queries: [
          expect.objectContaining({
            uri: expect.stringContaining('src/index.ts'),
            type: 'documentSymbols',
          }),
        ],
      })
    );
    expect(process.exitCode).toBeUndefined();
  });

  it('renders definition locations directly for human output', async () => {
    executeDirectTool.mockResolvedValue(
      lspEnvelope({
        kind: 'definition',
        locations: [
          {
            uri: 'src/cli/index.ts',
            displayRange: { startLine: 73, endLine: 73 },
            content:
              'export async function runCLI(argv?: string[]): Promise<boolean> {',
          },
        ],
      })
    );

    await run({ json: false });

    const output = vi.mocked(console.log).mock.calls.flat().join('\n');
    expect(output).toContain('src/cli/index.ts:73-73');
    expect(output).toContain('export async function runCLI');
    expect(process.exitCode).toBeUndefined();
  });

  it('renders callers/callees as readable lines, not [object Object]', async () => {
    executeDirectTool.mockResolvedValue(
      lspEnvelope({
        kind: 'callers',
        calls: [
          {
            direction: 'incoming',
            item: {
              name: 'countLines',
              kind: 'function',
              uri: 'utils/core/lines.ts',
              line: 18,
            },
            ranges: [{ line: 23, character: 9 }],
          },
        ],
      })
    );

    await run({ type: 'callers', json: false });

    const output = vi.mocked(console.log).mock.calls.flat().join('\n');
    expect(output).not.toContain('[object Object]');
    expect(output).toContain('countLines');
    expect(output).toContain('utils/core/lines.ts');
  });

  it('infers --line with a local fixed-string search when omitted', async () => {
    executeDirectTool
      .mockResolvedValueOnce({
        isError: false,
        content: [],
        structuredContent: {
          results: [
            {
              data: {
                files: [{ matches: [{ line: 42 }] }],
              },
            },
          ],
        },
      })
      .mockResolvedValueOnce(
        lspEnvelope({ kind: 'definition', locations: [] })
      );

    await run({ line: false });

    expect(executeDirectTool).toHaveBeenNthCalledWith(
      1,
      'localSearchCode',
      expect.objectContaining({
        queries: [expect.objectContaining({ keywords: 'runCLI' })],
      })
    );
    expect(executeDirectTool).toHaveBeenNthCalledWith(
      2,
      'lspGetSemantics',
      expect.objectContaining({
        queries: [expect.objectContaining({ lineHint: 42 })],
      })
    );
  });

  it('--repo materializes the remote file and defaults workspaceRoot to the saved repo root', async () => {
    executeDirectTool
      .mockResolvedValueOnce(materializedFileEnvelope())
      .mockResolvedValueOnce(
        lspEnvelope({ kind: 'definition', locations: [] })
      );

    await run({
      repo: 'facebook/react',
      branch: 'main',
      'force-refresh': true,
    });

    expect(executeDirectTool).toHaveBeenCalledTimes(2);
    expect(executeDirectTool).toHaveBeenNthCalledWith(
      1,
      'ghGetFileContent',
      expect.objectContaining({
        queries: [
          expect.objectContaining({
            owner: 'facebook',
            repo: 'react',
            branch: 'main',
            path: 'src/index.ts',
            type: 'file',
            forceRefresh: true,
            fullContent: true,
            minify: 'none',
          }),
        ],
      })
    );
    expect(executeDirectTool).toHaveBeenNthCalledWith(
      2,
      'lspGetSemantics',
      expect.objectContaining({
        queries: [
          expect.objectContaining({
            uri: '/tmp/octocode/tmp/tree/facebook/react/main/src/index.ts',
            workspaceRoot: '/tmp/octocode/tmp/tree/facebook/react/main',
            symbolName: 'runCLI',
            lineHint: 10,
          }),
        ],
      })
    );
  });
});
