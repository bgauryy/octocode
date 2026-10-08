import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { z, type ZodType } from 'zod';
import {
  commandJsonSchema,
  commandToken,
  defineCli,
  defineCommand,
  flagToken,
  flagsFromJsonSchema,
  flagsFromZod,
  parseCommandInput,
  usagePattern,
  withCommands,
  type Flag,
  type JsonSchema,
} from '../src/spec.js';

function view(flag: Flag) {
  return {
    property: flag.property,
    name: flag.name,
    kind: flag.kind,
    required: flag.required,
    presence: flag.presence,
    partial: flag.partial,
    hasDefault: flag.hasDefault,
    description: flag.description,
    defaultValue: flag.defaultValue,
    enumValues: flag.enumValues,
    itemKind: flag.itemKind,
  };
}

const zodSchema = z.object({
  query: z.string().describe('Search text'),
  limit: z.number().int(),
  state: z.enum(['open', 'closed']),
  force: z.boolean().default(false),
  labels: z.array(z.string()),
});

const jsonSchema: JsonSchema = {
  type: 'object',
  properties: {
    query: { type: 'string', description: 'Search text' },
    limit: { type: 'integer' },
    state: { type: 'string', enum: ['open', 'closed'] },
    force: { type: 'boolean', default: false },
    labels: { type: 'array', items: { type: 'string' } },
  },
  required: ['query', 'limit', 'state', 'labels'],
};

describe('flag parity', () => {
  it('matches Zod and JSON Schema flags for the supported subset', () => {
    const zodFlags = flagsFromZod(zodSchema).flags.map(view);
    const jsonFlags = flagsFromJsonSchema(jsonSchema).flags.map(view);
    expect(jsonFlags).toEqual(zodFlags);
    expect(zodFlags.map(flag => flag.kind)).toEqual(['string', 'integer', 'enum', 'boolean', 'array']);
    expect(zodFlags.find(flag => flag.name === 'force')).toMatchObject({ presence: true, required: false, defaultValue: false });
    expect(zodFlags.find(flag => flag.name === 'labels')).toMatchObject({ itemKind: 'string', required: true });
  });

  it('treats a default as optional even when JSON Schema still requires it', () => {
    const flags = flagsFromJsonSchema({
      type: 'object',
      properties: { query: { type: 'string', default: 'x' } },
      required: ['query'],
    }).flags;
    expect(flags[0]).toMatchObject({ required: false, hasDefault: true, defaultValue: 'x' });
    const zodFlags = flagsFromZod(z.object({ query: z.string().default('x') })).flags;
    expect(zodFlags[0]).toMatchObject({ required: false, hasDefault: true, defaultValue: 'x' });
    expect(z.toJSONSchema(z.object({ query: z.string().default('x') })).required).toContain('query');
  });

  it('keeps unsupported keywords as partial json flags', () => {
    const built = flagsFromJsonSchema({
      type: 'object',
      properties: {
        query: { type: 'string' },
        filter: { $ref: '#/$defs/filter' },
        choice: { oneOf: [{ type: 'string' }, { type: 'integer' }] },
      },
      required: ['query', 'filter', 'choice'],
      patternProperties: { '^x-': { type: 'string' } },
      additionalProperties: { type: 'string' },
    });
    expect(built.flags.map(flag => flag.property)).toEqual(['query', 'filter', 'choice']);
    expect(built.flags.find(flag => flag.property === 'filter')).toMatchObject({ kind: 'json', partial: true });
    expect(built.flags.find(flag => flag.property === 'choice')).toMatchObject({ kind: 'json', partial: true });
    expect(built.notes).toEqual(expect.arrayContaining(['$ref', 'oneOf', 'patternProperties', 'additionalProperties', 'partial']));
  });

  it('does not record additionalProperties when it is false', () => {
    const built = flagsFromJsonSchema({
      type: 'object',
      properties: { query: { type: 'string' } },
      additionalProperties: false,
    });
    expect(built.notes).not.toContain('additionalProperties');
    expect(built.flags).toHaveLength(1);
  });

  it('suffixes reserved flag names and disambiguates kebab collisions', () => {
    const flags = flagsFromJsonSchema({
      type: 'object',
      properties: {
        json: { type: 'string' },
        jsonValue: { type: 'string' },
        pageSize: { type: 'integer' },
      },
      required: ['json', 'jsonValue', 'pageSize'],
    }).flags;
    expect(flags.map(flag => flag.name)).toEqual(['json-value', 'json-value-2', 'page-size']);
  });

  it('turns a non-object root into one json input flag', () => {
    expect(flagsFromZod(z.string()).flags[0]).toMatchObject({ name: 'input', kind: 'json', partial: true, required: true });
    expect(flagsFromJsonSchema({ type: 'string' }).flags[0]).toMatchObject({ name: 'input', kind: 'json', partial: true });
    expect(flagsFromJsonSchema({ type: 'object' }).flags).toEqual([]);
    expect(flagsFromJsonSchema({ type: 'object', additionalProperties: false }).flags).toEqual([]);
    expect(flagsFromJsonSchema({}).flags).toEqual([]);
    const empty = defineCommand({
      name: 'now',
      description: 'Now',
      inputSchema: { type: 'object' },
      run: input => input,
    });
    expect(parseCommandInput(empty, {})).toEqual({});
  });

  it('records fidelity notes for refinements, unions, and nullable values', () => {
    const refined = defineCommand({
      name: 'search',
      description: 'Search',
      schema: z.object({
        query: z.string().refine(value => value.length > 1),
        code: z.string().min(2),
        mode: z.union([z.literal('a'), z.literal('b')]),
        rank: z.union([z.literal(1), z.literal(2)]),
        mixed: z.union([z.string(), z.number()]),
        maybe: z.string().nullable(),
        rows: z.array(z.object({ id: z.string() })),
        count: z.number(),
        fallback: z.string().prefault('x'),
        locked: z.string().readonly(),
        caught: z.string().catch('y'),
        dynamic: z.string().default(() => 'dyn'),
        titled: z.string().describe('Inner').optional(),
        literal: z.literal('only'),
        active: z.boolean().default(true),
        enabled: z.boolean(),
      }),
      run: input => input,
    });
    expect(refined.fidelityNotes).toEqual(expect.arrayContaining(['refine', 'constraint', 'union', 'nullable', 'array', 'partial']));
    expect(refined.flags.find(flag => flag.property === 'mode')?.enumValues).toEqual(['a', 'b']);
    expect(refined.flags.find(flag => flag.property === 'rank')?.enumValues).toEqual([1, 2]);
    expect(refined.flags.find(flag => flag.property === 'mixed')).toMatchObject({ kind: 'json', partial: true });
    expect(refined.flags.find(flag => flag.property === 'maybe')).toMatchObject({ kind: 'json', partial: true });
    expect(refined.flags.find(flag => flag.property === 'rows')).toMatchObject({ kind: 'json', partial: true });
    expect(refined.flags.find(flag => flag.property === 'count')).toMatchObject({ kind: 'number' });
    expect(refined.flags.find(flag => flag.property === 'fallback')).toMatchObject({ required: false, defaultValue: 'x' });
    expect(refined.flags.find(flag => flag.property === 'dynamic')?.defaultValue).toBe('dyn');
    expect(refined.flags.find(flag => flag.property === 'titled')).toMatchObject({ description: 'Inner', required: false });
    expect(refined.flags.find(flag => flag.property === 'literal')?.enumValues).toEqual(['only']);
    expect(refined.flags.find(flag => flag.property === 'active')).toMatchObject({ presence: false, kind: 'boolean' });
    expect(flagToken(refined.flags.find(flag => flag.property === 'enabled') as Flag)).toBe('--enabled <true|false>');
  });

  it('rejects object-valued enums and maps type arrays to partial json', () => {
    const flags = flagsFromJsonSchema({
      type: 'object',
      properties: {
        odd: { type: 'string', enum: [{ bad: true }] },
        mixed: { type: ['string', 'number'] },
        open: true,
        items: { type: 'array' },
      },
    }).flags;
    expect(flags.find(flag => flag.property === 'odd')).toMatchObject({ kind: 'string', partial: false });
    expect(flags.find(flag => flag.property === 'mixed')).toMatchObject({ kind: 'json', partial: true });
    expect(flags.find(flag => flag.property === 'open')).toMatchObject({ kind: 'json', partial: true });
    expect(flags.find(flag => flag.property === 'items')).toMatchObject({ kind: 'json', partial: true });
  });
});

describe('commands', () => {
  it('aliases illegal and reserved MCP names', () => {
    expect(commandToken('search')).toEqual({ token: 'search', aliased: false });
    expect(commandToken('issues.search')).toEqual({ token: 'issues.search', aliased: false });
    expect(commandToken('123bad')).toEqual({ token: '123bad', aliased: false });
    expect(commandToken('...help')).toEqual({ token: '...help', aliased: false });
    expect(commandToken('help')).toEqual({ token: 'help-command', aliased: true });
    expect(commandToken('version')).toEqual({ token: 'version-command', aliased: true });
    expect(commandToken('---')).toEqual({ token: 'tool-command', aliased: true });
    expect(commandToken('a b')).toEqual({ token: 'a-b', aliased: true });
  });

  it('rejects duplicate, reserved, and incomplete commands', () => {
    const command = defineCommand({
      name: 'search',
      description: 'Search',
      schema: zodSchema,
      run: input => input,
    });
    expect(defineCommand({ name: 'a.b', description: 'd', schema: z.object({}), run: () => 1 }).name).toBe('a.b');
    expect(() => defineCommand({ name: 'a b', description: 'd', schema: z.object({}), run: () => 1 })).toThrow(/Command name/);
    expect(() => defineCommand({ name: 'help', description: 'd', schema: z.object({}), run: () => 1 })).toThrow(/Reserved/);
    expect(() => defineCommand({ name: 'search', description: 'd', source: 'zod', run: () => 1 })).toThrow(/schema/);
    expect(() => defineCommand({ name: 'search', description: 'd', source: 'mcp', run: () => 1 })).toThrow(/inputSchema/);
    expect(() => defineCli({ name: 'issues', instructions: '', commands: [command, command] })).toThrow(/Duplicate/);
    expect(() => defineCli({ name: 'issues', instructions: '', commands: [{ ...command, name: 'version' }] })).toThrow(/Reserved/);
    expect(() => withCommands(defineCli({ name: 'issues', instructions: 'i', version: '1', commands: [command] }), [command])).toThrow(/Duplicate/);
  });

  it('builds usage from required flags first', () => {
    const command = defineCommand({
      name: 'search',
      description: 'Search issues by text.',
      schema: zodSchema,
      run: input => input,
    });
    expect(usagePattern('issues', 'search', command.flags)).toBe(
      'issues search --query <string> --limit <integer> --state <open|closed> --labels <string> [--force]',
    );
    expect(flagToken(command.flags[0] as Flag)).toBe('--query <string>');
    expect(commandJsonSchema(command)).toEqual(z.toJSONSchema(zodSchema, { io: 'input' }));
    const imported = defineCommand({
      name: 'search',
      description: 'Search',
      inputSchema: jsonSchema,
      run: input => input,
    });
    expect(commandJsonSchema(imported)).toBe(jsonSchema);
  });

  it('parses Zod and JSON Schema input, including root json values', () => {
    const command = defineCommand({
      name: 'search',
      description: 'Search',
      schema: z.object({
        query: z.string().min(2),
        force: z.boolean().default(false),
      }),
      run: input => input,
    });
    expect(parseCommandInput(command, { query: 'ab' })).toEqual({ query: 'ab', force: false });
    expect(parseCommandInput(command, { query: 'ab' }).query).toBe('ab');
    expect(() => parseCommandInput(command, { query: 'a' })).toThrow(/query:/);
    const imported = defineCommand({
      name: 'search',
      description: 'Search',
      inputSchema: {
        type: 'object',
        properties: { query: { type: 'string' } },
        required: ['query'],
      },
      run: input => input,
    });
    expect(parseCommandInput(imported, { query: 'ab' })).toEqual({ query: 'ab' });
    expect(() => parseCommandInput(imported, {})).toThrow();
    const echo = defineCommand({
      name: 'echo',
      description: 'Echo',
      schema: z.string(),
      run: input => input,
    });
    expect(parseCommandInput(echo, { input: 'hi' })).toEqual({ input: 'hi' });
    const raw = defineCommand({
      name: 'echo',
      description: 'Echo',
      inputSchema: { type: 'string' },
      run: input => input,
    });
    expect(parseCommandInput(raw, { input: 'hi' })).toEqual({ input: 'hi' });
    const broken = {
      ...command,
      schema: {
        def: { type: 'object', shape: {} },
        parse: () => { throw new Error('boom'); },
      } as unknown as ZodType,
    };
    expect(() => parseCommandInput(broken, { query: 'ab' })).toThrow('boom');
    expect(() => commandJsonSchema({ ...command, schema: undefined })).toThrow(/schema/);
    expect(() => commandJsonSchema({ ...imported, inputSchema: undefined })).toThrow(/inputSchema/);
    expect(() => parseCommandInput({ ...imported, inputSchema: undefined }, {})).toThrow(/inputSchema/);
  });
});

describe('package isolation', () => {
  it('stays private and is not a dependency of the launcher or MCP server', () => {
    const own = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')) as { private: boolean };
    expect(own.private).toBe(true);
    for (const name of ['octocode', 'octocode-mcp']) {
      const manifest = JSON.parse(readFileSync(new URL(`../../${name}/package.json`, import.meta.url), 'utf8')) as {
        dependencies?: Record<string, string>;
        devDependencies?: Record<string, string>;
        peerDependencies?: Record<string, string>;
      };
      const deps = { ...manifest.dependencies, ...manifest.devDependencies, ...manifest.peerDependencies };
      expect(deps['octocode-mcp-cli']).toBeUndefined();
    }
  });
});


describe('authored input JSON Schema', () => {
  it('keeps defaulted fields optional while preserving input constraints and required fields', () => {
    const schema = z.object({
      query: z.string().min(2).refine(value => value !== 'blocked'),
      args: z.array(z.string()).default([]),
      count: z.string(),
    });
    const command = defineCommand({ name: 'input', description: 'Input', schema, run: input => input });
    const json = commandJsonSchema(command);
    expect(json.required).toEqual(['query', 'count']);
    expect(json.properties).toMatchObject({ query: { type: 'string', minLength: 2 }, args: { type: 'array', default: [] }, count: { type: 'string' } });
    expect(parseCommandInput(command, { query: 'ok', count: 'three' })).toMatchObject({ args: [], count: 'three' });
    expect(() => parseCommandInput(command, { query: 'blocked', count: 'x' })).toThrow();
    const transformed = defineCommand({ name: 'transform', description: 'Transform', schema: z.object({ length: z.string().transform(value => value.length) }), run: input => input });
    expect(commandJsonSchema(transformed).properties).toMatchObject({ length: { type: 'string' } });
    expect(parseCommandInput(transformed, { length: 'three' })).toEqual({ length: 5 });
  });
});
