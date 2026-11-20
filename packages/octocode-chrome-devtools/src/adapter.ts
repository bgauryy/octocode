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
  chromeCommands,
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
 * - destructiveHint: it can change page or browser state, delete data or send requests. `flow` is
 *   destructive because its `run` action acts; `open` launches or reuses a browser: neither
 *   read-only nor destructive.
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
  'snapshot-query': { kind: 'readOnly', openWorld: false },
  'measure-query': { kind: 'readOnly', openWorld: false },
  'har-pager': { kind: 'readOnly', openWorld: false },
  'corpus-query': { kind: 'readOnly', openWorld: true },
  run: { kind: 'destructive', openWorld: true },
  cdp: { kind: 'destructive', openWorld: true },
  script: { kind: 'destructive', openWorld: true },
  check: { kind: 'destructive', openWorld: true },
  cookies: { kind: 'destructive', openWorld: true },
  'api-replay': { kind: 'destructive', openWorld: true },
  prune: { kind: 'destructive', openWorld: true },
  flow: { kind: 'destructive', openWorld: true },
  cleanup: { kind: 'destructive', openWorld: true },
  open: { kind: 'other', openWorld: true },
  'dev-check': { kind: 'other', openWorld: true },
  proxy: { kind: 'other', openWorld: true },
  session: { kind: 'other', openWorld: true },
  'har-redact': { kind: 'other', openWorld: true },
  'protocol-corpus': { kind: 'other', openWorld: true },
  'har-ingest': { kind: 'other', openWorld: true },
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
        const keys = Object.keys(result).join(', ');
        return cliView(`OK; data in structuredContent: ${keys}`, {
          structuredContent: result,
        });
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
const defaultPreset = 'research';
export const presets = {
  slim: slimSpec,
  research: researchSpec,
  full: spec,
};

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
  const preset =
    argv.length === 3 && argv[1] === '--preset' ? argv[2] : undefined;
  if (argv.length !== 1 && !(preset && Object.hasOwn(presets, preset)))
    throw Error(
      `Usage: --mcp [--preset ${Object.keys(presets).join('|')}] (default ${defaultPreset}); start from the workspace cwd`
    );
  const selected = presets[(preset ?? defaultPreset) as keyof typeof presets];
  const server = new McpServer(
    { name: spec.name, version: spec.version },
    mcpServerOptions(selected)
  );
  registerOn(server, selected);
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
