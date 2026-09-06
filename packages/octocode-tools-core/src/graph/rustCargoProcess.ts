import { execFile } from 'node:child_process';
import { access, realpath, stat } from 'node:fs/promises';
import { constants } from 'node:fs';
import { homedir } from 'node:os';
import {
  basename,
  delimiter,
  dirname,
  isAbsolute,
  join,
  relative,
  resolve,
  sep,
} from 'node:path';

export const TIMEOUT_MS = 5000;
export const MAX_BUFFER = 1_048_576;
const MAX_MANIFEST_PROBES = 64;

export class MetadataFailure extends Error {
  constructor(
    readonly code: string,
    message: string
  ) {
    super(message);
  }
}
export function within(root: string, path: string): boolean {
  const child = relative(root, path);
  return (
    child === '' ||
    (!child.startsWith(`..${sep}`) && child !== '..' && !isAbsolute(child))
  );
}
export function remaining(deadline: number): number {
  const milliseconds = deadline - Date.now();
  if (milliseconds <= 0)
    throw new MetadataFailure(
      'cargo-metadata-limit',
      'Cargo metadata exceeded its five-second execution budget.'
    );
  return milliseconds;
}
export function command(
  file: string,
  args: string[],
  cwd: string,
  env: NodeJS.ProcessEnv,
  deadline: number
): Promise<string> {
  return new Promise((fulfill, reject) => {
    execFile(
      file,
      args,
      {
        cwd,
        env,
        encoding: 'utf8',
        shell: false,
        timeout: remaining(deadline),
        maxBuffer: MAX_BUFFER,
      },
      (error, stdout) => {
        if (error) {
          const limited =
            error.killed || error.code === 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER';
          reject(
            new MetadataFailure(
              limited
                ? 'cargo-metadata-limit'
                : error.code === 'ENOENT'
                  ? 'cargo-unavailable'
                  : 'cargo-metadata-failed',
              limited
                ? 'Cargo metadata exceeded its time or output budget.'
                : 'The host Cargo command could not provide offline, locked workspace metadata.'
            )
          );
        } else if (Buffer.byteLength(stdout, 'utf8') > MAX_BUFFER) {
          reject(
            new MetadataFailure(
              'cargo-metadata-limit',
              'Cargo metadata exceeded its output budget.'
            )
          );
        } else fulfill(stdout);
      }
    );
  });
}

/** Use host PATH entries only; project rust-toolchain overrides never select the executable. */
export async function hostCargo(
  root: string,
  deadline: number
): Promise<{ executable: string; env: NodeJS.ProcessEnv }> {
  const paths: string[] = [];
  for (const entry of (process.env.PATH ?? '').split(delimiter)) {
    if (!isAbsolute(entry)) continue;
    try {
      const canonical = await realpath(entry);
      if (!within(root, canonical)) paths.push(canonical);
    } catch {
      /* Missing host PATH entries do not supply executables. */
    }
    remaining(deadline);
  }
  const home = homedir();
  // Avoid carrying repository-provided tool/wrapper/loader environment overrides.
  const env: NodeJS.ProcessEnv = {
    PATH: [...new Set(paths)].join(delimiter),
    HOME: home,
    USERPROFILE: home,
    RUSTUP_AUTO_INSTALL: '0',
    CARGO_NET_OFFLINE: 'true',
  };
  for (const key of [
    'SystemRoot',
    'SYSTEMROOT',
    'WINDIR',
    'TEMP',
    'TMP',
    'LANG',
    'LC_ALL',
  ]) {
    if (process.env[key]) env[key] = process.env[key];
  }
  const executableName = process.platform === 'win32' ? 'cargo.exe' : 'cargo';
  for (const directory of paths) {
    const executable = join(directory, executableName);
    try {
      await access(executable, constants.X_OK);
      const canonical = await realpath(executable);
      if (within(root, canonical)) continue;
      const siblingRustup = join(
        directory,
        process.platform === 'win32' ? 'rustup.exe' : 'rustup'
      );
      let proxy = /^rustup(?:\.exe)?$/i.test(basename(canonical));
      if (!proxy) {
        try {
          const [cargoStat, rustupStat] = await Promise.all([
            stat(executable),
            stat(siblingRustup),
          ]);
          proxy =
            cargoStat.dev === rustupStat.dev &&
            cargoStat.ino === rustupStat.ino;
        } catch {
          /* Standalone Cargo installations have no rustup proxy. */
        }
      }
      if (!proxy) return { executable, env };
      const rustup = /^rustup(?:\.exe)?$/i.test(basename(canonical))
        ? canonical
        : siblingRustup;
      const selected = (
        await command(rustup, ['which', 'cargo'], home, env, deadline)
      ).trim();
      if (!isAbsolute(selected))
        throw new MetadataFailure(
          'cargo-unavailable',
          'Rustup did not identify an absolute host Cargo executable.'
        );
      const selectedCanonical = await realpath(selected);
      if (within(root, selectedCanonical))
        throw new MetadataFailure(
          'cargo-unavailable',
          'The selected Cargo executable is inside the scanned repository.'
        );
      await access(selectedCanonical, constants.X_OK);
      return { executable: selectedCanonical, env };
    } catch (error) {
      if (error instanceof MetadataFailure) throw error;
    }
    remaining(deadline);
  }
  throw new MetadataFailure(
    'cargo-unavailable',
    'No trusted host Cargo executable is available on the absolute PATH.'
  );
}

/** Probe only ancestors of known Rust files, stopping at the supplied scan root. */
export async function manifests(
  root: string,
  files: readonly string[],
  deadline: number
): Promise<string[]> {
  const checked = new Map<string, string | null>();
  async function probe(directory: string): Promise<string | null> {
    remaining(deadline);
    if (checked.has(directory)) return checked.get(directory) ?? null;
    if (checked.size >= MAX_MANIFEST_PROBES)
      throw new MetadataFailure(
        'cargo-metadata-limit',
        'Cargo manifest discovery exceeded 64 ancestor-directory probes.'
      );
    checked.set(directory, null);
    const candidate = join(directory, 'Cargo.toml');
    try {
      const canonical = await realpath(candidate);
      if (!within(root, canonical))
        throw new MetadataFailure(
          'cargo-manifest-outside-root',
          'A Cargo manifest symlink leaves the scanned root.'
        );
      if (!(await stat(canonical)).isFile()) return null;
      checked.set(directory, canonical);
      return canonical;
    } catch (error) {
      if (error instanceof MetadataFailure) throw error;
      return null;
    }
  }
  const rootManifest = await probe(root);
  if (rootManifest) return [rootManifest];
  const found = new Set<string>();
  for (const file of [...files].sort()) {
    if (!file.endsWith('.rs')) continue;
    let directory = dirname(resolve(root, file));
    while (within(root, directory)) {
      const manifest = await probe(directory);
      if (manifest) {
        found.add(manifest);
        break;
      }
      if (directory === root) break;
      directory = dirname(directory);
    }
  }
  if (!found.size)
    throw new MetadataFailure(
      'cargo-manifest-missing',
      'No Cargo.toml was found at the scan root or known Rust-file ancestors.'
    );
  return [...found];
}
