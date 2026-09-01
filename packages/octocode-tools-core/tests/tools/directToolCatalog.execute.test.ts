import { afterEach, describe, expect, it } from 'vitest';

import { executeDirectTool } from '../../src/tools/directToolCatalog.js';
import {
  LOCAL_SEARCH_TOOL_NAME,
  STATIC_TOOL_NAMES,
} from '../../src/tools/toolNames.js';
import { cleanup } from '../../src/serverConfig.js';
import { setRuntimeSurface, _resetRuntimeSurface } from '@octocodeai/config';

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

    await expect(
      executeDirectTool('local.text', { queries: [{ path: 'src' }] })
    ).rejects.toThrow('Unknown tool: local.text');
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

  it('still throws for an unknown tool name', async () => {
    await expect(
      executeDirectTool('definitely-not-a-real-tool', {})
    ).rejects.toThrow(/Unknown tool/);
  });

  // ENABLE_CLONE gate is MCP-only (packages/octocode-mcp/src/tools/toolFilters.ts).
  // tools-core no longer rejects based on ENABLE_CLONE — it is gate-free at
  // this layer. The MCP decides whether to register/expose ghCloneRepo at all.
  it('does NOT gate ghCloneRepo in tools-core when ENABLE_CLONE is false (gate is MCP-only)', async () => {
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
            mainResearchGoal: 'Verify clone gate is MCP-only',
            researchGoal: 'Confirm tools-core does not gate on ENABLE_CLONE',
            reasoning:
              'Architectural decision: clone gating belongs in the MCP layer',
          },
        ],
      }
    );

    // tools-core must NOT return a cloneDisabled error — that code was removed.
    // The call may error for other reasons (network, auth) but not clone gating.
    const structured = result.structuredContent as
      { error?: { code?: string } } | undefined;
    expect(structured?.error?.code).not.toBe('cloneDisabled');
  });

  it('does NOT gate ghGetFileContent directory type in tools-core when ENABLE_CLONE is false (gate is MCP-only)', async () => {
    process.env.ENABLE_LOCAL = 'true';
    process.env.ENABLE_CLONE = 'false';
    cleanup();

    const result = await executeDirectTool(
      STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
      {
        queries: [
          {
            owner: 'octocat',
            repo: 'Hello-World',
            path: 'README.md',
            mainResearchGoal: 'Verify directory fetch clone gate is MCP-only',
            researchGoal:
              'Confirm tools-core does not gate directory fetch on ENABLE_CLONE',
            reasoning:
              'Architectural decision: clone gating belongs in the MCP layer',
          },
        ],
      }
    );

    // tools-core must NOT emit "Directory fetch requires local clone support".
    const text = JSON.stringify(result.structuredContent);
    expect(text).not.toContain('Directory fetch requires local clone support');
  });
});
