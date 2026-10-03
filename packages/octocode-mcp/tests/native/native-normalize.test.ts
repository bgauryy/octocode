import { afterAll, describe, expect, it } from 'vitest';
import { DIRECT_TOOL_DEFINITIONS } from '@octocodeai/config/schema';
import { loadNativeBinding, toolInputSchema } from '../../src/native/index.js';

// Asserts the MCP input repairs through the real addon's normalizeInput (the
// same native normalization the CLI applies); needs a current native build.
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

const { NativeRuntime } = loadNativeBinding();
const runtime = new NativeRuntime({ surface: 'mcp' });
afterAll(() => runtime.close());

const brief = { goal: 'g', reasoning: 'r' };
const validate = async (tool: string, args: unknown) => {
  const definition = DIRECT_TOOL_DEFINITIONS.find(d => d.name === tool)!;
  const schema = toolInputSchema(definition, value =>
    runtime.normalizeInput(tool, value)
  ) as Standard;
  return schema['~standard'].validate(args);
};
// Mirrors the MCP SDK's validateStandardSchema/formatIssue rendering.
const sdkMessage = async (tool: string, args: unknown) => {
  const result = await validate(tool, args);
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

describe('lossless scalar strings and stringified batches (benchmark input slips)', () => {
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
  it('parses JSON-encoded list fields a host sent as strings', async () => {
    const result = await validate('localSearch', {
      queries: [
        {
          ...brief,
          path: '.',
          searchText: 'x',
          include: '["*.go"]',
          exclude: '["*_test.go"]',
          excludeDir: '["node_modules","dist"]',
        },
      ],
    });
    expect(result.issues).toBeUndefined();
    expect(result.value).toMatchObject({
      queries: [
        {
          include: ['*.go'],
          exclude: ['*_test.go'],
          excludeDir: ['node_modules', 'dist'],
        },
      ],
    });
    const structure = await validate('structureSearch', {
      ...brief,
      path: '.',
      extensions: '["go"]',
    });
    expect(structure.issues).toBeUndefined();
    expect(structure.value).toMatchObject({
      queries: [{ extensions: ['go'] }],
    });
  });

  it('rejects malformed encoded lists without suggesting another string wrapper', async () => {
    const message = await sdkMessage('localSearch', {
      queries: [{ ...brief, path: '.', searchText: 'x', include: '["*.go"' }],
    });
    expect(message).toContain('send a JSON array, not a JSON-encoded string');
    expect(message).not.toContain('wrap the value');
  });

  it('validates every item after parsing an encoded list', async () => {
    const result = await validate('localSearch', {
      queries: [
        { ...brief, path: '.', searchText: 'x', include: '["*.go",42]' },
      ],
    });
    expect(result.issues?.length).toBeGreaterThan(0);
    expect(
      result.issues?.some(
        issue => issue.path?.join('.') === 'queries.0.include.1'
      )
    ).toBe(true);
  });

  it('rejects malformed encoded query batches before any native execution', async () => {
    const message = await sdkMessage('localSearch', {
      queries: '[{"goal":"g","reasoning":"r","path":".","searchText":"x"}',
    });
    expect(message).toContain('queries: Expected an array of query objects');
    expect(message).not.toContain('wrap the value');
  });

  it('wraps a bare scalar sent for a list of that scalar', async () => {
    const result = await validate('ghSearchCode', {
      ...brief,
      owner: 'o',
      keywords: 'wrap_app_handling_exceptions',
    });
    expect(result.issues).toBeUndefined();
    expect(result.value).toMatchObject({
      queries: [{ keywords: ['wrap_app_handling_exceptions'] }],
    });
  });

  it.each([
    ['70,130', ['70-130']],
    [[' 140-150'], ['140-150']],
    [['248', '325'], ['248-325']],
    [[248, 325], ['248-325']],
  ])(
    'repairs the host line-range spelling %j for localFetch and ghGetFileContent',
    async (ranges, expected) => {
      for (const [tool, extra] of [
        ['localFetch', { path: 'a.ts' }],
        ['ghGetFileContent', { owner: 'o', repo: 'r', path: 'a.ts' }],
      ] as const) {
        const result = await validate(tool, {
          queries: [{ ...brief, ...extra, ranges }],
        });
        expect(result.issues).toBeUndefined();
        expect(result.value).toMatchObject({ queries: [{ ranges: expected }] });
      }
    }
  );

  it('keeps rejecting a range spelling it cannot read losslessly', async () => {
    const message = await sdkMessage('localFetch', {
      queries: [{ ...brief, path: 'a.ts', ranges: ['3:5'] }],
    });
    expect(message).toMatch(/^queries\.0\.ranges\.0: /);
  });
});
