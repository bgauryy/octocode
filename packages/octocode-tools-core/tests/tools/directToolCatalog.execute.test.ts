import { afterEach, describe, expect, it } from 'vitest';

import { executeDirectTool } from '../../src/tools/directToolCatalog.js';
import { STATIC_TOOL_NAMES } from '../../src/tools/toolNames.js';
import { cleanup } from '../../src/serverConfig.js';
import {
  setRuntimeSurface,
  _resetRuntimeSurface,
} from '@octocodeai/config';

describe('executeDirectTool - invalid input handling (finding 3)', () => {
  const originalEnableClone = process.env.ENABLE_CLONE;
  const originalEnableLocal = process.env.ENABLE_LOCAL;

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
    _resetRuntimeSurface();
    cleanup();
  });

  it('runs a local tool by default when ENABLE_LOCAL is unset', async () => {
    setRuntimeSurface('mcp');
    delete process.env.ENABLE_LOCAL;
    cleanup();

    const result = await executeDirectTool(STATIC_TOOL_NAMES.LOCAL_RIPGREP, {
      queries: [
        {
          path: 'src/shared/config',
          keywords: 'resolveLocal',
          maxFiles: 3,
          mainResearchGoal: 'Verify local tools default on',
          researchGoal: 'Default local gate behavior',
          reasoning: 'Regression test: local tools should work by default',
        },
      ],
    });

    const structured = result.structuredContent as
      | { error?: { code?: string } }
      | undefined;
    expect(structured?.error?.code).not.toBe('localToolsDisabled');
  });

  it('rejects a local tool when ENABLE_LOCAL is false on the CLI surface', async () => {
    setRuntimeSurface('cli');
    process.env.ENABLE_LOCAL = 'false';
    cleanup();

    const result = await executeDirectTool(STATIC_TOOL_NAMES.LOCAL_RIPGREP, {
      queries: [
        {
          path: 'src/shared/config',
          keywords: 'resolveLocal',
          maxFiles: 3,
          mainResearchGoal: 'Verify ENABLE_LOCAL explicit opt-out',
          researchGoal: 'CLI local gate behavior',
          reasoning: 'Regression test: ENABLE_LOCAL=false disables local tools',
        },
      ],
    });

    expect(result.isError).toBe(true);
    const structured = result.structuredContent as
      | { error?: { code?: string; message?: string } }
      | undefined;
    expect(structured?.error?.code).toBe('localToolsDisabled');
    expect(structured?.error?.message).toContain('ENABLE_LOCAL=true');
  });

  it('rejects a local tool when ENABLE_LOCAL is false on the MCP surface', async () => {
    setRuntimeSurface('mcp');
    process.env.ENABLE_LOCAL = 'false';
    cleanup(); // invalidate the cached config so the new env is read

    const result = await executeDirectTool(STATIC_TOOL_NAMES.LOCAL_RIPGREP, {
      queries: [
        {
          path: '.',
          keywords: 'anything',
          mainResearchGoal: 'Verify local gate',
          researchGoal: 'Ensure direct local execution respects ENABLE_LOCAL',
          reasoning: 'Regression test for direct CLI local gate',
        },
      ],
    });

    expect(result.isError).toBe(true);
    const structured = result.structuredContent as
      | { error?: { code?: string; message?: string } }
      | undefined;
    expect(structured?.error?.code).toBe('localToolsDisabled');
    expect(structured?.error?.message).toContain('ENABLE_LOCAL=true');
  });

  it('returns a structured error result instead of throwing for invalid input', async () => {
    // A primitive is invalid for every tool's object input schema, so the
    // parse fails. It must surface as a structured CallToolResult error, not a
    // thrown exception (which diverges from the execution-error path).
    const result = await executeDirectTool(
      STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
      'not-an-object'
    );

    expect(result.isError).toBe(true);
    const structured = result.structuredContent as
      | { status?: string; tool?: string }
      | undefined;
    expect(structured?.status).toBe('error');
    expect(structured?.tool).toBe(STATIC_TOOL_NAMES.LOCAL_FIND_FILES);
  });

  it('still throws for an unknown tool name', async () => {
    await expect(
      executeDirectTool('definitely-not-a-real-tool', {})
    ).rejects.toThrow(/Unknown tool/);
  });

  it('does not expose oqlSearch as a direct tool; use the CLI search command or OQL API instead', async () => {
    await expect(executeDirectTool('oqlSearch', {})).rejects.toThrow(/Unknown tool/);
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
            reasoning: 'Architectural decision: clone gating belongs in the MCP layer',
          },
        ],
      }
    );

    // tools-core must NOT return a cloneDisabled error — that code was removed.
    // The call may error for other reasons (network, auth) but not clone gating.
    const structured = result.structuredContent as { error?: { code?: string } } | undefined;
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
            researchGoal: 'Confirm tools-core does not gate directory fetch on ENABLE_CLONE',
            reasoning: 'Architectural decision: clone gating belongs in the MCP layer',
          },
        ],
      }
    );

    // tools-core must NOT emit "Directory fetch requires local clone support".
    const text = JSON.stringify(result.structuredContent);
    expect(text).not.toContain('Directory fetch requires local clone support');
  });
});
