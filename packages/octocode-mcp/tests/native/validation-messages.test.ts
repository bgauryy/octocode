import { describe, expect, it } from 'vitest';
import { DIRECT_TOOL_DEFINITIONS } from '@octocodeai/config/schema';
import { toolInputSchema } from '../../src/native/index.js';

type Issue = {
  message: string;
  path?: ReadonlyArray<PropertyKey | { key: PropertyKey }>;
};
type Standard = {
  '~standard': {
    validate: (
      value: unknown
    ) =>
      | { value?: unknown; issues?: readonly Issue[] }
      | Promise<{ value?: unknown; issues?: readonly Issue[] }>;
  };
};

// Mirrors the MCP SDK's validateStandardSchema/formatIssue rendering.
const sdkMessage = async (tool: string, args: unknown) => {
  const definition = DIRECT_TOOL_DEFINITIONS.find(d => d.name === tool)!;
  const schema = toolInputSchema(definition) as Standard;
  const result = await schema['~standard'].validate(args);
  if (!result.issues?.length) return undefined;
  return result.issues
    .map(issue =>
      issue.path?.length
        ? `${issue.path
            .map(p => String(typeof p === 'object' ? p.key : p))
            .join('.')}: ${issue.message}`
        : issue.message
    )
    .join(', ');
};

describe('MCP validation messages are actionable (CLI parity)', () => {
  it('lists allowed discriminator values for an astSearch operation typo', async () => {
    const message = await sdkMessage('astSearch', {
      queries: [
        {
          operation: 'matches',
          path: '.',
          pattern: 'x',
          goal: 'g',
          reasoning: 'r',
        },
      ],
    });
    expect(message).toContain(
      'queries.0.operation: Value "matches" is outside the allowed enum; allowed: match, syntaxTree, symbols'
    );
    expect(message).toContain("did you mean 'match'?");
    expect(message).not.toContain('Invalid input');
  });

  it('lists allowed lspSearch operations across union branches', async () => {
    const message = await sdkMessage('lspSearch', {
      operation: 'refs',
      uri: 'a.ts',
      symbolName: 'x',
      lineHint: 1,
      goal: 'test goal',
      reasoning: 'r',
    });
    expect(message).toMatch(
      /queries\.0\.operation: Value "refs" is outside the allowed enum; allowed: .*references/
    );
    expect(message).toContain('documentSymbols');
    expect(message).toContain('workspaceSymbol');
  });

  it('suggests the nearest field for an unknown key and points at valid fields', async () => {
    const message = await sdkMessage('localSearch', {
      path: '.',
      searchText: 'a',
      goal: 'test goal',
      reasoning: 'r',
      serchText2: 1,
    });
    expect(message).toContain(
      "queries.0: Remove unknown field 'serchText2' (did you mean 'searchText'?)"
    );
    expect(message).toMatch(/queries\.0: Valid fields: .*searchText/);
  });

  it('names a missing required field plainly', async () => {
    const message = await sdkMessage('localSearch', {
      path: '.',
      goal: 'test goal',
      reasoning: 'r',
    });
    expect(message).toBe(
      'queries.0.searchText: Missing required field: searchText'
    );
  });

  it('reports the missing field of the selected union branch', async () => {
    const message = await sdkMessage('astSearch', {
      operation: 'symbols',
      goal: 'test goal',
      reasoning: 'r',
    });
    expect(message).toBe('queries.0.path: Missing required field: path');
  });

  it('leaves valid input untouched', async () => {
    const definition = DIRECT_TOOL_DEFINITIONS.find(
      d => d.name === 'localSearch'
    )!;
    const schema = toolInputSchema(definition) as Standard;
    const result = await schema['~standard'].validate({
      path: '.',
      searchText: 'a',
      goal: 'test goal',
      reasoning: 'r',
    });
    expect(result.issues).toBeUndefined();
    expect(result.value).toMatchObject({
      queries: [
        { path: '.', searchText: 'a', goal: 'test goal', reasoning: 'r' },
      ],
    });
  });
});

describe('discriminated unions', () => {
  it('lists allowed values when a z.discriminatedUnion discriminator is wrong', async () => {
    const message = await sdkMessage('ghSearchHistory', {
      operation: 'comit',
      keywords: ['a'],
      goal: 'test goal',
      reasoning: 'r',
    });
    expect(message).toBe(
      'queries.0.operation: Value "comit" is outside the allowed enum; allowed: pullRequest, issue, commit (did you mean \'commit\'?)'
    );
  });
});

describe('ghGetHistoryItem mixed batch (09-24 opaque failure regression)', () => {
  const issueRow = (extra: Record<string, unknown> = {}) => ({
    operation: 'issue',
    owner: 'vitest-dev',
    repo: 'vitest',
    number: 2008,
    charLength: 50,
    goal: 'test goal',
    reasoning: 'ok',
    ...extra,
  });

  it('lets a batch with one non-integer charOffset row reach native row isolation', async () => {
    const definition = DIRECT_TOOL_DEFINITIONS.find(
      d => d.name === 'ghGetHistoryItem'
    )!;
    const schema = toolInputSchema(definition) as Standard;
    const input = { queries: [issueRow(), issueRow({ charOffset: 1.5 })] };
    const result = await schema['~standard'].validate(input);
    expect(result.issues).toBeUndefined();
    expect(result.value).toEqual(input);
  });

  it('names the field and reason when the only row is invalid', async () => {
    const message = await sdkMessage('ghGetHistoryItem', {
      queries: [issueRow({ charOffset: 1.5 })],
    });
    expect(message).toMatch(/^queries\.0\.charOffset: .*int/);
  });
});

describe('shape slips found in agent transcripts', () => {
  const clasify = (
    context: Record<string, unknown>,
    question: Record<string, unknown> = { type: 'noul', instructions: 'x' }
  ) => ({
    goal: 'g',
    reasoning: 'r',
    resources: [{ id: 'a', context }],
    questions: [question],
  });

  it('shows the wrapped value when a list field gets a string', async () => {
    const message = await sdkMessage('localSearch', {
      queries: [
        {
          goal: 'g',
          reasoning: 'r',
          path: '.',
          searchText: 'x',
          include: 'src/**',
        },
      ],
    });
    expect(message).toContain(
      'queries.0.include: Expected array; wrap the value: ["src/**"]'
    );
  });

  it('names both forms when a clasify question mixes preset and custom fields', async () => {
    const message = await sdkMessage(
      'clasify',
      clasify(
        { value: 'held' },
        {
          questionType: 'sufficient',
          target: 't',
          type: 'noul',
          instructions: 'x',
        }
      )
    );
    expect(message).toContain('belong to different forms');
    expect(message).toContain('`questionType`');
    expect(message).toContain('`type`');
  });

  it('names both forms when a context mixes value with tool and query', async () => {
    const message = await sdkMessage(
      'clasify',
      clasify({ value: 'x', tool: 'localFetch', query: { path: '/a' } })
    );
    expect(message).toContain('belong to different forms');
    expect(message).toContain('`value`');
  });

  it('lists allowed context tools for a non-read tool', async () => {
    const message = await sdkMessage(
      'clasify',
      clasify({ tool: 'astRewrite', query: { path: '/a' } })
    );
    expect(message).toContain('context.tool: Value "astRewrite" is outside');
    expect(message).toContain('localFetch');
    expect(message).not.toContain("Remove unknown field 'tool'");
  });
});

describe('lossless scalar strings and stringified batches (benchmark input slips)', () => {
  const brief = { goal: 'g', reasoning: 'r' };
  const validate = async (tool: string, args: unknown) => {
    const definition = DIRECT_TOOL_DEFINITIONS.find(d => d.name === tool)!;
    const schema = toolInputSchema(definition) as Standard;
    return schema['~standard'].validate(args);
  };

  it('coerces exact integer and lowercase boolean strings on integer/boolean fields', async () => {
    const result = await validate('localSearch', {
      queries: [
        {
          ...brief,
          path: '.',
          searchText: '10',
          contextLines: '3',
          wholeWord: 'true',
          invertMatch: 'false',
        },
      ],
    });
    expect(result.issues).toBeUndefined();
    expect(result.value).toMatchObject({
      queries: [
        {
          searchText: '10',
          contextLines: 3,
          wholeWord: true,
          invertMatch: false,
        },
      ],
    });
  });

  it('coerces inside a bare query and a union-variant row', async () => {
    const bare = await validate('structureSearch', {
      ...brief,
      path: '.',
      maxDepth: '2',
    });
    expect(bare.issues).toBeUndefined();
    expect(bare.value).toMatchObject({ queries: [{ maxDepth: 2 }] });
    const variant = await validate('ghGetHistoryItem', {
      queries: [
        {
          ...brief,
          operation: 'issue',
          owner: 'o',
          repo: 'r',
          number: '12',
        },
      ],
    });
    expect(variant.issues).toBeUndefined();
    expect(variant.value).toMatchObject({ queries: [{ number: 12 }] });
  });

  it.each([' 3', '3.0', '1e2', '+3', '03', '-0', '0x10', ''])(
    'keeps rejecting the non-canonical integer string %j',
    async contextLines => {
      const message = await sdkMessage('localSearch', {
        queries: [{ ...brief, path: '.', searchText: 'x', contextLines }],
      });
      expect(message).toMatch(/^queries\.0\.contextLines: /);
    }
  );

  it.each(['True', 'TRUE', '1', 'yes'])(
    'keeps rejecting the non-canonical boolean string %j',
    async wholeWord => {
      const message = await sdkMessage('localSearch', {
        queries: [{ ...brief, path: '.', searchText: 'x', wholeWord }],
      });
      expect(message).toMatch(/^queries\.0\.wholeWord: /);
    }
  );

  it('accepts queries sent as a JSON-encoded array', async () => {
    const result = await validate('localSearch', {
      queries: JSON.stringify([{ ...brief, path: '.', searchText: 'x' }]),
    });
    expect(result.issues).toBeUndefined();
    expect(result.value).toMatchObject({
      queries: [{ path: '.', searchText: 'x' }],
    });
  });

  it('never suggests wrapping a JSON string that is not an array', async () => {
    const message = await sdkMessage('localSearch', { queries: '{"a":1}' });
    expect(message).not.toContain('wrap the value');
    expect(message).toContain('queries: Expected an array of query objects');
  });
});

describe('missing discriminators and hidden fields', () => {
  it('names a missing operation with every variant', async () => {
    const message = await sdkMessage('astSearch', {
      queries: [{ goal: 'g', reasoning: 'r', path: '.', pattern: 'x' }],
    });
    expect(message).toBe(
      'queries.0.operation: Missing required field: operation (one of: match, syntaxTree, symbols)'
    );
  });

  it('names a missing discriminated-union operation the same way', async () => {
    const message = await sdkMessage('ghGetHistoryItem', {
      queries: [
        { goal: 'g', reasoning: 'r', owner: 'a', repo: 'b', number: 1 },
      ],
    });
    expect(message).toBe(
      'queries.0.operation: Missing required field: operation (one of: pullRequest, issue, commit, compare)'
    );
  });

  it('lists only agent-composed fields and suggests the nearest one', async () => {
    const message = await sdkMessage('ghGetHistoryItem', {
      queries: [
        {
          goal: 'g',
          reasoning: 'r',
          operation: 'pullRequest',
          owner: 'a',
          repo: 'b',
          number: 1,
          matchStrng: 'x',
        },
      ],
    });
    expect(message).toContain(
      "queries.0: Remove unknown field 'matchStrng' (did you mean 'matchString'?)"
    );
    expect(message).toMatch(/Valid fields: .*matchString/);
    for (const hidden of [
      'debug',
      'filePage',
      'commentBodyOffset',
      'charOffset',
    ])
      expect(message).not.toMatch(
        new RegExp(`Valid fields: .*\\b${hidden}\\b`)
      );
  });
});
