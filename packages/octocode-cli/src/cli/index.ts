import { parseArgs, hasHelpFlag, hasVersionFlag } from './parser.js';
import type { CLICommand, CLICommandSpec, ParsedArgs } from './types.js';

declare const __APP_VERSION__: string;

async function loadCommandsModule(): Promise<{
  findCommand(name: string): CLICommand | undefined;
}> {
  return import('./commands.js');
}

async function loadStaticCommandHelpModule(): Promise<{
  findStaticCommandHelp(name: string): CLICommandSpec | undefined;
}> {
  return import('./command-help-specs.js');
}

async function loadToolCommandModule(): Promise<{
  executeToolCommand(args: ParsedArgs): Promise<boolean>;
  printToolsContext(): Promise<void>;
  showToolHelp(toolName: string): Promise<boolean>;
  showAvailableTools(): Promise<void>;
  showMultipleToolSchemas(toolNames: string[]): Promise<void>;
}> {
  return import('./tool-command.js');
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

function printLegacyToolCommandError(): void {
  console.log();
  console.log(
    "  Use octocode --tool <toolName> --queries '<json-stringified-input>'."
  );
  console.log(
    '  Example: octocode --tool localSearchCode --queries \'{"path":".","pattern":"runCLI"}\''
  );
  console.log();
}

function showVersion(): void {
  const version =
    typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : 'unknown';
  console.log(`octocode v${version}`);
}

export async function runCLI(argv?: string[]): Promise<boolean> {
  const args = parseArgs(argv);

  if (args.options['tools-context'] === true) {
    const { printToolsContext } = await loadToolCommandModule();
    await printToolsContext();
    return true;
  }

  if (hasHelpFlag(args)) {
    if (
      args.command === 'tool' &&
      typeof args.options.tool === 'string' &&
      typeof args.args[0] === 'string'
    ) {
      const { showToolHelp } = await loadToolCommandModule();
      if (await showToolHelp(args.args[0])) {
        return true;
      }
    }

    if (args.command === 'tool') {
      const { showHelp } = await loadMainHelpModule();
      showHelp();
      return true;
    }

    if (args.command === 'tools') {
      if (typeof args.args[0] === 'string') {
        const { showToolHelp } = await loadToolCommandModule();
        if (await showToolHelp(args.args[0])) {
          return true;
        }
      }
      const { showAvailableTools } = await loadToolCommandModule();
      await showAvailableTools();
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

      const [{ findCommand }, { showHelp }] = await Promise.all([
        loadCommandsModule(),
        loadMainHelpModule(),
      ]);
      const cmd = findCommand(args.command);
      if (cmd) {
        showCommandHelp(cmd);
        return true;
      }
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

  if (!args.command) {
    return false;
  }

  if (args.command === 'tool') {
    if (typeof args.options.tool !== 'string') {
      printLegacyToolCommandError();
      process.exitCode = 1;
      return true;
    }

    const success = await (
      await loadToolCommandModule()
    ).executeToolCommand(args);
    if (!success) {
      process.exitCode = 1;
    }
    return true;
  }

  if (args.command === 'tools') {
    const { executeToolCommand } = await loadToolCommandModule();
    const success = await executeToolCommand(args);
    if (!success) {
      process.exitCode = 1;
    }
    return true;
  }

  if (args.command === 'instructions') {
    const { printToolsContext } = await loadToolCommandModule();
    await printToolsContext();
    return true;
  }

  const { findCommand } = await loadCommandsModule();
  const command = findCommand(args.command);

  if (!command) {
    console.log();
    console.log(`  Unknown command: ${args.command}`);
    console.log(`  Run 'octocode --help' to see available commands.`);
    console.log();
    process.exitCode = 1;
    return true;
  }

  await command.handler(args);
  return true;
}
