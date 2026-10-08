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
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import {
  chromeCommands as commands,
  chromeInstructions,
  chromeResearchCommands,
} from '@octocodeai/config/schema';
import { schemaFor, type Invocation } from './command-inputs.js';
import { invoke, stopEngine } from './invocation.js';
export { connectStdio, cliFromMcp, runCli } from 'octocode-mcp-cli';
export const spec = defineCli({
  name: 'octocode-chrome-devtools',
  version: '0.1.0',
  instructions: chromeInstructions,
  commands: Object.entries(commands).map(([name, description]) =>
    defineCommand({
      name,
      description,
      schema: schemaFor(name),
      annotations: {
        readOnlyHint: [
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
        ].includes(name),
        destructiveHint: [
          'cleanup',
          'prune',
          'cookies',
          'api-replay',
          'run',
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
        const result = await invoke(name, input as Invocation, context?.signal);
        return cliView(
          result.ok ? 'OK' : 'Failed; inspect capture before retrying.',
          { structuredContent: result }
        );
      },
    })
  ),
});

export const researchSpec = {
  ...spec,
  commands: spec.commands.filter(command =>
    (chromeResearchCommands as readonly string[]).includes(command.name)
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
      if (!command) throw Error('Unknown command ' + name);
      const input = JSON.parse(readFileSync(path === '-' ? 0 : path, 'utf8'));
      command.schema!.parse(input);
      argv = [
        name,
        ...command.flags
          .filter(flag => Object.hasOwn(input, flag.property))
          .flatMap(flag => [
            '--' + flag.name,
            typeof input[flag.property] === 'object'
              ? JSON.stringify(input[flag.property])
              : String(input[flag.property]),
          ]),
        ...(argv.includes('--json') ? ['--json'] : []),
      ];
    }
    const compact = argv.includes('--json');
    return runCli(
      { ...spec, name: spec.name + ' /cli' },
      argv,
      compact
        ? {
            stdout: text =>
              process.stdout.write(
                JSON.stringify(
                  JSON.parse(text, (_key, value, context) =>
                    typeof value === 'number' && context?.source
                      ? JSON.rawJSON(context.source)
                      : value
                  )
                ) + '\n'
              ),
            stderr: text => process.stderr.write(text),
          }
        : undefined
    );
  }
  const selected =
    argv.length === 3 && argv[1] === '--preset' && argv[2] === 'research'
      ? researchSpec
      : spec;
  if (argv.length !== 1 && selected !== researchSpec)
    throw Error(
      'Usage: --mcp [--preset research]; start from the workspace cwd'
    );
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
