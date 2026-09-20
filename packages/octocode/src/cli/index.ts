import { setRuntimeSurface } from '@octocodeai/config';
import {
  delegateToNative,
  resolveNativeBin,
  shouldDelegateToNative,
} from './native-delegate.js';
import { hasHelpFlag, hasVersionFlag, parseArgs } from './parser.js';

/**
 * The npm CLI is a launcher, not a second implementation. The native Rust
 * binary owns every command — parsing, help, version, validation, execution.
 * Node retains exactly two responsibilities:
 *  - `skill`: bundled-skill materialization (the native `skill` command
 *    shells back to this CLI; delegating it would recurse), and
 *  - the TTY client picker for a bare `install` (selection only — every
 *    operation after selection runs native).
 */
export async function runCLI(argv?: string[]): Promise<boolean> {
  const { maybeWarnAboutStaleBuild } = await import('./stale-build.js');
  maybeWarnAboutStaleBuild();
  setRuntimeSurface('cli');

  const rawArgv = argv ?? process.argv.slice(2);
  const args = parseArgs(rawArgv);
  if (args.options['no-color'] === true) process.env.NO_COLOR = '1';

  if (!shouldDelegateToNative(args.command)) {
    const { skillCommand } = await import('./commands/skill.js');
    await skillCommand.handler(args);
    return true;
  }

  const bin = resolveNativeBin();
  if (!bin) {
    throw new Error(
      'The native Octocode runtime is unavailable for this platform or installation.'
    );
  }

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

  if (interactiveInstall) {
    const { runInteractiveInstall } = await import('./interactive-install.js');
    process.exitCode = await runInteractiveInstall(bin, rawArgv);
  } else {
    process.exitCode = await delegateToNative(bin, rawArgv);
  }
  return true;
}
