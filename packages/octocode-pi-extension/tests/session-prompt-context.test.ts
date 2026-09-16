import { expect, test } from 'vitest';
import { assembleSessionPromptContext, type SessionPromptContents } from '../src/tools/session-prompt-context.js';

test('complete MCP contracts can use the shared context allowance without a smaller catalog cap', () => {
  const catalog = 'schema-data '.repeat(12_000);
  const contents: SessionPromptContents = {
    'octocode-product-policy': 'policy', 'mcp-tool-contracts': catalog,
    'runtime-tool-contracts': '', 'native-tool-contracts': '', 'dynamic-tool-contracts': '',
    'available-skills': '', 'session-artifact-contract': '', 'awareness-cli-runtime': '',
  };
  const assembled = assembleSessionPromptContext(contents);
  expect(assembled.content).toContain(catalog);
  expect(assembled.contents['mcp-tool-contracts']).toBe(catalog);
  expect(() => assembleSessionPromptContext({ ...contents, 'mcp-tool-contracts': catalog.repeat(3) })).toThrow(/token budget/);
});
