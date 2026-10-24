import { expect, it } from 'vitest';
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
  expect(help).not.toContain(instructions);
  expect(help).not.toContain('\nNOTES\n');
});

it('lists every flag with its type, enum, default and required marker', () => {
  const help = commandHelp(spec, command);
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
});

it('builds an example value for every array and object flag kind', () => {
  const help = commandHelp(spec, command);
  expect(help).toContain('EXAMPLES');
  expect(help).toContain('--counts 1');
  expect(help).toContain('--scores 1.5');
  expect(help).toContain('--bits true');
  expect(help).toContain(`--payload '{"id":"value"}'`);
});

it('shows "(none)" flags and the OUTPUT section for a command without inputs', () => {
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
});

it('shows the title beside the description and lists annotation hints', () => {
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
  expect(commandHelpJson(spec, weather)).toMatchObject({
    name: 'get_weather',
    title: 'Weather Information Provider',
    inputSchema: weather.inputSchema,
    outputSchema: { type: 'string' },
    annotations: { readOnlyHint: true, openWorldHint: true },
  });
});

it('indexes only the first sentence of a description and keeps the full text in command help', () => {
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
});

it('renders nested output object fields with their required marker', () => {
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
});

it('returns the same flags from --help --json', () => {
  const json = commandHelpJson(spec, command);
  expect(json.description).toBe(description);
  expect(json.flags).toBe(command.flags);
  expect(json.usage).toContain('--query <string>');
  expect(json.inputSchema).toBe(json.schema);
  expect(Object.keys(json.schema.properties ?? {}).toSorted()).toEqual(command.flags.map(flag => flag.property).toSorted());
});

it('strips ANSI from rendered help and keeps it in the JSON description', () => {
  expect(stripAnsi('\u001b[31mred\u001b[0m')).toBe('red');
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
