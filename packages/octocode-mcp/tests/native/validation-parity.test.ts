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

type ToolErrorEnvelope = {
  kind?: string;
  tool?: string;
  errorCode?: string;
  details?: string[];
};

const mcpMessage = async (tool: string, input: unknown) => {
  const result = (await mcpRuntime.executeMcp(
    `mcp-${request++}`,
    tool,
    input
  )) as {
    isError?: boolean;
    content?: { text?: string }[];
    structuredContent?: ToolErrorEnvelope;
  };
  if (!result.isError) return { text: '', envelope: undefined };
  return {
    text: (result.content ?? []).map(block => block.text ?? '').join('\n'),
    envelope: result.structuredContent,
  };
};

let request = 0;
const nativeDetails = async (tool: string, input: unknown) => {
  try {
    await runtime.execute(`parity-${request++}`, tool, input);
  } catch (error) {
    const thrown = JSON.parse((error as Error).message) as {
      code?: string;
      payload: ToolErrorEnvelope;
    };
    return {
      text: (thrown.payload.details ?? []).join('\n'),
      code: thrown.code,
      envelope: thrown.payload,
    };
  }
  return { text: '', code: undefined, envelope: undefined };
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
    const mcp = await mcpMessage(tool, input);
    const cli = await nativeDetails(tool, input);
    // Both surfaces answer with the typed octocode.toolError envelope.
    expect(mcp.envelope).toMatchObject({
      kind: 'octocode.toolError',
      tool,
      errorCode: 'invalidInput',
    });
    expect(mcp.text).toContain('(errorCode: invalidInput)');
    expect(cli.code).toBe('invalidInput');
    expect(cli.envelope).toMatchObject({
      kind: 'octocode.toolError',
      tool,
      errorCode: 'invalidInput',
    });
    const surfaces = { mcp: mcp.text, cli: cli.text };
    for (const [surface, text] of Object.entries(surfaces)) {
      expect(text, surface).not.toBe('');
      for (const guidance of says) expect(text, surface).toContain(guidance);
      for (const banned of never) expect(text, surface).not.toContain(banned);
    }
  });
});
