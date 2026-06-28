import { afterEach, describe, expect, it } from 'vitest';

import { executeDirectTool } from '../../src/tools/directToolCatalog.js';
import {
  OQL_SEARCH_TOOL_NAME,
  STATIC_TOOL_NAMES,
} from '../../src/tools/toolNames.js';
import { cleanup } from '../../src/serverConfig.js';
import {
  setRuntimeSurface,
  _resetRuntimeSurface,
} from '../../src/shared/config/runtimeSurface.js';

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

  it('rejects ghCloneRepo when ENABLE_CLONE is false', async () => {
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
            mainResearchGoal: 'Verify clone gate',
            researchGoal: 'Ensure direct clone execution respects ENABLE_CLONE',
            reasoning: 'Regression test for direct CLI clone gate',
          },
        ],
      }
    );

    expect(result.isError).toBe(true);
    const structured = result.structuredContent as
      | {
          error?: {
            code?: string;
            message?: string;
          };
        }
      | undefined;
    expect(structured?.error?.code).toBe('cloneDisabled');
    expect(structured?.error?.message).toContain('ENABLE_CLONE=true');
  });

  it('rejects ghGetFileContent directory materialization when ENABLE_CLONE is false', async () => {
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
            path: 'src',
            type: 'directory',
            mainResearchGoal: 'Verify directory fetch clone gate',
            researchGoal:
              'Ensure directory fetch requires clone support before provider work',
            reasoning: 'Regression test for ghGetFileContent directory gate',
          },
        ],
      }
    );

    expect(result.isError).toBe(true);
    const text = JSON.stringify(result.structuredContent);
    expect(text).toContain('Directory fetch requires local clone support');
    expect(text).toContain('ENABLE_CLONE=true');
  });

  it('wraps oqlSearch output once in the standard direct-tool results[].data shape', async () => {
    setRuntimeSurface('cli');
    cleanup();

    const result = await executeDirectTool(OQL_SEARCH_TOOL_NAME, {
      target: 'code',
      from: { kind: 'local', path: 'src/oql' },
      where: { kind: 'text', value: 'runOqlSearch' },
      view: 'discovery',
      limit: 2,
    });

    expect(result.isError).not.toBe(true);
    const structured = result.structuredContent as {
      results?: Array<{ id?: string; status?: string; data?: unknown }>;
      oql?: unknown;
    };
    expect(Array.isArray(structured.results)).toBe(true);
    expect(structured.results?.[0]?.id).toBe('oqlSearch-1');
    expect(structured.results?.[0]?.status).toBeUndefined();
    expect(structured).not.toHaveProperty('oql');
    expect(
      (structured.results?.[0]?.data as { results?: unknown[] }).results?.length
    ).toBeGreaterThan(0);
  });

  it('deduplicates direct oqlSearch row continuation hints without dropping executable queries', async () => {
    setRuntimeSurface('cli');
    cleanup();

    const result = await executeDirectTool(OQL_SEARCH_TOOL_NAME, {
      target: 'code',
      from: { kind: 'local', path: 'src/oql' },
      where: { kind: 'text', value: 'runOqlSearch' },
      view: 'discovery',
      limit: 2,
    });

    expect(result.isError).not.toBe(true);
    const structured = result.structuredContent as {
      results?: Array<{
        data?: {
          nextHints?: Record<string, { why?: string; confidence?: string }>;
          results?: Array<{
            next?: Record<
              string,
              { query?: Record<string, unknown>; why?: string; confidence?: string }
            >;
          }>;
        };
      }>;
    };
    const data = structured.results?.[0]?.data;
    const firstRowNext = data?.results?.[0]?.next;
    expect(data?.nextHints?.['next.fetch']).toMatchObject({
      why: 'Read the exact content at this hit.',
      confidence: 'exact',
    });
    expect(firstRowNext?.['next.fetch']?.query).toMatchObject({
      target: 'content',
    });
    expect(firstRowNext?.['next.fetch']).not.toHaveProperty('why');
    expect(firstRowNext?.['next.fetch']).not.toHaveProperty('confidence');
  });

  it('marks zero-match oqlSearch rows as empty without failing the call', async () => {
    setRuntimeSurface('cli');
    cleanup();

    const result = await executeDirectTool(OQL_SEARCH_TOOL_NAME, {
      target: 'code',
      from: { kind: 'local', path: 'src/oql' },
      where: { kind: 'text', value: 'definitely-no-such-oql-symbol-xyz' },
      view: 'discovery',
      limit: 2,
    });

    expect(result.isError).not.toBe(true);
    const structured = result.structuredContent as {
      results?: Array<{ status?: string; data?: unknown }>;
    };
    expect(structured.results?.[0]?.status).toBe('empty');
  });
});
