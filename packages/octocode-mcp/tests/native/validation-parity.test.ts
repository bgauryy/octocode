import { afterAll, describe, expect, it } from 'vitest';
import { loadNativeBinding } from '../../src/native/index.js';

// One case list, two surfaces of the real addon: the MCP error text and the
// CLI error details must give the same repair guidance, because native is the
// only validator. Needs a current native build.
type Runtime = {
  execute(requestId: string, tool: string, input: unknown): Promise<unknown>;
  executeMcp(requestId: string, tool: string, input: unknown): Promise<unknown>;
  close(): Promise<void>;
};

const { NativeRuntime } = loadNativeBinding() as unknown as {
  NativeRuntime: new (options: { surface: string }) => Runtime;
};
const runtime = new NativeRuntime({ surface: 'cli' });
const mcpRuntime = new NativeRuntime({ surface: 'mcp' });
afterAll(() => Promise.all([runtime.close(), mcpRuntime.close()]));

const brief = { mainGoal: 'g', reasoning: 'r' };

const mcpMessage = async (tool: string, input: unknown) => {
  const result = (await mcpRuntime.executeMcp(
    `mcp-${request++}`,
    tool,
    input
  )) as { isError?: boolean; content?: { text?: string }[] };
  return result.isError
    ? (result.content ?? []).map(block => block.text ?? '').join('\n')
    : '';
};

let request = 0;
const nativeDetails = async (tool: string, input: unknown) => {
  try {
    await runtime.execute(`parity-${request++}`, tool, input);
  } catch (error) {
    const payload = JSON.parse((error as Error).message).payload as {
      details?: string[];
    };
    return (payload.details ?? []).join('\n');
  }
  return '';
};

const cases: ReadonlyArray<{
  name: string;
  tool: string;
  query: Record<string, unknown>;
  says: string[];
  never?: string[];
}> = [
  {
    name: 'boolean for an on/off enum',
    tool: 'localSearch',
    query: { path: '.', matchString: 'needle', regex: true },
    says: ['booleans are not accepted; use "rust"'],
  },
  {
    name: 'number outside its range',
    tool: 'structureSearch',
    query: { path: '.', maxDepth: 50 },
    says: ['maxDepth: Number is outside the allowed range (1-20)'],
  },
  {
    name: 'a field the published view leaves out',
    tool: 'ghSearchHistory',
    query: {
      operation: 'pullRequest',
      owner: 'a',
      repo: 'b',
      qualifers: 'author:x',
    },
    says: ["Remove unknown field 'qualifers'", "did you mean 'qualifiers'?"],
  },
  {
    name: 'core-authored required-field guidance',
    tool: 'ghSearchCode',
    query: { keywords: ['needle'] },
    says: [
      'owner: Missing required field: owner (Set owner: code search cannot span all of GitHub (add repo to narrow further).)',
    ],
  },
  {
    name: 'a flat clasify resource is named as sent',
    tool: 'clasify',
    query: {
      resources: [
        {
          tool: 'localFetch',
          query: { path: 'package.json' },
          candidateEvidence: 'fileChunks',
        },
      ],
      questions: [{ type: 'yesno', ask: 'Does this define a package?' }],
    },
    says: [
      'resources.0.candidateEvidence: candidateEvidence requires localSearch or ghSearchCode.',
    ],
    never: ['context.candidateEvidence'],
  },
  {
    name: 'a misspelled lspSearch path',
    tool: 'lspSearch',
    query: { pth: 'package.json', symbolName: 'name', lineHint: 1 },
    says: ["did you mean 'path'?"],
    never: ["did you mean 'page'?"],
  },
  {
    name: 'a match-only field on a symbols query',
    tool: 'astSearch',
    query: { operation: 'symbols', path: '.', include: ['*.ts'] },
    says: [
      "Remove 'include' from queries[0]: it applies only with pattern or rule",
    ],
    never: ['namedOnly', 'send queries[0] to localSearch'],
  },
];

describe('CLI and MCP validation guidance parity', () => {
  it.each(cases)('$name', async ({ tool, query, says, never = [] }) => {
    const input = { queries: [{ ...brief, ...query }] };
    const surfaces = {
      mcp: await mcpMessage(tool, input),
      cli: await nativeDetails(tool, input),
    };
    for (const [surface, text] of Object.entries(surfaces)) {
      expect(text, surface).not.toBe('');
      for (const guidance of says) expect(text, surface).toContain(guidance);
      for (const banned of never) expect(text, surface).not.toContain(banned);
    }
  });
});
