import { parseArgs, hasHelpFlag, hasVersionFlag } from './parser.js';
import { EXIT } from './exit-codes.js';
import type { CLICommand, CLICommandSpec, ParsedArgs } from './types.js';

declare const __APP_VERSION__: string;

async function loadCommandsModule(): Promise<{
  loadCommand(name: string): Promise<CLICommand | undefined>;
}> {
  return import('./commands/index.js');
}

async function loadStaticCommandHelpModule(): Promise<{
  findStaticCommandHelp(name: string): CLICommandSpec | undefined;
}> {
  return import('./command-help-specs.js');
}

async function loadToolCommandModule(): Promise<{
  executeToolCommand(args: ParsedArgs): Promise<boolean>;
  printToolsContext(options?: { full?: boolean }): Promise<void>;
  showToolHelp(toolName: string): Promise<boolean>;
  showAvailableTools(): Promise<void>;
  showMultipleToolSchemas(toolNames: string[]): Promise<void>;
}> {
  return import('./tool-command.js');
}

async function loadLightToolHelpModule(): Promise<{
  printLightInstructions(options?: { full?: boolean }): void;
  printToolRuntimeUnavailable(): void;
  showLightAvailableTools(): void;
  showLightToolHelp(toolName: string): boolean;
}> {
  return import('./light-tool-help.js');
}

async function tryLoadToolCommandModule(): Promise<Awaited<
  ReturnType<typeof loadToolCommandModule>
> | null> {
  try {
    return await loadToolCommandModule();
  } catch {
    return null;
  }
}

async function loadMainHelpModule(): Promise<{
  showHelp(): void;
}> {
  return import('./main-help.js');
}

async function loadHelpModule(): Promise<{
  showCommandHelp(command: CLICommandSpec): void;
}> {
  return import('./help.js');
}

const KNOWN_TOP_LEVEL_OPTIONS = new Set([
  'no-color',
  'help',
  'version',
  'context',
]);

function showVersion(): void {
  const version =
    typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : 'unknown';
  console.log(`octocode v${version}`);
}

export async function runCLI(argv?: string[]): Promise<boolean> {
  const args = parseArgs(argv);

  if (args.options['no-color'] === true) {
    process.env.NO_COLOR = '1';
  }

  if (hasHelpFlag(args)) {
    if (args.command === 'tools') {
      if (typeof args.args[0] === 'string') {
        const toolModule = await tryLoadToolCommandModule();
        if (toolModule && (await toolModule.showToolHelp(args.args[0]))) {
          return true;
        }
        const { showLightToolHelp } = await loadLightToolHelpModule();
        if (showLightToolHelp(args.args[0])) return true;
      }
      const toolModule = await tryLoadToolCommandModule();
      if (toolModule) {
        await toolModule.showAvailableTools();
        return true;
      }
      const { showLightAvailableTools } = await loadLightToolHelpModule();
      showLightAvailableTools();
      return true;
    }

    if (args.command) {
      const [{ findStaticCommandHelp }, { showCommandHelp }] =
        await Promise.all([loadStaticCommandHelpModule(), loadHelpModule()]);

      const staticCommand = findStaticCommandHelp(args.command);
      if (staticCommand) {
        showCommandHelp(staticCommand);
        return true;
      }

      const { showHelp } = await loadMainHelpModule();
      showHelp();
      return true;
    }

    const { showHelp } = await loadMainHelpModule();
    showHelp();
    return true;
  }

  if (hasVersionFlag(args)) {
    showVersion();
    return true;
  }

  if (!args.command && args.options.context === true) {
    const toolModule = await tryLoadToolCommandModule();
    if (toolModule) {
      await toolModule.printToolsContext({
        full: args.options['full'] === true,
      });
      return true;
    }
    const { printLightInstructions } = await loadLightToolHelpModule();
    printLightInstructions({ full: args.options['full'] === true });
    return true;
  }

  if (!args.command) {
    const unknownOption = Object.keys(args.options).find(
      option => !KNOWN_TOP_LEVEL_OPTIONS.has(option)
    );
    if (unknownOption) {
      const suggestion =
        unknownOption === 'contecxt' ? ' (did you mean --context?)' : '';
      console.log();
      console.log(`  Unknown option: --${unknownOption}${suggestion}`);
      console.log(`  Run 'octocode --help' to see available commands.`);
      console.log();
      process.exitCode = EXIT.NOT_FOUND;
      return true;
    }
    return false;
  }

  if (args.command === 'tools') {
    const toolModule = await tryLoadToolCommandModule();
    if (!toolModule) {
      const {
        printToolRuntimeUnavailable,
        showLightAvailableTools,
        showLightToolHelp,
      } = await loadLightToolHelpModule();
      if (!args.args[0] || args.args[0] === 'list' || args.options.list) {
        showLightAvailableTools();
        return true;
      }
      if (!args.options.queries && showLightToolHelp(args.args[0])) {
        return true;
      }
      printToolRuntimeUnavailable();
      process.exitCode = EXIT.TOOL;
      return true;
    }

    const success = await toolModule.executeToolCommand(args);
    if (!success && !process.exitCode) {
      process.exitCode = EXIT.GENERAL;
    }
    return true;
  }

  if (args.command === 'context') {
    const toolModule = await tryLoadToolCommandModule();
    if (toolModule) {
      await toolModule.printToolsContext({
        full: args.options['full'] === true,
      });
      return true;
    }
    const { printLightInstructions } = await loadLightToolHelpModule();
    printLightInstructions({ full: args.options['full'] === true });
    return true;
  }

  const { loadCommand } = await loadCommandsModule();
  const command = await loadCommand(args.command);

  if (!command) {
    console.log();
    console.log(`  Unknown command: ${args.command}`);
    console.log(`  Run 'octocode --help' to see available commands.`);
    console.log();
    process.exitCode = EXIT.NOT_FOUND;
    return true;
  }

  await command.handler(args);
  return true;
}
