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
  chromeInstructions,
  chromeResearchCommands,
  chromeSchemaFor,
} from '@octocodeai/config/chrome';
import manifest from '../package.json' with { type: 'json' };
import { invoke, stopEngine } from './invocation.js';
export { connectStdio, cliFromMcp, runCli } from 'octocode-mcp-cli';
export { call, chrome, closeChrome } from './functions.js';
export type { ChromeCommandName, CallOptions } from './functions.js';
// Commands that only read saved or declared data; the rest drive Chrome or write state.
const READ_ONLY = new Set([
  'skill',
  'schema',
  'protocol',
  'targets',
  'artifact',
  'query',
  'snapshot-query',
  'measure-query',
  'har-pager',
  'corpus-query',
]);

export const spec = defineCli({
  name: 'octocode-chrome-devtools',
  version: manifest.version,
  instructions: chromeInstructions,
  commands: Object.entries(chromeCommands).map(([name, description]) => {
    const readOnly = READ_ONLY.has(name);
    return defineCommand({
      name,
      description,
      schema: chromeSchemaFor(name),
      annotations: {
        readOnlyHint: readOnly,
        idempotentHint: readOnly,
        destructiveHint: [
          'cleanup',
          'prune',
          'cookies',
          'api-replay',
          'run',
          'flow',
          'step',
          'cdp',
          'script',
          'check',
        ].includes(name),
        openWorldHint: ![
          'skill',
          'schema',
          'artifact',
          'query',
          'snapshot-query',
          'measure-query',
          'har-pager',
        ].includes(name),
      },
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

// The skill's first flow ends in a screenshot and the shared instructions name dev-check; flow replays named plans; the default surface must hold every tool they name.
const EXTRA_DEFAULT_COMMANDS = ['screenshot', 'dev-check', 'flow'];

export const researchSpec = {
  ...spec,
  commands: spec.commands.filter(command =>
    [...chromeResearchCommands, ...EXTRA_DEFAULT_COMMANDS].includes(
      command.name
    )
  ),
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
  const presets = { research: researchSpec, full: spec };
  const preset =
    argv.length === 3 && argv[1] === '--preset' ? argv[2] : undefined;
  if (argv.length !== 1 && !(preset && Object.hasOwn(presets, preset)))
    throw Error(
      `Usage: --mcp [--preset ${Object.keys(presets).join('|')}] (default research); start from the workspace cwd`
    );
  const selected = presets[(preset ?? 'research') as keyof typeof presets];
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
