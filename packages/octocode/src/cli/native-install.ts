// Install-time step (the package's postinstall): make the `octocode` bin the
// native binary itself, so each command runs it directly instead of starting
// Node to start it (esbuild optimizes its bin the same way). The bin stays the
// Node launcher stub, untouched, whenever that cannot be done safely; the
// launcher then resolves, or downloads, the binary at each call as before.
//
// The new bin is staged next to the old one and renamed over it, so the bin is
// always either the stub or a native binary that just reported the pinned
// version. A hard link shares the platform package's file and stays valid if
// that package is removed later; a copy stands in where linking is refused.
import {
  chmodSync,
  copyFileSync,
  linkSync,
  readFileSync,
  renameSync,
  rmSync,
} from 'node:fs';
import { dirname, join } from 'node:path';
import * as resolver from '../../../octocode-native/bin/resolve-binary.cjs';
import { verifyBinaryVersion } from '../../../octocode-native/bin/fetch-binary.cjs';
import { errorMessage } from './native-delegate.js';

/**
 * `expected` is true when the install context itself needs the Node launcher
 * (see `nodeRequired`), false when something went wrong for this install.
 */
export type InstallOutcome =
  | { bin: 'native'; path: string }
  | { bin: 'node'; reason: string; expected: boolean };

export interface InstallOptions {
  /** The installed `octocode` package directory. */
  packageRoot: string;
  /** The pinned platform package version; only a bundled build has one. */
  version: string | undefined;
  env?: NodeJS.ProcessEnv;
  platform?: NodeJS.Platform;
  resolveNative?: () => string;
  link?: (from: string, to: string) => void;
  /** What `bin --version` prints. */
  run?: (bin: string) => string;
}

/**
 * Why the bin must stay a Node script here, or undefined when it may be the
 * native binary: only an installed copy (inside a node_modules tree) may be
 * swapped, never the workspace source package; Windows bin shims (`.cmd`, `.ps1`) run their target with
 * Node, Yarn 2+ runs every package bin through Node, and a pnpm global
 * install keeps the `node` shim it wrote for the stub: pnpm rewrites the
 * project's shims after a script approval but not the global bin's, so the
 * swapped binary would run as `node <binary>`.
 */
function nodeRequired(
  platform: NodeJS.Platform,
  env: NodeJS.ProcessEnv,
  packageRoot: string
): string | undefined {
  if (!/(^|[\\/])node_modules[\\/]/.test(packageRoot)) {
    return 'a source checkout, not an installed copy';
  }
  if (platform === 'win32') return 'Windows bin shims start Node';
  const yarn = /\byarn\/(\d+)/.exec(env.npm_config_user_agent ?? '');
  if (yarn && Number(yarn[1]) >= 2) {
    return `Yarn ${yarn[1]} runs package bins through Node`;
  }
  if (
    /\bpnpm\//.test(env.npm_config_user_agent ?? '') &&
    /[\\/]global[\\/]v\d+[\\/]/.test(packageRoot)
  ) {
    return 'pnpm global bin shims start Node';
  }
  return undefined;
}

/** The bin path the package manifest declares. */
function declaredBin(packageRoot: string): string {
  const manifest = JSON.parse(
    readFileSync(join(packageRoot, 'package.json'), 'utf8')
  ) as { bin?: unknown };
  if (typeof manifest.bin !== 'string') {
    throw new Error(`${packageRoot}/package.json declares no single bin`);
  }
  return join(packageRoot, manifest.bin);
}

/** Point the package's bin at the native binary when that is safe. Never throws. */
export function installNativeBin({
  packageRoot,
  version,
  env = process.env,
  platform = process.platform,
  resolveNative = resolver.resolveNativeBinaryPath,
  link = linkSync,
  run,
}: InstallOptions): InstallOutcome {
  const blocked = nodeRequired(platform, env, packageRoot);
  if (blocked) return { bin: 'node', reason: blocked, expected: true };
  if (!version) {
    return { bin: 'node', reason: 'no pinned native version', expected: false };
  }
  let staged: string | undefined;
  try {
    const native = resolveNative();
    const bin = declaredBin(packageRoot);
    staged = join(dirname(bin), `.octocode.${process.pid}.tmp`);
    rmSync(staged, { force: true });
    try {
      link(native, staged);
    } catch {
      copyFileSync(native, staged);
      chmodSync(staged, 0o755);
    }
    verifyBinaryVersion(staged, version, run);
    renameSync(staged, bin);
    return { bin: 'native', path: bin };
  } catch (error) {
    return { bin: 'node', reason: errorMessage(error), expected: false };
  } finally {
    if (staged) rmSync(staged, { force: true });
  }
}

/** The one stderr line for a Node launcher that stays unexpectedly; undefined when nothing needs saying. */
export function installNotice(outcome: InstallOutcome): string | undefined {
  if (outcome.bin === 'native' || outcome.expected) return undefined;
  return `octocode: the octocode command stays the Node launcher: ${outcome.reason}`;
}
