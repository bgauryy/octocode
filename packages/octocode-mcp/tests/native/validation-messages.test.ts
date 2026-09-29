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
