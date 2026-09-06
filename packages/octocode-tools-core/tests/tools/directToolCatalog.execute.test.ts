import { afterEach, describe, expect, it } from 'vitest';

import { executeDirectTool } from '../../src/tools/directToolCatalog.exec.js';
import {
  LOCAL_SEARCH_TOOL_NAME,
  STATIC_TOOL_NAMES,
} from '../../src/tools/toolNames.js';
import { cleanup } from '../../src/serverConfig.js';
import { setRuntimeSurface, _resetRuntimeSurface } from '@octocodeai/config';
import { ADAPTER_PARITY_CASES } from '../fixtures/adapterParityFixture.js';

describe('executeDirectTool - invalid input handling (finding 3)', () => {
  const originalEnableClone = process.env.ENABLE_CLONE;
  const originalEnableLocal = process.env.ENABLE_LOCAL;
  const originalToolsToRun = process.env.TOOLS_TO_RUN;
  const originalDisableTools = process.env.DISABLE_TOOLS;

  afterEach(() => {
    if (originalEnableClone === undefined) {
      delete process.env.ENABLE_CLONE;
    } else {
      process.env.ENABLE_CLONE = originalEnableClone;
    }
    if (originalEnableLocal === undefined) {
      delete process.env.ENABLE_LOCAL;
    } else {
      process.env.ENABLE_LOCAL = originalEnableLocal;
    }
    if (originalToolsToRun === undefined) {
      delete process.env.TOOLS_TO_RUN;
    } else {
      process.env.TOOLS_TO_RUN = originalToolsToRun;
    }
    if (originalDisableTools === undefined) {
      delete process.env.DISABLE_TOOLS;
    } else {
      process.env.DISABLE_TOOLS = originalDisableTools;
    }
    _resetRuntimeSurface();
    cleanup();
  });

  it('runs a local tool by default on the CLI surface when ENABLE_LOCAL is unset', async () => {
    setRuntimeSurface('cli');
    delete process.env.ENABLE_LOCAL;
    cleanup();

    const result = await executeDirectTool(LOCAL_SEARCH_TOOL_NAME, {
      queries: [
        {
          path: 'src/shared/config',
          operation: 'text',
          searchText: 'resolveLocal',
          maxFiles: 3,
          reasoning: 'Regression test: local tools should work by default',
        },
      ],
    });

    const structured = result.structuredContent as
      { error?: { code?: string } } | undefined;
    expect(structured?.error?.code).not.toBe('localToolsDisabled');
  });

  it('rejects a local tool when ENABLE_LOCAL is false on the CLI surface', async () => {
    setRuntimeSurface('cli');
    process.env.ENABLE_LOCAL = 'false';
    cleanup();

    const result = await executeDirectTool(LOCAL_SEARCH_TOOL_NAME, {
      queries: [
        {
          path: 'src/shared/config',
          operation: 'text',
          searchText: 'resolveLocal',
          maxFiles: 3,
          reasoning: 'Regression test: ENABLE_LOCAL=false disables local tools',
        },
      ],
    });

    expect(result.isError).toBe(true);
    const structured = result.structuredContent as
      { error?: { code?: string; message?: string } } | undefined;
    expect(structured?.error?.code).toBe('localToolsDisabled');
    expect(structured?.error?.message).toContain('ENABLE_LOCAL=true');
  });

  it('rejects a local tool when ENABLE_LOCAL is false on the MCP surface', async () => {
    setRuntimeSurface('mcp');
    process.env.ENABLE_LOCAL = 'false';
    cleanup(); // invalidate the cached config so the new env is read

    const result = await executeDirectTool(LOCAL_SEARCH_TOOL_NAME, {
      queries: [
        {
          path: '.',
          operation: 'text',
          searchText: 'anything',
          reasoning: 'Regression test for direct CLI local gate',
        },
      ],
    });

    expect(result.isError).toBe(true);
    const structured = result.structuredContent as
      { error?: { code?: string; message?: string } } | undefined;
    expect(structured?.error?.code).toBe('localToolsDisabled');
    expect(structured?.error?.message).toContain('ENABLE_LOCAL=true');
  });

  it('does not resurrect a removed compatibility tool through TOOLS_TO_RUN', async () => {
    setRuntimeSurface('cli');
    process.env.TOOLS_TO_RUN = 'local.text';
    cleanup();

    const result = await executeDirectTool('local.text', {
      queries: [{ path: 'src' }],
    });
    expect(result.isError).toBe(true);
    expect(result.content).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'text',
          text: expect.stringContaining('Unknown tool: local.text'),
        }),
      ])
    );
  });

  it('treats TOOLS_TO_RUN as a strict direct-CLI allowlist', async () => {
    setRuntimeSurface('cli');
    process.env.TOOLS_TO_RUN = 'local.text';
    cleanup();

    const result = await executeDirectTool(LOCAL_SEARCH_TOOL_NAME, {
      queries: [
        {
          path: 'src',
          operation: 'text',
          searchText: 'executeDirectTool',
          reasoning: 'Verify the direct CLI honors the strict allowlist',
        },
      ],
    });

    expect(result.isError).toBe(true);
    const structured = result.structuredContent as
      { error?: { code?: string; message?: string } } | undefined;
    expect(structured?.error?.code).toBe('toolNotEnabled');
    expect(structured?.error?.message).toContain('outside the TOOLS_TO_RUN');
  });

  it.each([
    ['TOOLS_TO_RUN', LOCAL_SEARCH_TOOL_NAME, 'toolNotEnabled'],
    ['DISABLE_TOOLS', STATIC_TOOL_NAMES.GITHUB_SEARCH, 'toolDisabled'],
  ] as const)(
    'applies %s to default remote tools in direct execution',
    async (flag, value, errorCode) => {
      setRuntimeSurface('cli');
      delete process.env.TOOLS_TO_RUN;
      delete process.env.DISABLE_TOOLS;
      process.env[flag] = value;
      cleanup();

      const result = await executeDirectTool(STATIC_TOOL_NAMES.GITHUB_SEARCH, {
        queries: [{ operation: 'code', keywords: ['executeDirectTool'] }],
      });

      expect(result.isError).toBe(true);
      const structured = result.structuredContent as
        { error?: { code?: string } } | undefined;
      expect(structured?.error?.code).toBe(errorCode);
    }
  );

  it.each(ADAPTER_PARITY_CASES)(
    'applies the strict direct-execution allowlist to $name',
    async testCase => {
      setRuntimeSurface('cli');
      process.env.ENABLE_LOCAL = 'true';
      process.env.ENABLE_CLONE = 'true';
      process.env.TOOLS_TO_RUN = 'local.text';
      delete process.env.DISABLE_TOOLS;
      cleanup();

      const result = await executeDirectTool(testCase.name, testCase.input);

      expect(result.isError).toBe(true);
      const structured = result.structuredContent as
        { error?: { code?: string } } | undefined;
      expect(structured?.error?.code).toBe('toolNotEnabled');
    }
  );

  it('returns a structured error result instead of throwing for invalid input', async () => {
    // A primitive is invalid for every tool's object input schema, so the
    // parse fails. It must surface as a structured CallToolResult error, not a
    // thrown exception (which diverges from the execution-error path).
    const result = await executeDirectTool(
      LOCAL_SEARCH_TOOL_NAME,
      'not-an-object'
    );

    expect(result.isError).toBe(true);
    const structured = result.structuredContent as
      { status?: string; tool?: string } | undefined;
    expect(structured?.status).toBe('error');
    expect(structured?.tool).toBe(LOCAL_SEARCH_TOOL_NAME);
  });

  it('returns an error envelope for an unknown tool name', async () => {
    const result = await executeDirectTool('definitely-not-a-real-tool', {});

    expect(result.isError).toBe(true);
    expect(result.content).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'text',
          text: expect.stringContaining(
            'Unknown tool: definitely-not-a-real-tool'
          ),
        }),
      ])
    );
  });

  it('gates ghCloneRepo in tools-core when ENABLE_CLONE is false', async () => {
    process.env.ENABLE_LOCAL = 'true';
    process.env.ENABLE_CLONE = 'false';
    cleanup();

    const result = await executeDirectTool(
      STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
      {
        queries: [
          {
            owner: 'octocat',
            repo: 'Hello-World',
          },
        ],
      }
    );

    const structured = result.structuredContent as
      { error?: { code?: string } } | undefined;
    expect(result.isError).toBe(true);
    expect(structured?.error?.code).toBe('cloneDisabled');
  });

  it.each([
    ['ENABLE_LOCAL', 'false', 'localToolsDisabled'],
    ['ENABLE_CLONE', 'false', 'cloneDisabled'],
  ] as const)(
    'gates ghGetFileContent directory materialization inside tools-core when %s=%s',
    async (flag, value, errorCode) => {
      process.env.ENABLE_LOCAL = 'true';
      process.env.ENABLE_CLONE = 'true';
      process.env[flag] = value;
      cleanup();

      const result = await executeDirectTool(
        STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            {
              owner: 'octocat',
              repo: 'Hello-World',
              path: 'src',
              type: 'directory',
            },
          ],
        }
      );

      expect(JSON.stringify(result.structuredContent)).toContain(errorCode);
    }
  );
});
