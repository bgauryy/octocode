import { createRequire } from 'node:module';

declare const __APP_VERSION__: string | undefined;

/**
 * `--version` prints the one release version, read from the installed
 * `octocode` package.json. The native runtime ships at the same version; a
 * mismatched install also names the native version it loaded.
 */
export function versionLine(): string | null {
  const launcher =
    typeof __APP_VERSION__ === 'string' ? __APP_VERSION__ : undefined;
  if (!launcher) return null;
  let native: string | undefined;
  try {
    const require = createRequire(import.meta.url);
    native = (
      require('@octocodeai/octocode-native/package.json') as {
        version?: string;
      }
    ).version;
  } catch {
    native = undefined;
  }
  return formatVersion(launcher, native);
}

export function formatVersion(
  launcher: string,
  native: string | undefined
): string {
  return native && native !== launcher
    ? `octocode ${launcher} (native ${native})`
    : `octocode ${launcher}`;
}
