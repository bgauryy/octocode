import { afterAll, describe, expect, it } from 'vitest';
import { DIRECT_TOOL_DEFINITIONS } from '@octocodeai/config/schema';
import { loadNativeBinding, toolInputSchema } from '../../src/native/index.js';

// One case list, two surfaces: the MCP message (core Zod issues shaped by
// validationMessages) and the native CLI details from the real addon must give
// the same repair guidance. Needs a current native build.
type Issue = {
  message: string;
  path?: ReadonlyArray<PropertyKey | { key: PropertyKey }>;
};
type Standard = {
  '~standard': {
    validate: (
      value: unknown
    ) => { issues?: readonly Issue[] } | Promise<{ issues?: readonly Issue[] }>;
  };
};
type Runtime = {
  normalizeInput(tool: string, input: unknown): unknown;
  execute(requestId: string, tool: string, input: unknown): Promise<unknown>;
  close(): Promise<void>;
};

const { NativeRuntime } = loadNativeBinding() as unknown as {
  NativeRuntime: new (options: { surface: string }) => Runtime;
};
const runtime = new NativeRuntime({ surface: 'cli' });
afterAll(() => runtime.close());

const brief = { goal: 'g', reasoning: 'r' };

const mcpMessage = async (tool: string, input: unknown) => {
  const definition = DIRECT_TOOL_DEFINITIONS.find(d => d.name === tool)!;
  const schema = toolInputSchema(definition, value =>
    runtime.normalizeInput(tool, value)
  ) as Standard;
  const result = await schema['~standard'].validate(input);
  return (result.issues ?? [])
    .map(issue =>
      issue.path?.length
        ? `${issue.path
            .map(p => String(typeof p === 'object' ? p.key : p))
            .join('.')}: ${issue.message}`
        : issue.message
    )
    .join(', ');
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
    query: { path: '.', searchText: 'needle', regex: true },
    says: ['booleans are not accepted; use "rust"'],
  },
  {
    name: 'number outside its range',
    tool: 'structureSearch',
    query: { path: '.', maxDepth: 50 },
    says: ['maxDepth: Number is outside the allowed range (0-20)'],
  },
  {
    name: 'a field the published view leaves out',
    tool: 'ghSearchHistory',
    query: {
      operation: 'pullRequest',
      owner: 'a',
      repo: 'b',
      mergedAt: '2026-01-01',
    },
    says: ["Remove unknown field 'mergedAt'", "did you mean 'merged-at'?"],
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
    name: 'a path sent to lspSearch',
    tool: 'lspSearch',
    query: { path: 'package.json', symbolName: 'name', lineHint: 1 },
    says: ["did you mean 'uri'?"],
    never: ["did you mean 'page'?"],
  },
  {
    name: 'a match-only field on a symbols query',
    tool: 'astSearch',
    query: { operation: 'symbols', path: '.', include: ['*.ts'] },
    says: [
      "Remove 'include' from queries[0]: it applies only with pattern or rule",
    ],
    never: ['namedOnly', 'nodeLimit', 'send queries[0] to localSearch'],
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
