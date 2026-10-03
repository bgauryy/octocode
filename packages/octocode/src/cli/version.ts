import { createRequire } from 'node:module';

declare const __APP_VERSION__: string | undefined;

/**
 * `--version` names both layers: the installed `octocode` package and the
 * native runtime it pins. The native binary alone reports only its own crate
 * version, which differs from the npm package users install.
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
  return native
    ? `octocode ${launcher} (native ${native})`
    : `octocode ${launcher}`;
}
