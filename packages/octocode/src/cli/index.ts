import { setRuntimeSurface } from '@octocodeai/config';
import {
  delegateToNative,
  resolveNativeBin,
  shouldDelegateToNative,
} from './native-delegate.js';
import { hasHelpFlag, hasVersionFlag, parseArgs } from './parser.js';
import { EXIT } from './exit-codes.js';

/**
 * The npm CLI is a launcher, not a second implementation. The native Rust
 * binary owns every command — parsing, help, version, validation, execution.
 * Node retains exactly three responsibilities:
 *  - `scheme`: joins core-owned presentation with the native machine catalog
 *    after a fail-closed fingerprint check,
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
  if (args.options['no-color']) process.env.NO_COLOR = '1';

  if (!shouldDelegateToNative(args.command)) {
    if (args.command === 'scheme') {
      const { schemeCommand } = await import('./commands/scheme.js');
      await schemeCommand.handler(args);
      return true;
    }
    const { skillCommand } = await import('./commands/skill.js');
    await skillCommand.handler(args);
    return true;
  }

  const bin = resolveNativeBin();
  if (!bin) {
    // Runtime-unavailable is an execution failure (exit 5), matching the native
    // exit-code table and the `scheme` path; do not throw into main().catch,
    // which would report a generic exit 1 for the same condition.
    process.stderr.write(
      'The native Octocode runtime is unavailable for this platform or installation.\n'
    );
    process.exitCode = EXIT.TOOL;
    return false;
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
