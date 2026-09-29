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
 * binary owns command parsing, command help, validation, and execution.
 * Node retains presentation and management responsibilities:
 *  - `scheme`: joins core-owned presentation with the native machine catalog
 *    after a fail-closed fingerprint check (also supplies root help instructions),
 *  - `skill`: bundled-skill materialization (the native `skill` command
 *    shells back to this CLI; delegating it would recurse), and
 *  - the TTY client picker for a bare `install` (selection only — every
 *    operation after selection runs native).
 */
/** Boolean flags arrive as `true` (bare) or a string (`--json=true`). */
export function isTrueFlag(value: unknown): boolean {
  if (value === true) return true;
  return (
    typeof value === 'string' &&
    ['true', '1', 'yes'].includes(value.toLowerCase())
  );
}

export async function runCLI(argv?: string[]): Promise<boolean> {
  const { maybeWarnAboutStaleBuild } = await import('./stale-build.js');
  maybeWarnAboutStaleBuild();
  setRuntimeSurface('cli');

  const rawArgv = argv ?? process.argv.slice(2);
  const args = parseArgs(rawArgv);
  if (args.options['no-color']) process.env.NO_COLOR = '1';
  if (args.command === 'help' && ['scheme', 'skill'].includes(args.args[0])) {
    args.command = args.args.shift() ?? null;
    args.options.help = true;
  }

  // A bare `octocode` (no command, no help/version flag) is the agent overview:
  // the same catalog `scheme` emits — short tool descriptions, availability, the
  // `scheme <name>` route to a tool's params, and the canonical instructions.
  // Root help appends the same instructions to the native command reference;
  // a bare `--version` prints launcher and native versions (see version.ts).
  if (args.command === null && !hasHelpFlag(args) && !hasVersionFlag(args)) {
    const { schemeCommand } = await import('./commands/scheme.js');
    await schemeCommand.handler({ ...args, command: 'scheme', args: [] });
    return true;
  }

  if (args.command === null && hasVersionFlag(args) && !hasHelpFlag(args)) {
    const { versionLine } = await import('./version.js');
    const line = versionLine();
    if (line) {
      process.stdout.write(`${line}\n`);
      return true;
    }
  }

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
    !isTrueFlag(args.options.list) &&
    !isTrueFlag(args.options.json) &&
    process.stdin.isTTY === true &&
    process.stdout.isTTY === true;

  if (interactiveInstall) {
    const { runInteractiveInstall } = await import('./interactive-install.js');
    process.exitCode = await runInteractiveInstall(bin, rawArgv);
  } else {
    process.exitCode = await delegateToNative(bin, rawArgv);
    const rootHelp =
      (args.command === null && hasHelpFlag(args)) ||
      (args.command === 'help' && args.args.length === 0);
    if (process.exitCode === EXIT.OK && rootHelp) {
      const { printAgentInstructions } = await import('./commands/scheme.js');
      // Help must never fail: drift/unavailable-catalog diagnostics are
      // already written to stderr by the presenter, so the exit code stays OK.
      await printAgentInstructions();
    }
  }
  return true;
}
