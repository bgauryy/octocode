import Ajv2020 from 'ajv/dist/2020.js';
import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { commandHelp, commandHelpJson, defaultOutput, rootHelp, rootHelpJson, stripAnsi } from '../src/help.js';
import { defineCli, defineCommand } from '../src/spec.js';

const instructions = 'Use the index before a command.';
const description = 'Search issues by text.';

const command = defineCommand({
  name: 'search',
  description,
  schema: z.object({
    query: z.string().describe('Search text'),
    limit: z.number().int(),
    ratio: z.number(),
    state: z.enum(['open', 'closed']),
    active: z.boolean(),
    labels: z.array(z.string()),
    counts: z.array(z.number().int()),
    scores: z.array(z.number()),
    bits: z.array(z.boolean()),
    payload: z.object({ id: z.string() }),
    force: z.boolean().default(false).describe('Force'),
    note: z.string().optional(),
  }),
  run: input => input,
});

const spec = defineCli({ name: 'issues', instructions, commands: [command] });

describe('help contract', () => {
  it('prints instructions once in the default output and root help', () => {
    const bare = defaultOutput(spec);
    const root = rootHelp(spec);
    expect(bare.split(instructions)).toHaveLength(2);
    expect(root.split(instructions)).toHaveLength(2);
    expect(bare).toContain('USAGE');
    expect(bare).toContain('issues <command> [flags]');
    expect(bare).toContain('COMMANDS');
    expect(bare).toContain('search:  Search issues by text.');
    expect(bare).not.toContain('\nFLAGS\n');
    expect(root).toContain('FLAGS');
    expect(root).toContain('-h, --help');
    expect(root).toContain('--version');
    expect(root).toContain('LEARN MORE');
    expect(root).toContain('issues <command> [flags]');
    expect(rootHelpJson(spec)).toEqual({
      instructions,
      commands: [{ name: 'search', description }],
    });
  });

  it('starts command help with the description and omits instructions', () => {
    const help = commandHelp(spec, command);
    expect(help.startsWith(description)).toBe(true);
    expect(help).toContain('issues search --query <string> --limit <integer>');
    expect(help).toContain('[--force]');
    expect(help).toContain('[--note <string>]');
    expect(help).toContain('FLAGS');
    expect(help).toContain('--query string');
    expect(help).toContain('Search text (required)');
    expect(help).toContain('--limit int');
    expect(help).toContain('--labels strings');
    expect(help).toContain('--counts ints');
    expect(help).toContain('--scores numbers');
    expect(help).toContain('--bits bools');
    expect(help).toContain('--state string');
    expect(help).toContain('{open|closed}');
    expect(help).toContain('--force');
    expect(help).toContain('Force (default false)');
    expect(help).toContain('--payload json');
    expect(help).toContain('id string');
    expect(help).toContain('(required)');
    expect(help).toContain('EXAMPLES');
    expect(help).toContain('--counts 1');
    expect(help).toContain('--scores 1.5');
    expect(help).toContain('--bits true');
    expect(help).toContain(`--payload '{"id":"value"}'`);
    expect(help).not.toContain(instructions);
    expect(help).not.toContain('\nNOTES\n');
    const empty = defineCommand({
      name: 'doctor',
      description: 'Check.',
      schema: z.object({}),
      outputSchema: { type: 'object', properties: {} },
      run: () => 'ok',
    });
    const emptyHelp = commandHelp(spec, empty);
    expect(emptyHelp).toContain('FLAGS');
    expect(emptyHelp).toContain('(none)');
    expect(emptyHelp).toContain('OUTPUT');
    const weather = defineCommand({
      name: 'get_weather',
      title: 'Weather Information Provider',
      description: 'Get current weather information for a location',
      inputSchema: { type: 'object', properties: {}, additionalProperties: false },
      outputSchema: { type: 'string' },
      annotations: { readOnlyHint: true, openWorldHint: true },
      run: () => 'ok',
    });
    const weatherHelp = commandHelp(spec, weather);
    expect(weatherHelp.startsWith('Get current weather information for a location')).toBe(true);
    expect(weatherHelp).toContain('Weather Information Provider');
    expect(weatherHelp).toContain('OUTPUT\n      json');
    expect(weatherHelp).toContain('HINTS\n  read-only\n  open-world');
    expect(defaultOutput(defineCli({ name: 'issues', instructions, commands: [weather] }))).toContain(
      'get_weather:  Weather Information Provider — Get current weather information for a location',
    );
    const long = defineCommand({
      name: 'local',
      title: 'Local search',
      description: 'Find text in a file. Then read the hit.',
      schema: z.object({}),
      run: () => 'ok',
    });
    const longSpec = defineCli({ name: 'issues', instructions, commands: [long] });
    expect(defaultOutput(longSpec)).toContain('local:  Local search — Find text in a file.');
    expect(defaultOutput(longSpec)).not.toContain('Then read the hit.');
    expect(commandHelp(longSpec, long).startsWith('Find text in a file. Then read the hit.')).toBe(true);
    const nestedOut = defineCommand({
      name: 'place',
      description: 'Place',
      inputSchema: { type: 'object', properties: {} },
      outputSchema: {
        type: 'object',
        properties: {
          place: { type: 'object', required: ['id'], properties: { id: { type: 'string' } } },
        },
      },
      run: () => 'ok',
    });
    const placeHelp = commandHelp(spec, nestedOut);
    expect(placeHelp).toContain('place json');
    expect(placeHelp).toContain('id string');
    expect(placeHelp).toContain('(required)');
    expect(commandHelpJson(spec, weather)).toMatchObject({
      name: 'get_weather',
      title: 'Weather Information Provider',
      inputSchema: weather.inputSchema,
      outputSchema: { type: 'string' },
      annotations: { readOnlyHint: true, openWorldHint: true },
    });
  });

  it('quotes spaced enum values and builds a valid JSON example', () => {
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

    const text = defineCommand({
      name: 'raw',
      description: 'Raw text.',
      inputSchema: { type: 'string' },
      run: () => 'ok',
    });
    expect(commandHelp(spec, text)).toContain(`--input '"value"'`);
  });

  it('returns the same flags from --help --json', () => {
    const json = commandHelpJson(spec, command);
    expect(json.description).toBe(description);
    expect(json.flags).toBe(command.flags);
    expect(json.usage).toContain('--query <string>');
    expect(json.schema).toEqual(command.flags.length > 0 ? json.schema : {});
    expect(stripAnsi('\u001b[31mred\u001b[0m')).toBe('red');
  });

  it('strips ANSI from rendered help and keeps it in the JSON description', () => {
    const colored = defineCommand({
      name: 'paint',
      description: '\u001b[31mPaint\u001b[0m',
      schema: z.object({}),
      run: () => 'ok',
    });
    const coloredSpec = defineCli({
      name: 'issues',
      instructions: '\u001b[32mUse the index\u001b[0m',
      commands: [colored],
    });
    expect(defaultOutput(coloredSpec)).toContain('Use the index');
    expect(defaultOutput(coloredSpec)).not.toContain('\u001b');
    expect(rootHelp(coloredSpec)).not.toContain('\u001b');
    expect(commandHelp(coloredSpec, colored).startsWith('Paint')).toBe(true);
    expect(commandHelp(coloredSpec, colored)).not.toContain('\u001b');
    expect(commandHelpJson(coloredSpec, colored).description).toBe('\u001b[31mPaint\u001b[0m');
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

});
