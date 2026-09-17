// Native-delegation seam: run the compiled Rust `octocode` binary
// (packages/octocode-native, backed by octocode-engine) under the hood while
// keeping `npx octocode` as the entry point.
//
// The goal is to make this Node package a thin interface over the native
// runtime. Full removal of the TypeScript implementation is blocked only by a
// few management commands the native binary does not (yet) own, so those stay
// on the TS path — see TS_ONLY_COMMANDS. Everything else can be delegated.
//
// Delegation is currently OPT-IN (`OCTOCODE_RUNTIME=native`) with the TS path
// as the safe default, mirroring the octocode-mcp runtime selector. The default
// flips to native once the native binary is shipped through platform packages
// and parity is proven by a differential harness.
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { existsSync } from 'node:fs';

/**
 * Commands the native binary does NOT cover and that therefore stay on the
 * TypeScript implementation:
 *   - `lsp-server`: LSP toolchain provisioning/management (octocode-engine LSP
 *      manager + server manifest) — no native equivalent.
 *   - `install`: native now writes all formats natively (JSON, codex TOML, goose
 *      YAML), but the TS `install` command still owns interactive client
 *      detection/prompts that native's flag-only CLI lacks, so routing stays TS
 *      until interactive parity lands.
 *   - `skill`: native's `skill` command spawns `octocode skill` (this Node CLI),
 *      so it MUST stay TS — delegating it would infinitely re-enter native.
 *      Skill materialization is owned by @octocodeai/octocode-skill-installer.
 *
 * Note: there is no `sync` command — MCP sync analysis is exposed via
 * `status --sync`, which native now supports at byte parity, so `status`
 * delegates freely.
 */
export const TS_ONLY_COMMANDS: ReadonlySet<string> = new Set([
  'lsp-server',
  'install',
  'skill',
]);

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
 * Decide whether a parsed command should be delegated to the native binary.
 * Requires opt-in (`OCTOCODE_RUNTIME=native`), a command native covers, and a
 * resolvable native binary.
 */
export function shouldDelegateToNative(
  command: string | null | undefined,
  env: NodeJS.ProcessEnv = process.env
): boolean {
  if (!command) return false;
  if (env.OCTOCODE_RUNTIME?.trim().toLowerCase() !== 'native') return false;
  if (TS_ONLY_COMMANDS.has(command)) return false;
  return resolveNativeBin(env) !== null;
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
