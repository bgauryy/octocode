import { setRuntimeSurface } from '@octocodeai/config';
import {
  findUnknownOptions,
  printUnknownOptionError,
} from './command-validation.js';
import { loadCommand } from './commands/index.js';
import { findCommandSpec } from './commands/specs.js';
import { EXIT } from './exit-codes.js';
import { showCommandHelp } from './help.js';
import {
  delegateToNative,
  resolveNativeBin,
  shouldDelegateToNative,
} from './native-delegate.js';
import { hasHelpFlag, hasVersionFlag, parseArgs } from './parser.js';

declare const __APP_VERSION__: string;

function showVersion(): void {
  const version =
    typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : 'unknown';
  console.log(`octocode v${version}`);
}

export async function runCLI(argv?: string[]): Promise<boolean> {
  const { maybeWarnAboutStaleBuild } = await import('./stale-build.js');
  maybeWarnAboutStaleBuild();
  setRuntimeSurface('cli');

  const rawArgv = argv ?? process.argv.slice(2);
  const args = parseArgs(rawArgv);
  if (args.options['no-color'] === true) process.env.NO_COLOR = '1';

  // Node owns only skill materialization and the TTY client picker for a bare
  // install command. Every operation after selection is native-owned.
  const hasExplicitIde = rawArgv.some(
    value => value === '--ide' || value.startsWith('--ide=')
  );
  const interactiveInstall =
    args.command === 'install' &&
    !hasExplicitIde &&
    !hasHelpFlag(args) &&
    !hasVersionFlag(args) &&
    args.options.list !== true &&
    args.options.json !== true &&
    process.stdin.isTTY === true &&
    process.stdout.isTTY === true;
  const nodeOwned = interactiveInstall || !shouldDelegateToNative(args.command);

  if (!nodeOwned || interactiveInstall) {
    const bin = resolveNativeBin();
    if (!bin) {
      throw new Error(
        'The native Octocode runtime is unavailable for this platform or installation.'
      );
    }
    if (interactiveInstall) {
      const { runInteractiveInstall } =
        await import('./interactive-install.js');
      process.exitCode = await runInteractiveInstall(bin, rawArgv);
    } else {
      process.exitCode = await delegateToNative(bin, rawArgv);
    }
    return true;
  }

  if (hasHelpFlag(args)) {
    const spec = args.command ? findCommandSpec(args.command) : undefined;
    if (spec) {
      showCommandHelp(spec);
      return true;
    }
  }

  if (hasVersionFlag(args)) {
    showVersion();
    return true;
  }

  const command = args.command ? await loadCommand(args.command) : undefined;
  if (!command) {
    process.exitCode = EXIT.NOT_FOUND;
    return true;
  }

  const unknownOptions = findUnknownOptions(command, args);
  if (unknownOptions.length > 0) {
    printUnknownOptionError(command, unknownOptions);
    process.exitCode = EXIT.USAGE;
    return true;
  }

  await command.handler(args);
  return true;
}
