import { readFileSync } from 'node:fs';
import {
  defineCli,
  defineCommand,
  runCli,
  cliView,
  registerOn,
  mcpServerOptions,
} from 'octocode-mcp-cli';
import { McpServer } from '@modelcontextprotocol/server';
import { StdioServerTransport } from '@modelcontextprotocol/server/stdio';
import {
  chromeAdvertisedSchemaFor,
  chromeCommands,
  chromeFullCommands,
  chromeInstructionsFor,
  chromeResearchCommands,
  chromeSlimCommands,
  chromeSchemaFor,
} from '@octocodeai/config/chrome';
import manifest from '../package.json' with { type: 'json' };
import { invoke, stopEngine } from './invocation.js';
export { connectStdio, cliFromMcp, runCli } from 'octocode-mcp-cli';
export { call, chrome, closeChrome } from './functions.js';
export type { ChromeCommandName, CallOptions } from './functions.js';
/**
 * Annotation rule table, one row per tool.
 * - readOnlyHint: the tool changes no browser, page, profile or system state beyond saving new
 *   capture evidence under .octocode/; every readOnly tool is also idempotent and not destructive.
 * - destructiveHint: it can change page or browser state, delete data or send requests. A tool is
 *   classified by its most powerful action, not per action: `flow` is destructive because its `run`
 *   and `save` actions act (its `list` and `show` only read, said in its description); `group` opens
 *   and closes tabs; `helper` reaches prune, api-replay and har-redact; `open` launches or reuses a browser: neither read-only nor destructive.
 * - openWorldHint: it reaches Chrome, a network or other systems rather than only saved files.
 */
export const annotationRules = {
  targets: { kind: 'readOnly', openWorld: true },
  snapshot: { kind: 'readOnly', openWorld: true },
  screenshot: { kind: 'readOnly', openWorld: true },
  protocol: { kind: 'readOnly', openWorld: true },
  schema: { kind: 'readOnly', openWorld: false },
  skill: { kind: 'readOnly', openWorld: false },
  query: { kind: 'readOnly', openWorld: false },
  artifact: { kind: 'readOnly', openWorld: false },
  run: { kind: 'destructive', openWorld: true },
  cdp: { kind: 'destructive', openWorld: true },
  script: { kind: 'destructive', openWorld: true },
  check: { kind: 'destructive', openWorld: true },
  cookies: { kind: 'destructive', openWorld: true },
  helper: { kind: 'destructive', openWorld: true },
  group: { kind: 'destructive', openWorld: true },
  flow: { kind: 'destructive', openWorld: true },
  cleanup: { kind: 'destructive', openWorld: true },
  open: { kind: 'other', openWorld: true },
  'dev-check': { kind: 'other', openWorld: true },
  proxy: { kind: 'other', openWorld: true },
  session: { kind: 'other', openWorld: true },
} as const satisfies Record<
  keyof typeof chromeCommands,
  { kind: 'readOnly' | 'destructive' | 'other'; openWorld: boolean }
>;
const annotationsFor = (name: keyof typeof annotationRules) => {
  const { kind, openWorld } = annotationRules[name];
  return {
    readOnlyHint: kind === 'readOnly',
    idempotentHint: kind === 'readOnly',
    destructiveHint: kind === 'destructive',
    openWorldHint: openWorld,
  };
};

export const spec = defineCli({
  name: 'octocode-chrome-devtools',
  version: manifest.version,
  instructions: chromeInstructionsFor(Object.keys(chromeCommands)),
  commands: Object.entries(chromeCommands).map(([name, description]) => {
    return defineCommand({
      name,
      description,
      schema: chromeSchemaFor(name),
      annotations: annotationsFor(name as keyof typeof annotationRules),
      run: async (input, context) => {
        const result = await invoke(
          name,
          chromeSchemaFor(name).parse(input),
          context?.signal
        );
        // invoke() throws on failure (isError, text from failureSummary); only successes reach here.
        // The compact reply is the text and the structured content: a text-only client sees it all.
        return cliView(JSON.stringify(result), { structuredContent: result });
      },
    });
  }),
});

// Each preset is a subset of the next: slim < research (the default) < full. A name
// list owns membership; each preset's instructions name only the tools it holds.
const subset = (names: readonly string[]) => ({
  ...spec,
  instructions: chromeInstructionsFor(names),
  commands: spec.commands.filter(command => names.includes(command.name)),
});
export const researchSpec = subset(chromeResearchCommands);
export const slimSpec = subset(chromeSlimCommands);
// The CLI registry keeps snapshot and screenshot; no MCP preset exports them (plan ops and the
// page-snapshot and page-screenshot recipes capture the same evidence).
export const fullSpec = subset(chromeFullCommands);
const defaultPreset = 'research';
export const presets = {
  slim: slimSpec,
  research: researchSpec,
  full: fullSpec,
};

/**
 * Opt-in tool groups: `--tools <group|tool>[,..]` adds them to any preset. One constant owns the
 * grouping; together the groups hold every tool the full preset has beyond research, each once.
 */
export const toolGroups = {
  data: ['cookies', 'session', 'script'],
  devtools: ['dev-check', 'helper', 'proxy'],
  capture: ['check'],
} as const satisfies Record<string, readonly (keyof typeof chromeCommands)[]>;
/** The preset's spec with the named groups and tools added; a tool the preset already holds is a no-op. Unknown names fail fast with the valid list. */
export const withTools = (
  presetName: keyof typeof presets,
  requested: readonly string[]
) => {
  const base = presets[presetName];
  const groups = toolGroups as Record<string, readonly string[]>;
  const added = new Set<string>();
  for (const name of requested) {
    if (Object.hasOwn(groups, name)) groups[name].forEach(t => added.add(t));
    else if (chromeFullCommands.includes(name)) added.add(name);
    else
      throw Error(
        `Unknown --tools name "${name}". Groups: ${Object.keys(groups).join(', ')}. Tools: ${chromeFullCommands.join(', ')}`
      );
  }
  const held = new Set(base.commands.map(command => command.name));
  if ([...added].every(name => held.has(name))) return base;
  return subset(
    spec.commands.map(c => c.name).filter(n => held.has(n) || added.has(n))
  );
};

type Registrar = Parameters<typeof registerOn>[0];
type JsonSchema = Record<string, unknown>;
const SAFE_INTEGER = Number.MAX_SAFE_INTEGER;
/** The advertised JSON Schema without what costs tokens and tells a client nothing: `$schema`, and the safe-integer bounds of every integer (the runtime still validates them). */
const trimJsonSchema = (node: unknown, root = true): unknown => {
  if (Array.isArray(node)) return node.map(item => trimJsonSchema(item, false));
  if (node === null || typeof node !== 'object') return node;
  return Object.fromEntries(
    Object.entries(node)
      .filter(
        ([key, value]) =>
          !(root && key === '$schema') &&
          !(
            (key === 'maximum' && value === SAFE_INTEGER) ||
            (key === 'minimum' && value === -SAFE_INTEGER)
          )
      )
      .map(([key, value]) => [key, trimJsonSchema(value, false)])
  );
};
type Standard = {
  validate: (value: unknown) => unknown;
  jsonSchema: { input: (options: unknown) => JsonSchema };
};
const standardOf = (schema: unknown) =>
  (schema as { '~standard': Standard })['~standard'];
/** Registers each tool with the schema it advertises (trimmed, and for `schema` limited to this preset's tools) while the command's own schema still validates every call. */
const advertising = (
  server: Registrar,
  names: readonly string[]
): Registrar => ({
  registerTool: (name, config, callback) => {
    const advertised = standardOf(chromeAdvertisedSchemaFor(name));
    const own = standardOf(config.inputSchema);
    const input = (options: unknown) => {
      const json = trimJsonSchema(
        advertised.jsonSchema.input(options)
      ) as JsonSchema;
      const command = (
        json.properties as Record<string, JsonSchema> | undefined
      )?.command;
      if (name === 'schema' && Array.isArray(command?.enum))
        command.enum = command.enum.filter(value =>
          names.includes(String(value))
        );
      return json;
    };
    return server.registerTool(
      name,
      {
        ...config,
        inputSchema: {
          '~standard': { ...own, jsonSchema: { input, output: input } },
        },
      },
      callback
    );
  },
});

export async function main(argv = process.argv.slice(2)) {
  if (argv[0] !== '--mcp') {
    // Whole-input files/stdin avoid OS argument-size limits for large typed plans.
    if (argv.includes('--input')) {
      const at = argv.indexOf('--input'),
        path = argv[at + 1],
        name = argv[0];
      if (at !== 1 || !path || argv.slice(3).some(value => value !== '--json'))
        throw Error(
          'Usage: octocode-chrome-devtools /cli <command> --input <file|-> [--json]'
        );
      const command = spec.commands.find(command => command.name === name);
      if (!command) throw Error(`Unknown command ${name}`);
      const input = JSON.parse(readFileSync(path === '-' ? 0 : path, 'utf8'));
      command.schema!.parse(input);
      argv = [
        name,
        ...command.flags
          .filter(flag => Object.hasOwn(input, flag.property))
          .flatMap(flag => [
            `--${flag.name}`,
            typeof input[flag.property] === 'object'
              ? JSON.stringify(input[flag.property])
              : String(input[flag.property]),
          ]),
        ...(argv.includes('--json') ? ['--json'] : []),
      ];
    }
    const compact = argv.includes('--json');
    return runCli(
      { ...spec, name: `${spec.name} /cli` },
      argv,
      compact
        ? {
            stdout: text =>
              process.stdout.write(
                `${JSON.stringify(
                  JSON.parse(text, (_key, value, context) =>
                    typeof value === 'number' && context?.source
                      ? JSON.rawJSON(context.source)
                      : value
                  )
                )}\n`
              ),
            stderr: text => process.stderr.write(text),
          }
        : undefined
    );
  }
  const usage = `Usage: --mcp [--preset ${Object.keys(presets).join('|')}] [--tools <${Object.keys(toolGroups).join('|')}|tool>[,..]] (default preset ${defaultPreset}); start from the workspace cwd`;
  let preset: string | undefined;
  let tools: string[] = [];
  const flags = argv.slice(1);
  if (flags.length % 2) throw Error(usage);
  for (let at = 0; at < flags.length; at += 2) {
    const value = flags[at + 1];
    if (flags[at] === '--preset' && preset === undefined) preset = value;
    else if (flags[at] === '--tools' && !tools.length)
      tools = value.split(',').filter(Boolean);
    else throw Error(usage);
  }
  if (preset !== undefined && !Object.hasOwn(presets, preset))
    throw Error(usage);
  const selected = withTools(
    (preset ?? defaultPreset) as keyof typeof presets,
    tools
  );
  const server = new McpServer(
    { name: spec.name, version: spec.version },
    mcpServerOptions(selected)
  );
  registerOn(
    advertising(
      server,
      selected.commands.map(command => command.name)
    ),
    selected
  );
  const stop = async () => {
    stopEngine();
    await server.close();
  };
  process.once('SIGINT', stop);
  process.once('SIGTERM', stop);
  await server.connect(new StdioServerTransport());
  process.stdin.once('end', stop);
  return 0;
}
