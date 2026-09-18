// Thin Node-to-Rust delegation boundary. Public tools and flag-only management
// commands execute in the compiled native CLI; Node retains only interactive
// installation and skill materialization.
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { existsSync } from 'node:fs';

/**
 * `skill` remains in Node because the native command intentionally invokes this
 * launcher for shared skill materialization. Delegating it would recurse.
 */
export const NODE_OWNED_COMMANDS: ReadonlySet<string> = new Set(['skill']);

/**
 * Resolve the native `octocode` binary (or its platform-selecting launcher),
 * or null when unavailable. An explicit `OCTOCODE_NATIVE_BIN` path wins;
 * otherwise the published `@octocodeai/octocode-native` launcher shim is
 * resolved if installed. Never throws.
 */
export function resolveNativeBin(
  env: NodeJS.ProcessEnv = process.env
): string | null {
  const explicit = env.OCTOCODE_NATIVE_BIN?.trim();
  if (explicit) {
    return existsSync(explicit) ? explicit : null;
  }
  try {
    const require = createRequire(import.meta.url);
    return require.resolve('@octocodeai/octocode-native/bin/octocode.cjs');
  } catch {
    return null;
  }
}

/**
 * Decide whether a parsed command is owned by the native binary. Availability
 * is checked separately so a missing installation fails closed instead of
 * silently running the retired TypeScript implementation.
 */
export function shouldDelegateToNative(
  command: string | null | undefined,
  _env: NodeJS.ProcessEnv = process.env
): boolean {
  if (!command) return true;
  if (NODE_OWNED_COMMANDS.has(command)) return false;
  return true;
}

/**
 * Spawn the native binary with the given argv, passing stdio through. Returns
 * the child exit code (0 when it exits cleanly, 1 on spawn failure).
 */
export function delegateToNative(
  bin: string,
  argv: readonly string[],
  env: NodeJS.ProcessEnv = process.env
): number {
  const isLauncher = bin.endsWith('.cjs') || bin.endsWith('.js');
  const result = isLauncher
    ? spawnSync(process.execPath, [bin, ...argv], { stdio: 'inherit', env })
    : spawnSync(bin, [...argv], { stdio: 'inherit', env });
  if (result.error) return 1;
  return result.status ?? 1;
}
