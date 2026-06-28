import { existsSync, readdirSync } from 'node:fs';

/**
 * Canonical `{os}-{arch}[-musl]` platform identifier, matching the npm
 * native-addon convention (the same keys esbuild / swc / the engine's own
 * optionalDependencies use): `darwin-arm64`, `darwin-x64`, `linux-x64`,
 * `linux-arm64`, `linux-x64-musl`, `linux-arm64-musl`, `win32-x64`,
 * `win32-arm64`. This is the key the server download manifest is keyed on.
 */
export type PlatformId =
  | 'darwin-arm64'
  | 'darwin-x64'
  | 'linux-x64'
  | 'linux-arm64'
  | 'linux-x64-musl'
  | 'linux-arm64-musl'
  | 'win32-x64'
  | 'win32-arm64';

let cachedPlatformId: PlatformId | undefined;

/** True when the current Linux runtime links musl libc (Alpine etc.). */
export function isMuslLinux(): boolean {
  if (process.platform !== 'linux') return false;
  // 1) Fastest, in-process: glibc runtime version is absent on musl.
  try {
    const report = process.report?.getReport() as
      | { header?: { glibcVersionRuntime?: string } }
      | undefined;
    if (report?.header) {
      return report.header.glibcVersionRuntime == null;
    }
  } catch {
    // process.report unavailable — fall through to the loader probe.
  }
  // 2) Dynamic-loader probe: musl ships `/lib/ld-musl-*.so.1`.
  try {
    return readdirSync('/lib').some(name => name.startsWith('ld-musl-'));
  } catch {
    return false;
  }
}

/** Resolve (and cache) the canonical platform id for this machine. */
export function detectPlatformId(): PlatformId {
  if (cachedPlatformId) return cachedPlatformId;
  const arch = process.arch === 'x64' ? 'x64' : 'arm64';
  if (process.platform === 'darwin') {
    cachedPlatformId = `darwin-${arch}` as PlatformId;
  } else if (process.platform === 'win32') {
    cachedPlatformId = `win32-${arch}` as PlatformId;
  } else {
    const suffix = isMuslLinux() ? '-musl' : '';
    cachedPlatformId = `linux-${arch}${suffix}` as PlatformId;
  }
  return cachedPlatformId;
}

/**
 * Executable file-name candidates for a bare command on this OS. On Windows a
 * server may exist only as `gopls.exe` / `pyright.cmd`, so we expand against
 * `PATHEXT`; on POSIX the bare name is the only candidate.
 */
export function executableNames(command: string): string[] {
  if (process.platform !== 'win32') return [command];
  const pathext = (process.env.PATHEXT ?? '.EXE;.CMD;.BAT;.COM')
    .split(';')
    .map(ext => ext.trim().toLowerCase())
    .filter(Boolean);
  return [command, ...pathext.map(ext => `${command}${ext}`)];
}

/** First existing path among `dir/<name>` for every executable-name candidate. */
export function firstExecutableIn(
  dir: string,
  command: string,
  join: (dir: string, name: string) => string
): string | null {
  for (const name of executableNames(command)) {
    const candidate = join(dir, name);
    if (existsSync(candidate)) return candidate;
  }
  return null;
}
