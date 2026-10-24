// Help rendering of JSON Schema inputs: enums, array rows, mixed shapes, conditional requirements.
import Ajv2020 from 'ajv/dist/2020.js';
import { expect, it } from 'vitest';
import { z } from 'zod';
import { commandHelp } from '../src/help.js';
import { defineCli, defineCommand } from '../src/spec.js';

const spec = defineCli({ name: 'issues', instructions: 'Use the index before a command.', commands: [] });

it('quotes spaced enum values in usage, flags and examples', () => {
  const located = defineCommand({
    name: 'get-structured-content',
    description: 'Weather',
    inputSchema: {
      type: 'object',
      properties: {
        location: { type: 'string', enum: ['New York', 'Chicago'] },
      },
      required: ['location'],
    },
    run: () => 'ok',
  });
  const locatedHelp = commandHelp(spec, located);
  expect(locatedHelp).toContain('--location <"New York"|Chicago>');
  expect(locatedHelp).toContain('--location string');
  expect(locatedHelp).toContain('{"New York"|Chicago}');
  expect(locatedHelp).toContain('--location "New York"');
});

it('documents array-of-object rows and builds an example the schema accepts', () => {
  const queries = {
    type: 'object',
    required: ['queries'],
    properties: {
      queries: {
        type: 'array',
        items: {
          type: 'object',
          required: ['matchString', 'path'],
          properties: {
            matchString: { type: 'string', description: 'Text or regex' },
            path: { type: 'string' },
            regex: { type: 'string', enum: ['literal', 'rust'], default: 'rust' },
          },
        },
      },
    },
  };
  const search = defineCommand({
    name: 'localSearch',
    description: 'Find text.',
    inputSchema: queries,
    run: () => 'ok',
  });
  const searchHelp = commandHelp(spec, search);
  const example = `--queries '[{"matchString":"value","path":"value"}]'`;
  expect(searchHelp).toContain(example);
  expect(searchHelp).toContain('[ ].matchString string');
  expect(searchHelp).toContain('Text or regex (required)');
  expect(searchHelp).toContain('[ ].path string');
  expect(searchHelp).toContain('[ ].regex string');
  expect(searchHelp).toContain('{literal|rust}');
  expect(searchHelp).toContain('(default "rust")');
  const ajv = new Ajv2020({ allErrors: true, strict: false, validateSchema: false });
  const validate = ajv.compile(queries);
  expect(validate({ queries: [{ matchString: 'value', path: 'value' }] })).toBe(true);
});

it('renders every JSON Schema shape of a mixed object input', () => {
  const mixed = defineCommand({
    name: 'bag',
    description: 'Mixed JSON.',
    inputSchema: {
      type: 'object',
      required: ['choice', 'bag'],
      properties: {
        choice: {
          anyOf: [
            {
              type: 'object',
              required: ['name'],
              properties: { name: { type: 'string', description: 'Display\nname' } },
            },
            { type: 'string' },
          ],
        },
        bag: {
          type: 'object',
          required: ['count', 'ratio', 'ok', 'empty', 'title', 'code', 'loop', 'gone', 'rows'],
          properties: {
            count: { type: 'integer', minimum: 4 },
            ratio: { type: 'number', minimum: 2.5 },
            ok: { type: 'boolean' },
            empty: { type: 'null' },
            title: { const: "a'b" },
            nested: { type: 'object', properties: { id: { type: 'string' } } },
            tags: { type: 'array', items: { type: 'string' } },
            nums: { type: 'array', items: { type: 'integer' } },
            scores: { type: 'array', items: { type: 'number' } },
            flags: { type: 'array', items: { type: 'boolean' } },
            label: { type: ['string', 'null'] },
            extra: { description: 'Free form' },
            code: { $ref: '#/$defs/Code' },
            loop: { $ref: '#/$defs/Loop' },
            gone: { $ref: '#/nope' },
            rows: { type: 'array' },
          },
        },
      },
      $defs: {
        Code: { type: 'string', enum: ['New York'] },
        Loop: { $ref: '#/$defs/Loop' },
      },
    },
    run: () => 'ok',
  });
  const mixedHelp = commandHelp(spec, mixed);
  expect(mixedHelp).toContain('shape 1');
  expect(mixedHelp).toContain('name string');
  expect(mixedHelp).toContain('Display name (required)');
  expect(mixedHelp).toContain('shape 2');
  expect(mixedHelp).toContain('count int');
  expect(mixedHelp).toContain('ratio number');
  expect(mixedHelp).toContain('ok boolean');
  expect(mixedHelp).toContain('empty null');
  expect(mixedHelp).toContain('nested json');
  expect(mixedHelp).toContain('id string');
  expect(mixedHelp).toContain('tags strings');
  expect(mixedHelp).toContain('nums ints');
  expect(mixedHelp).toContain('scores numbers');
  expect(mixedHelp).toContain('flags bools');
  expect(mixedHelp).toContain('label string');
  expect(mixedHelp).toContain('extra json');
  expect(mixedHelp).toContain('Free form');
  expect(mixedHelp).toContain('code string');
  expect(mixedHelp).toContain('{"New York"}');
  expect(mixedHelp).toContain('"a\'b"');
  expect(mixedHelp).not.toContain('EXAMPLES');
});

it('wraps a non-object input schema as one --input flag', () => {
  const text = defineCommand({
    name: 'raw',
    description: 'Raw text.',
    inputSchema: { type: 'string' },
    run: () => 'ok',
  });
  expect(commandHelp(spec, text)).toContain(`--input '"value"'`);
});

it('renders the same screen for a Zod command and the same JSON Schema', () => {
  const zodCommand = defineCommand({
    name: 'search',
    description: 'Search issues by text.',
    schema: z.object({
      query: z.string().describe('Search text'),
      limit: z.number().int().optional(),
      force: z.boolean().default(false).describe('Force'),
      payload: z.object({ id: z.string() }),
    }),
    run: () => 'ok',
  });
  const mcpCommand = defineCommand({
    name: 'search',
    description: 'Search issues by text.',
    inputSchema: {
      type: 'object',
      required: ['query', 'payload'],
      properties: {
        query: { type: 'string', description: 'Search text' },
        limit: { type: 'integer' },
        force: { type: 'boolean', default: false, description: 'Force' },
        payload: { type: 'object', required: ['id'], properties: { id: { type: 'string' } } },
      },
    },
    run: () => 'ok',
  });
  const zodHelp = commandHelp(spec, zodCommand);
  const mcpHelp = commandHelp(spec, mcpCommand);
  for (const help of [zodHelp, mcpHelp]) {
    expect(help).toContain('USAGE');
    expect(help).toContain('issues search --query <string> --payload <json>');
    expect(help).toContain('[--limit <integer>]');
    expect(help).toContain('[--force]');
    expect(help).toContain('--query string');
    expect(help).toContain('Search text (required)');
    expect(help).toContain('--limit int');
    expect(help).toContain('--force');
    expect(help).toContain('Force (default false)');
    expect(help).toContain('--payload json');
    expect(help).toContain('id string');
    expect(help).toContain('(required)');
    expect(help).toContain(`--payload '{"id":"value"}'`);
  }
});

it('includes conditional required fields and refuses invalid supplied examples', () => {
  const send = defineCommand({ name: 'send', description: 'Send', inputSchema: {
    type: 'object', properties: { body: { type: 'string' }, to: { type: 'string' }, topic: { type: 'string' } },
    required: ['body'], oneOf: [{ required: ['to'] }, { required: ['topic'] }], additionalProperties: false,
  }, run: input => input });
  expect(commandHelp(spec, send)).toContain('--body "value" --to "value"');
  const composed = defineCommand({ name: 'compose', description: 'Compose', inputSchema: {
    type: 'object', properties: { to: { type: 'string' }, topic: { type: 'string' }, note: { type: 'string' }, task: { type: 'object', title: 'Task contract', properties: { goal: { type: 'string' } }, required: ['goal'] } },
    allOf: [{ oneOf: [{ required: ['to'] }, { required: ['topic'] }] }, { oneOf: [{ required: ['note'] }, { required: ['task'] }] }], additionalProperties: false,
  }, run: input => input });
  const composedHelp = commandHelp(spec, composed);
  expect(composedHelp).toContain('--to "value" --note "value"');
  expect(composedHelp).toContain('Task contract');
  const explicit = defineCommand({ name: 'example', description: 'Example', inputSchema: {
    type: 'object', properties: { text: { type: 'string', pattern: '^chosen$' }, force: { type: 'boolean', default: false } }, required: ['text'],
    examples: [{ text: 'invalid' }, { text: 'chosen', force: true }],
  }, run: input => input });
  expect(commandHelp(spec, explicit)).toContain('--text chosen --force');
  const invalid = defineCommand({ name: 'invalid', description: 'Invalid', inputSchema: {
    type: 'object', properties: { text: { type: 'string', pattern: '^chosen$' } }, required: ['text'], examples: [{ text: 'invalid' }],
  }, run: input => input });
  expect(commandHelp(spec, invalid)).not.toContain('EXAMPLES');
});
