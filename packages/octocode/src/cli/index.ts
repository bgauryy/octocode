import { parseArgs, hasHelpFlag, hasVersionFlag } from './parser.js';
import { EXIT } from './exit-codes.js';
import {
  shouldDelegateToNative,
  resolveNativeBin,
  delegateToNative,
} from './native-delegate.js';
import type { CLICommand, CLICommandSpec } from './types.js';
import { setRuntimeSurface } from '@octocodeai/config';

declare const __APP_VERSION__: string;

async function loadCommandsModule(): Promise<{
  loadCommand(name: string): Promise<CLICommand | undefined>;
  isRegisteredCommand(name: string): boolean;
}> {
  return import('./commands/index.js');
}

async function loadStaticCommandHelpModule(): Promise<{
  findStaticCommandHelp(name: string): CLICommandSpec | undefined;
}> {
  return import('./command-help-specs.js');
}

async function loadToolCommandModule(): Promise<{
  toolCommand: CLICommand;
  getToolsContextString(options?: {
    full?: boolean;
    minimal?: boolean;
  }): Promise<string>;
  printToolsContext(options?: {
    full?: boolean;
    minimal?: boolean;
  }): Promise<void>;
  showToolHelp(toolName: string): Promise<boolean>;
  showAvailableTools(): Promise<void>;
  showMultipleToolSchemas(toolNames: string[]): Promise<void>;
}> {
  const [command, context, help, list] = await Promise.all([
    import('./tool-command/command.js'),
    import('./tool-command/context.js'),
    import('./tool-command/help.js'),
    import('./tool-command/list-view.js'),
  ]);
  return {
    toolCommand: command.toolCommand,
    getToolsContextString: context.getToolsContextString,
    printToolsContext: context.printToolsContext,
    showToolHelp: help.showToolHelp,
    showAvailableTools: list.showAvailableTools,
    showMultipleToolSchemas: help.showMultipleToolSchemas,
  };
}

async function loadLightToolHelpModule(): Promise<{
  printLightInstructions(options?: { full?: boolean; minimal?: boolean }): void;
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
  showHelp(): Promise<void>;
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
  // Global output modifiers (help FLAGS line). With no command they are no-ops
  // that fall through to the main help (exit 0) rather than "unknown options".
  'json',
  'compact',
  'brief',
  'pretty',
  'minimal',
  'raw',
]);

function showVersion(): void {
  const version =
    typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : 'unknown';
  console.log(`octocode v${version}`);
}

export async function runCLI(argv?: string[]): Promise<boolean> {
  const { maybeWarnAboutStaleBuild } = await import('./stale-build.js');
  maybeWarnAboutStaleBuild();

  // Declare the CLI surface before any config is read: local and clone support
  // default to enabled here, while still honoring explicit env/file disables.
  setRuntimeSurface('cli');

  const args = parseArgs(argv);

  if (args.options['no-color'] === true) {
    process.env.NO_COLOR = '1';
  }

  // Opt-in: run the native Rust binary under the hood for the commands it
  // covers (everything except the TS-only management commands). Keeps
  // `npx octocode` as the interface; the TS path stays the default until
  // native ships via platform packages and parity is proven.
  if (shouldDelegateToNative(args.command)) {
    const bin = resolveNativeBin();
    if (bin) {
      process.exitCode = delegateToNative(bin, argv ?? process.argv.slice(2));
      return true;
    }
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
      const [{ isRegisteredCommand }, { findStaticCommandHelp }] =
        await Promise.all([
          loadCommandsModule(),
          loadStaticCommandHelpModule(),
        ]);
      const registered =
        isRegisteredCommand(args.command) || args.command === 'context';
      if (registered) {
        const helpModule = await loadHelpModule();
        const staticCommand = findStaticCommandHelp(args.command);
        if (staticCommand) {
          helpModule.showCommandHelp(staticCommand);
          return true;
        }
        console.log();
        console.log(`  Missing command help spec for: ${args.command}`);
        console.log();
        process.exitCode = EXIT.TOOL;
        return true;
      }

      console.log();
      console.log(`  Unknown command: ${args.command}`);
      console.log(`  Run '--help' to see available commands.`);
      console.log();
      process.exitCode = EXIT.NOT_FOUND;
      return true;
    }

    const { showHelp } = await loadMainHelpModule();
    await showHelp();
    return true;
  }

  if (hasVersionFlag(args)) {
    showVersion();
    return true;
  }

  if (!args.command) {
    const unknownOption = Object.keys(args.options).find(
      option => !KNOWN_TOP_LEVEL_OPTIONS.has(option)
    );
    if (unknownOption) {
      const { suggestFlag } = await import('./command-validation.js');
      const hint = suggestFlag(unknownOption, KNOWN_TOP_LEVEL_OPTIONS);
      const suggestion = hint ? ` (did you mean --${hint}?)` : '';
      console.log();
      console.log(`  Unknown option: --${unknownOption}${suggestion}`);
      console.log(`  Run '--help' to see available commands.`);
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
      if (!args.args[0] && args.options.list === undefined) {
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

    await toolModule.toolCommand.handler(args);
    return true;
  }

  if (args.command === 'context') {
    const toolModule = await tryLoadToolCommandModule();
    if (toolModule) {
      const options = {
        full: args.options['full'] === true,
        minimal: args.options['minimal'] === true,
      };
      if (args.options['json'] === true) {
        const context = await toolModule.getToolsContextString(options);
        console.log(JSON.stringify({ context }));
      } else {
        await toolModule.printToolsContext(options);
      }
      return true;
    }
    const { printLightInstructions } = await loadLightToolHelpModule();
    printLightInstructions({
      full: args.options['full'] === true,
      minimal: args.options['minimal'] === true,
    });
    return true;
  }

  const { loadCommand } = await loadCommandsModule();
  const command = await loadCommand(args.command);

  if (!command) {
    console.log();
    console.log(`  Unknown command: ${args.command}`);
    console.log(`  Run '--help' to see available commands.`);
    console.log();
    process.exitCode = EXIT.NOT_FOUND;
    return true;
  }

  const {
    findUnknownOptions,
    printUnknownOptionError,
    findInvalidNumericOptions,
  } = await import('./command-validation.js');
  const unknownOptions = findUnknownOptions(command, args);
  if (unknownOptions.length > 0) {
    printUnknownOptionError(command, unknownOptions);
    process.exitCode = EXIT.USAGE;
    return true;
  }

  const badNumeric = findInvalidNumericOptions(args);
  if (badNumeric.length > 0) {
    console.log();
    console.log(
      `  Invalid numeric value: ${badNumeric.join(', ')} — must be a whole number >= 0.`
    );
    console.log();
    process.exitCode = EXIT.USAGE;
    return true;
  }

  await command.handler(args);
  return true;
}
