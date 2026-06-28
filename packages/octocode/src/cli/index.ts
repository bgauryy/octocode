import { existsSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs, hasHelpFlag, hasVersionFlag } from './parser.js';
import { EXIT } from './exit-codes.js';
import type { CLICommand, CLICommandSpec, ParsedArgs } from './types.js';
import {
  setRuntimeSurface,
  invalidateConfigCache,
} from '@octocodeai/octocode-tools-core/config';

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
  getToolsContextString(options?: { full?: boolean }): Promise<string>;
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
  'context',
]);

let staleBuildWarningShown = false;

function maybeWarnAboutStaleBuild(): void {
  if (staleBuildWarningShown || process.env.OCTOCODE_NO_STALE_BUILD_WARNING) {
    return;
  }
  staleBuildWarningShown = true;

  const currentFile = fileURLToPath(import.meta.url);
  if (!currentFile.includes(`${path.sep}out${path.sep}`)) return;

  const sourceFile = path.resolve(
    path.dirname(currentFile),
    '..',
    '..',
    'src',
    'cli',
    'index.ts'
  );
  if (!existsSync(sourceFile)) return;

  const builtMtime = statSync(currentFile).mtimeMs;
  const sourceMtime = statSync(sourceFile).mtimeMs;
  if (sourceMtime <= builtMtime + 1000) return;

  console.error(
    '  Warning: built CLI output looks older than src/cli/index.ts. Run `yarn build` before dogfooding source edits.'
  );
}

function showVersion(): void {
  const version =
    typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : 'unknown';
  console.log(`octocode v${version}`);
}

export async function runCLI(argv?: string[]): Promise<boolean> {
  maybeWarnAboutStaleBuild();

  // Declare the CLI surface before any config is read: local and clone support
  // default to enabled here, while still honoring explicit env/file disables.
  setRuntimeSurface('cli');
  invalidateConfigCache();

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

      const { loadCommand } = await loadCommandsModule();
      const liveCommand = await loadCommand(args.command);
      if (liveCommand) {
        console.log();
        console.log(
          `  Missing octocode-core command spec for: ${liveCommand.name}`
        );
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

  if (!args.command && args.options.context === true) {
    const toolModule = await tryLoadToolCommandModule();
    if (toolModule) {
      const options = { full: args.options['full'] === true };
      if (args.options['json'] === true) {
        const context = await toolModule.getToolsContextString(options);
        console.log(JSON.stringify({ context }));
      } else {
        await toolModule.printToolsContext(options);
      }
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
      const options = { full: args.options['full'] === true };
      if (args.options['json'] === true) {
        const context = await toolModule.getToolsContextString(options);
        console.log(JSON.stringify({ context }));
      } else {
        await toolModule.printToolsContext(options);
      }
      return true;
    }
    const { printLightInstructions } = await loadLightToolHelpModule();
    printLightInstructions({ full: args.options['full'] === true });
    return true;
  }

  if (args.command === 'search' && args.options.scheme === true) {
    // Engine-free `/schema` subpath: print the schema without loading the
    // native engine. --compact prints the lean agent guide (TEXT);
    // --json --compact prints a small machine-readable guide.
    const { oqlSchemaText, oqlCompactSchemeText, oqlCompactSchemeJson } =
      await import('@octocodeai/octocode-tools-core/schema');
    const schemeText =
      args.options.json === true && args.options.compact === true
        ? oqlCompactSchemeJson()
        : args.options.compact === true
          ? oqlCompactSchemeText()
          : oqlSchemaText();
    process.stdout.write(`${schemeText}\n`);
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
