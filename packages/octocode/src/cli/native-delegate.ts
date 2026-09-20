// Thin Node-to-Rust delegation boundary. Public tools and flag-only management
// commands execute in the compiled native CLI; Node retains only interactive
// installation and skill materialization. This CLI is the single launcher: it
// resolves and spawns the platform native binary directly (no intermediate
// `octocode.cjs` shim process), so a tool call costs one Node hop, not two.
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { existsSync } from 'node:fs';
import { constants as osConstants } from 'node:os';

interface NativeBinaryResolver {
  resolveNativeBinaryPath: () => string;
}

const FORWARDED_SIGNALS: NodeJS.Signals[] = ['SIGINT', 'SIGTERM', 'SIGHUP'];

/**
 * `skill` remains in Node because the native command intentionally invokes this
 * launcher for shared skill materialization. Delegating it would recurse.
 */
export const NODE_OWNED_COMMANDS: ReadonlySet<string> = new Set(['skill']);

/**
 * Resolve the compiled native `octocode` binary, or null when unavailable. An
 * explicit `OCTOCODE_NATIVE_BIN` path wins; otherwise the platform binary is
 * resolved directly through `@octocodeai/octocode-native`'s shared resolver — so
 * delegation spawns the native binary itself, not a second Node launcher. Never
 * throws.
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
    const { resolveNativeBinaryPath } =
      require('@octocodeai/octocode-native/bin/resolve-binary.cjs') as NativeBinaryResolver;
    return resolveNativeBinaryPath();
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
 * Spawn the native binary with the given argv, passing stdio through, and
 * resolve with the child exit code (0 clean, 1 on spawn failure, 128+N on a
 * signal death).
 *
 * Uses async `spawn` (not `spawnSync`) so a termination signal directed only at
 * this Node process — e.g. `SIGTERM` from systemd/docker, which does not hit the
 * whole process group — is forwarded to the native child instead of queuing
 * behind a blocking wait. While the child owns stdio this function is the sole
 * signal owner: it removes the parent's SIGINT/SIGTERM/SIGHUP handlers (so their
 * "Goodbye"/forced-exit cannot race the child's own interrupt handling and drain)
 * and restores them once the child exits.
 */
export function delegateToNative(
  bin: string,
  argv: readonly string[],
  env: NodeJS.ProcessEnv = process.env
): Promise<number> {
  const isLauncher = bin.endsWith('.cjs') || bin.endsWith('.js');
  const child = isLauncher
    ? spawn(process.execPath, [bin, ...argv], { stdio: 'inherit', env })
    : spawn(bin, [...argv], { stdio: 'inherit', env });

  const saved = new Map<NodeJS.Signals, NodeJS.SignalsListener[]>();
  const forwarders = new Map<NodeJS.Signals, () => void>();
  for (const signal of FORWARDED_SIGNALS) {
    saved.set(signal, process.listeners(signal) as NodeJS.SignalsListener[]);
    process.removeAllListeners(signal);
    const forward = (): void => {
      if (child.exitCode === null && child.signalCode === null) {
        child.kill(signal);
      }
    };
    forwarders.set(signal, forward);
    process.on(signal, forward);
  }
  const restoreSignals = (): void => {
    for (const signal of FORWARDED_SIGNALS) {
      const forward = forwarders.get(signal);
      if (forward) {
        process.removeListener(signal, forward);
      }
      for (const listener of saved.get(signal) ?? []) {
        process.on(signal, listener);
      }
    }
  };

  return new Promise<number>(resolve => {
    child.once('error', () => {
      restoreSignals();
      resolve(1);
    });
    child.once('close', (code, signal) => {
      restoreSignals();
      if (code !== null) {
        resolve(code);
        return;
      }
      // Signal death: report 128+N so OOM (SIGKILL→137) and crashes
      // (SIGSEGV→139) stay distinguishable from an ordinary error exit.
      const signalNumber = signal ? osConstants.signals[signal] : undefined;
      resolve(typeof signalNumber === 'number' ? 128 + signalNumber : 1);
    });
  });
}
