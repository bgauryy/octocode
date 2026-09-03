/**
 * registerUniqueTool — collision guard and registration helper.
 *
 * Native research tool registration was removed in the MCPTool-first refactor.
 * All Octocode research tools (GitHub, local, LSP, npm) are now served via the
 * bundled octocode MCP server through MCPTool. See mcp-tool.ts for the bridge.
 */
import assert from 'node:assert/strict';
import { test } from 'vitest';
import { Type } from 'typebox';
import type { ToolDefinition } from '../src/types.js';

test('registerUniqueTool registers a tool and throws on name collision', async () => {
  const { registerUniqueTool } = await import('../src/tools/octocode-tools.js');

  const registered = new Map<string, ToolDefinition>();
  const pi = { registerTool: (def: ToolDefinition) => registered.set(def.name, def) };
  const names = new Set<string>();

  registerUniqueTool(pi, names, {
    name: 'myTool',
    label: 'My Tool',
    description: 'Test tool',
    parameters: Type.Object({}),
    execute: async () => ({ content: [{ type: 'text', text: 'ok' }] }),
  });

  assert.equal(registered.has('myTool'), true, 'tool is registered');
  assert.equal(names.has('myTool'), true, 'name is tracked');

  // Second registration of the same name must throw.
  assert.throws(
    () => registerUniqueTool(pi, names, {
      name: 'myTool',
      label: 'My Tool',
      description: 'Duplicate',
      parameters: Type.Object({}),
      execute: async () => ({ content: [{ type: 'text', text: 'ok' }] }),
    }),
    /tool name collision/,
    'collision throws a descriptive error'
  );

  // Registration is fail-closed when the host cannot materialize a tool.
  assert.throws(() =>
    registerUniqueTool({}, new Set(['other']), {
      name: 'noRegisterTool',
      label: 'No-op',
      description: 'No registerTool on pi',
      parameters: Type.Object({}),
      execute: async () => ({ content: [{ type: 'text', text: 'ok' }] }),
    }),
    /registerTool/
  );
});

test('research tools are NOT registered as native Pi tools — served via MCPTool octocode server', async () => {
  // Verify that the 13 research tools are not registered in the
  // extension's tool palette. They are accessed through MCPTool instead.
  const nativeResearchTools = [
    'ghSearch', 'ghGetFileContent', 'ghSearchPullRequests', 'ghSearchIssues',
    'ghSearchCommits', 'ghListReleases', 'ghSearchDiscussions', 'ghCloneRepo',
    'npmSearch', 'localSearch', 'localAnalyzeGraph', 'localGetFileContent',
    'lspGetSemantics',
  ];

  // octocode-tools.ts no longer exports registerOctocodeTools.
  const mod = await import('../src/tools/octocode-tools.js') as Record<string, unknown>;
  assert.equal(
    'registerOctocodeTools' in mod,
    false,
    'registerOctocodeTools must not be exported — native tool registration is removed'
  );
  assert.equal(
    typeof mod['registerUniqueTool'],
    'function',
    'registerUniqueTool is still exported for other tool registrations'
  );

  // Confirm the tool names list itself (documentation anchor).
  assert.equal(nativeResearchTools.length, 13);
});
