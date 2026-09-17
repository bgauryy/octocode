import { expect, test } from 'vitest';
import { assembleSessionPromptContext, type SessionPromptContents } from '../src/tools/session-prompt-context.js';

test('MCP catalog within per-segment budget assembles correctly', () => {
  // ~2.5K tokens (10K chars) — within the 6K token per-segment cap
  const catalog = 'schema-data '.repeat(900);
  const contents: SessionPromptContents = {
    'octocode-product-policy': 'policy', 'mcp-tool-contracts': catalog,
    'runtime-tool-contracts': '', 'dynamic-tool-contracts': '',
    'available-skills': '', 'session-artifact-contract': '', 'awareness-cli-runtime': '',
  };
  const assembled = assembleSessionPromptContext(contents);
  expect(assembled.content).toContain(catalog);
  expect(assembled.contents['mcp-tool-contracts']).toBe(catalog);
});

test('MCP catalog exceeding per-segment budget throws', () => {
  // ~8.25K tokens (33K chars) — exceeds the 6K token per-segment cap
  const oversized = 'schema-data '.repeat(3_000);
  const contents: SessionPromptContents = {
    'octocode-product-policy': 'policy', 'mcp-tool-contracts': oversized,
    'runtime-tool-contracts': '', 'dynamic-tool-contracts': '',
    'available-skills': '', 'session-artifact-contract': '', 'awareness-cli-runtime': '',
  };
  expect(() => assembleSessionPromptContext(contents)).toThrow(/token budget/);
});
