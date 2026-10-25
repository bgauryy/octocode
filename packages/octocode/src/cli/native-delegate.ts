// The npm CLI only starts the native binary. The platform resolver and the
// signal-forwarding launcher are the native package's own scripts, bundled
// here at build time; the binary comes from the one per-platform package npm
// installs (`optionalDependencies`, selected by its `os`/`cpu`). When npm
// skipped that package, a bundled launcher downloads it once, at the exact
// pinned version, into the Octocode home.
import { existsSync } from 'node:fs';
import { devOverridesAllowed, getOctocodeHome } from '@octocodeai/config';
import * as resolver from '../../../octocode-native/bin/resolve-binary.cjs';
import * as fetcher from '../../../octocode-native/bin/fetch-binary.cjs';
import * as launcher from '../../../octocode-native/bin/launch-native.cjs';

declare const __OCTOCODE_BUNDLED__: boolean | undefined;
declare const __OCTOCODE_NATIVE_VERSION__: string | undefined;
const devOverrideOptions = {
  bundled:
    typeof __OCTOCODE_BUNDLED__ !== 'undefined' &&
    __OCTOCODE_BUNDLED__ === true,
};
/** The pinned platform package version; only a bundled launcher has one. */
const bundledNativeVersion =
  typeof __OCTOCODE_NATIVE_VERSION__ === 'string'
    ? __OCTOCODE_NATIVE_VERSION__
    : undefined;

type NativeBin = { bin: string } | { error: string };

export class NativeLaunchError extends Error {
  override name = 'NativeLaunchError';
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function missingSuffix(error: unknown): string | undefined {
  if (typeof error !== 'object' || error === null) return undefined;
  const { code, suffix } = error as { code?: unknown; suffix?: unknown };
  return code === resolver.PLATFORM_PACKAGE_MISSING &&
    typeof suffix === 'string'
    ? suffix
    : undefined;
}

/**
 * The native binary to run: `OCTOCODE_NATIVE_BIN` in development, else the
 * installed platform package's binary, else (given `nativeVersion`) the
 * platform package fetched from the registry. Never throws.
 */
export async function resolveNativeBin(
  env: NodeJS.ProcessEnv = process.env,
  nativeVersion: string | undefined = bundledNativeVersion
): Promise<NativeBin> {
  const explicit = devOverridesAllowed(env, devOverrideOptions)
    ? env.OCTOCODE_NATIVE_BIN?.trim()
    : undefined;
  if (explicit) {
    return existsSync(explicit)
      ? { bin: explicit }
      : { error: `OCTOCODE_NATIVE_BIN points at a missing path: ${explicit}` };
  }
  try {
    return { bin: resolver.resolveNativeBinaryPath() };
  } catch (error) {
    const suffix = missingSuffix(error);
    if (!suffix || !nativeVersion) return { error: errorMessage(error) };
    try {
      return {
        bin: await fetcher.fetchNativeBinary({
          suffix,
          version: nativeVersion,
          home: getOctocodeHome(env),
          env,
        }),
      };
    } catch (fetchError) {
      return {
        error: `${errorMessage(error)}\nDownloading it from the registry failed too: ${errorMessage(fetchError)}`,
      };
    }
  }
}

export async function delegateToNative(
  bin: string,
  argv: readonly string[],
  env: NodeJS.ProcessEnv = process.env
): Promise<number> {
  try {
    return await launcher.runForwardingSignals(bin, argv, { env });
  } catch (error) {
    throw new NativeLaunchError(
      `The native Octocode binary could not start (${bin}): ${errorMessage(error)}`,
      { cause: error }
    );
  }
}
