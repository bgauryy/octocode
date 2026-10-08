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
 *  - `schema`: joins core-owned presentation with the native machine catalog
 *    after a fail-closed fingerprint check,
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

  const rawArgv = argv ?? process.argv.slice(2);
  const args = parseArgs(rawArgv);
  if (args.command === 'help' && args.args[0] === 'skill') {
    args.command = args.args.shift() ?? null;
    args.options.help = true;
  }

  // A bare `octocode` is the overview for whoever runs it: the command
  // reference on a terminal, the tool catalog with agent instructions (the
  // `schema` listing) on a pipe.
  if (args.command === null && !hasHelpFlag(args) && !hasVersionFlag(args)) {
    if (process.stdout.isTTY !== true) {
      const { schemaCommand } = await import('./commands/schema.js');
      await schemaCommand.handler({ ...args, command: 'schema', args: [] });
      return true;
    }
    rawArgv.push('--help');
  }

  if (args.command === null && hasVersionFlag(args) && !hasHelpFlag(args)) {
    const { versionLine } = await import('./version.js');
    const line = versionLine();
    if (line) {
      process.stdout.write(`${line}\n`);
      return true;
    }
  }

  // The binary owns command help, `schema --help` included.
  const nodeOwned =
    !shouldDelegateToNative(args.command) &&
    !(args.command === 'schema' && hasHelpFlag(args));
  if (nodeOwned) {
    if (args.command === 'schema') {
      const { schemaCommand } = await import('./commands/schema.js');
      await schemaCommand.handler(args);
      return true;
    }
    const { skillCommand } = await import('./commands/skill.js');
    await skillCommand.handler(args);
    return true;
  }

  const bin = resolveNativeBin();
  if (!bin) {
    // Runtime-unavailable is an execution failure (exit 5), matching the native
    // exit-code table and the `schema` path; do not throw into main().catch,
    // which would report a generic exit 1 for the same condition.
    process.stderr.write(
      'The native Octocode runtime is unavailable for this platform or installation.\n'
    );
    process.exitCode = EXIT.TOOL;
    return false;
  }

  if (
    args.command === 'config' &&
    rawArgv[rawArgv.indexOf('config') + 1] === 'view' &&
    !hasHelpFlag(args)
  ) {
    const { configViewCommand } = await import('./commands/config-view.js');
    process.exitCode = await configViewCommand(bin, rawArgv);
    return true;
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
  }
  return true;
}
