import { describe, expect, it } from 'vitest';
import { DIRECT_TOOL_DEFINITIONS } from '@octocodeai/config/schema';
import { toolInputSchema } from '../../src/native/index.js';
import { wrapBareQuery } from './wrapBareQuery.js';

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
  const schema = toolInputSchema(definition, wrapBareQuery) as Standard;
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
          mainGoal: 'g',
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
      mainGoal: 'test goal',
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
      mainGoal: 'test goal',
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
      mainGoal: 'test goal',
      reasoning: 'r',
    });
    expect(message).toBe(
      'queries.0.searchText: Missing required field: searchText'
    );
  });

  it('reports the missing field of the selected union branch', async () => {
    const message = await sdkMessage('astSearch', {
      operation: 'symbols',
      mainGoal: 'test goal',
      reasoning: 'r',
    });
    expect(message).toBe('queries.0.path: Missing required field: path');
  });

  it('leaves valid input untouched', async () => {
    const definition = DIRECT_TOOL_DEFINITIONS.find(
      d => d.name === 'localSearch'
    )!;
    const schema = toolInputSchema(definition, wrapBareQuery) as Standard;
    const result = await schema['~standard'].validate({
      path: '.',
      searchText: 'a',
      mainGoal: 'test goal',
      reasoning: 'r',
    });
    expect(result.issues).toBeUndefined();
    expect(result.value).toMatchObject({
      queries: [
        { path: '.', searchText: 'a', mainGoal: 'test goal', reasoning: 'r' },
      ],
    });
  });
});

describe('discriminated unions', () => {
  it('lists allowed values when a z.discriminatedUnion discriminator is wrong', async () => {
    const message = await sdkMessage('ghSearchHistory', {
      operation: 'comit',
      keywords: ['a'],
      mainGoal: 'test goal',
      reasoning: 'r',
    });
    expect(message).toBe(
      'queries.0.operation: Value "comit" is outside the allowed enum; allowed: pullRequest, issue, commit (did you mean \'commit\'?)'
    );
  });
});

describe('a field sent with the wrong operation names its operation (CLI parity)', () => {
  const row = (extra: Record<string, unknown>) => ({
    owner: 'cli',
    repo: 'cli',
    mainGoal: 'g',
    reasoning: 'r',
    ...extra,
  });

  it.each(['issue', 'commit'])(
    'points a pullRequest-only field on an %s query at operation:"pullRequest"',
    async operation => {
      const message = await sdkMessage('ghSearchHistory', {
        queries: [row({ operation, review: 'approved' })],
      });
      expect(message).toContain(
        `queries.0: Remove 'review' from queries[0]: it applies only with operation:"pullRequest".`
      );
      expect(message).not.toContain('send one shape');
      expect(message).not.toMatch(/Valid fields: .*\breview\b/);
    }
  );

  it('names a ghGetHistoryItem commit-only field on an issue query', async () => {
    const message = await sdkMessage('ghGetHistoryItem', {
      queries: [row({ operation: 'issue', number: 1, ref: 'main' })],
    });
    expect(message).toContain(
      `queries.0: Remove 'ref' from queries[0]: it applies only with operation:"commit".`
    );
    expect(message).toMatch(/Valid fields: .*\bnumber\b/);
    expect(message).not.toMatch(/Valid fields: .*\bref\b/);
  });

  it('lists both operations for a field two branches share', async () => {
    const message = await sdkMessage('ghSearchHistory', {
      queries: [row({ operation: 'commit', label: ['bug'] })],
    });
    expect(message).toContain(
      `queries.0: Remove 'label' from queries[0]: it applies only with operation:"pullRequest" or operation:"issue".`
    );
    expect(message).not.toContain('send one shape');
  });

  it('keeps naming a missing required field before the operation', async () => {
    const message = await sdkMessage('ghGetHistoryItem', {
      queries: [row({ operation: 'commit', ref: 'main', commentPage: 2 })],
    });
    expect(message).toContain(
      "queries.0: Remove 'commentPage' from queries[0]: it applies only with number"
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
    mainGoal: 'test goal',
    reasoning: 'ok',
    ...extra,
  });

  it('lets a batch with one non-integer charOffset row reach native row isolation', async () => {
    const definition = DIRECT_TOOL_DEFINITIONS.find(
      d => d.name === 'ghGetHistoryItem'
    )!;
    const schema = toolInputSchema(definition, wrapBareQuery) as Standard;
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

describe('batch and brief placement slips from recorded sessions (CLI parity)', () => {
  it('says how to split a batch over the row limit', async () => {
    const message = await sdkMessage('localFetch', {
      queries: Array.from({ length: 7 }, () => ({
        path: 'package.json',
        mainGoal: 'g',
        reasoning: 'r',
      })),
    });
    expect(message).toContain(
      'Send at most 5 rows per call: split the batch into 2 calls.'
    );
  });

  it('moves a top-level goal into each queries row', async () => {
    const message = await sdkMessage('ghSearchCode', {
      goal: 'g',
      queries: [
        {
          owner: 'o',
          repo: 'r',
          keywords: ['k'],
          mainGoal: 'g',
          reasoning: 'r',
        },
      ],
    });
    expect(message).toContain(
      "Move 'goal' into each queries[] row: a top-level goal is not inherited."
    );
  });

  it('moves a top-level mainGoal into each queries row', async () => {
    const message = await sdkMessage('localSearch', {
      mainGoal: 'g',
      queries: [{ path: '.', searchText: 'x' }],
    });
    expect(message).toContain(
      "Move 'mainGoal' into each queries[] row: a top-level mainGoal is not inherited."
    );
  });

  it('accepts a row without a brief', async () => {
    expect(
      await sdkMessage('localSearch', {
        queries: [{ path: '.', searchText: 'x' }],
      })
    ).toBeUndefined();
  });
});

describe('shape slips found in agent transcripts', () => {
  const clasify = (
    context: Record<string, unknown>,
    question: Record<string, unknown> = { type: 'noul', instructions: 'x' }
  ) => ({
    mainGoal: 'g',
    reasoning: 'r',
    resources: [{ id: 'a', context }],
    questions: [question],
  });

  it('shows the wrapped value when a list field gets a string', async () => {
    const message = await sdkMessage('localSearch', {
      queries: [
        {
          mainGoal: 'g',
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

  it('asks for a JSON array, not a wrap, for a malformed encoded list', async () => {
    const message = await sdkMessage('localSearch', {
      queries: [
        {
          mainGoal: 'g',
          reasoning: 'r',
          path: '.',
          searchText: 'x',
          include: '["*.go"',
        },
      ],
    });
    expect(message).toBe(
      'queries.0.include: Expected array; send a JSON array, not a JSON-encoded string'
    );
  });

  it('never suggests wrapping a string into a list of objects', async () => {
    const message = await sdkMessage('clasify', {
      mainGoal: 'g',
      reasoning: 'r',
      resources: 'a',
      questions: [{ type: 'noul', instructions: 'x' }],
    });
    expect(message).not.toContain('wrap the value');
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

  it('asks for one shape when a clasify batch also carries matrix fields', async () => {
    const message = await sdkMessage('clasify', {
      queries: [clasify({ value: 'x' })],
      mainGoal: 'x',
    });
    expect(message).toContain(
      "Remove 'mainGoal' from the request: it applies only with resources and questions"
    );
    expect(message).toContain('send one shape');
    // Never name the rejected field as valid in the same breath.
    expect(message).not.toMatch(/Valid fields: .*\bmainGoal\b/);
    expect(message).toContain('Valid fields: queries');
  });

  it('names the cell count and the fileChunks rule over the cell limit (CLI parity)', async () => {
    const message = await sdkMessage('clasify', {
      ...clasify({ value: 'x' }),
      resources: Array.from({ length: 6 }, (_, i) => ({
        id: `r${i}`,
        context: { value: 'x' },
      })),
      questions: Array.from({ length: 5 }, (_, i) => ({
        id: `q${i}`,
        type: 'noul',
        instructions: 'x',
      })),
    });
    expect(message).toBe(
      'queries.0: Expanded resources × questions produces 30 cells; maximum is 25 (a fileChunks resource counts as 5 resources).'
    );
  });

  it('rejects prefilter on a search at validation with the CLI wording', async () => {
    const message = await sdkMessage('clasify', {
      ...clasify({
        tool: 'localSearch',
        query: { path: '/r', searchText: 'x' },
      }),
      resources: [
        {
          id: 'a',
          prefilter: ['x'],
          context: {
            tool: 'localSearch',
            query: { path: '/r', searchText: 'x' },
          },
        },
      ],
    });
    expect(message).toBe(
      'queries.0.resources.0.prefilter: prefilter applies only to localFetch or ghGetFileContent file reads; remove it or narrow the search itself.'
    );
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

describe('missing discriminators and hidden fields', () => {
  it('names a missing operation with every variant', async () => {
    const message = await sdkMessage('astSearch', {
      queries: [{ mainGoal: 'g', reasoning: 'r', path: '.', pattern: 'x' }],
    });
    expect(message).toBe(
      'queries.0.operation: Missing required field: operation (one of: match, syntaxTree, symbols)'
    );
  });

  it('names a missing discriminated-union operation the same way', async () => {
    const message = await sdkMessage('ghGetHistoryItem', {
      queries: [
        { mainGoal: 'g', reasoning: 'r', owner: 'a', repo: 'b', number: 1 },
      ],
    });
    expect(message).toBe(
      'queries.0.operation: Missing required field: operation (one of: pullRequest, issue, commit, compare)'
    );
  });

  it('lists canonical fields the published view hides, never continuation-only ones', async () => {
    const message = await sdkMessage('localSearch', {
      queries: [
        {
          mainGoal: 'g',
          reasoning: 'r',
          path: '.',
          searchText: 'x',
          madeUp: 1,
        },
      ],
    });
    expect(message).toMatch(/Valid fields: .*\bmaxMatchesPerFile\b/);
    expect(message).toMatch(/Valid fields: .*\bpageSize\b/);
    for (const hidden of ['debug', 'page', 'matchPage', 'snapshot'])
      expect(message).not.toMatch(
        new RegExp(`Valid fields: .*\\b${hidden}\\b`)
      );
    const lsp = await sdkMessage('lspSearch', {
      queries: [
        {
          mainGoal: 'g',
          reasoning: 'r',
          operation: 'callers',
          uri: 'a.ts',
          symbolName: 'x',
          lineHint: 1,
          madeUp: 1,
        },
      ],
    });
    expect(lsp).toMatch(/Valid fields: .*\bdepth\b/);
  });

  it('lists only agent-composed fields and suggests the nearest one', async () => {
    const message = await sdkMessage('ghGetHistoryItem', {
      queries: [
        {
          mainGoal: 'g',
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
