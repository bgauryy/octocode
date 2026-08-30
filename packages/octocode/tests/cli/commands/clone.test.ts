import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

import { EXIT } from '../../../src/cli/exit-codes.js';

const executeDirectTool = vi.fn();

vi.mock('@octocodeai/octocode-tools-core/direct', () => ({
  executeDirectTool: (...args: unknown[]) => executeDirectTool(...args),
}));

vi.mock('../../../src/utils/colors.js', () => ({
  c: (_color: string, s: string) => s,
  dim: (s: string) => s,
}));

import { cloneCommand } from '../../../src/cli/commands/clone.js';
import type { ParsedArgs } from '../../../src/cli/types.js';

function run(args: string[], options: Record<string, string | boolean> = {}) {
  const parsed: ParsedArgs = { command: 'clone', args, options };
  return cloneCommand.handler(parsed);
}

describe('clone command', () => {
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

  it('delegates to ghCloneRepo and prints the canonical tool output once', async () => {
    executeDirectTool.mockResolvedValue({
      isError: false,
      content: [
        {
          type: 'text',
          text: 'results:\n  - index: 0\n    data:\n      location:\n        localPath: /tmp/octocode/tmp/clone/react\n',
        },
      ],
      structuredContent: {
        results: [
          {
            index: 0,
            data: {
              location: { localPath: '/tmp/octocode/tmp/clone/react' },
            },
          },
        ],
      },
    });

    await run(['facebook/react']);

    expect(executeDirectTool).toHaveBeenCalledWith(
      'ghCloneRepo',
      expect.objectContaining({
        queries: [
          expect.objectContaining({ owner: 'facebook', repo: 'react' }),
        ],
      })
    );
    expect(console.log).toHaveBeenCalledTimes(3);
    expect(console.log).toHaveBeenCalledWith(
      expect.stringContaining('localPath: /tmp/octocode/tmp/clone/react')
    );
    expect(console.log).not.toHaveBeenCalledWith(
      expect.stringContaining('Local clone:')
    );
  });

  it('hints at cache fetch when sparse-checkout fails on a file path', async () => {
    executeDirectTool.mockResolvedValue({
      isError: true,
      content: [
        {
          type: 'text',
          text: 'Clone failed: sparse-checkout set README.md failed: fatal: README.md is not a directory',
        },
      ],
      structuredContent: {
        results: [
          {
            index: 0,
            status: 'error',
            data: {
              error:
                'Clone failed: sparse-checkout set README.md failed: fatal: README.md is not a directory',
            },
          },
        ],
      },
    });

    await run(['bgauryy/octocode/README.md']);

    expect(process.exitCode).toBe(EXIT.TOOL);
    expect(console.error).toHaveBeenCalledWith(
      expect.stringContaining('is a file, but clone checks out directories')
    );
    expect(console.error).toHaveBeenCalledWith(
      expect.stringContaining('cache fetch bgauryy/octocode README.md')
    );
    expect(console.error).toHaveBeenCalledWith(
      expect.stringContaining('tools ghGetFileContent --scheme')
    );
  });

  it('uses the common tool-error envelope for JSON usage errors', async () => {
    await run([], { json: true });

    expect(
      JSON.parse(String(vi.mocked(console.log).mock.calls[0]?.[0]))
    ).toEqual({
      kind: 'octocode.toolError',
      version: 1,
      tool: 'ghCloneRepo',
      error: 'Provide a GitHub ref: owner/repo[/path][@branch] or a URL.',
    });
    expect(process.exitCode).toBe(EXIT.USAGE);
  });
});
