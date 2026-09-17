// Runtime selection seam for the MCP server.
//
// The default MCP runtime is the TypeScript tools-core runner (see index.ts).
// The native Rust runtime (packages/octocode-native, loaded as a NAPI addon and
// bridged in ./index.mjs) is strictly OPT-IN and only activates when a working
// addon can be resolved. A blunt default-flip is deliberately NOT done here:
//   - native/tools-core parity must first be proven by the differential harness
//     under tests/native/parity.
// Until both hold, the default stays tools-core and native selection always
// falls back safely on any failure.
import { createRequire } from 'node:module';

export type RuntimeKind = 'native' | 'tools-core';

/**
 * Resolve a loadable native NAPI addon and return its module path, or null when
 * none is available. A candidate is only accepted when it can be required and
 * exports the `NativeRuntime` class the bridge depends on.
 *
 * Resolution order: explicit binding, then the installed platform loader.
 */
export function resolveNativeAddon(
  env: NodeJS.ProcessEnv = process.env
): string | null {
  const require = createRequire(import.meta.url);
  let installed: string | undefined;
  try {
    installed = require.resolve('@octocodeai/octocode-native/native.cjs');
  } catch {
    installed = undefined;
  }
  for (const candidate of [env.OCTOCODE_NATIVE_BINDING, installed]) {
    if (!candidate) continue;
    try {
      const binding = require(candidate) as { NativeRuntime?: unknown };
      if (typeof binding.NativeRuntime === 'function') return candidate;
    } catch {
      // Try the next source.
    }
  }
  return null;
}

/**
 * Select the MCP tool runtime. Native requires BOTH an explicit opt-in
 * (`OCTOCODE_RUNTIME=native`) AND a resolvable addon; otherwise the default
 * tools-core runtime is used.
 */
export function selectRuntime(
  env: NodeJS.ProcessEnv = process.env
): RuntimeKind {
  const optedIn = env.OCTOCODE_RUNTIME?.trim().toLowerCase() === 'native';
  return optedIn && resolveNativeAddon(env) ? 'native' : 'tools-core';
}

/**
 * Boot the selected runtime with graceful fallback. When native is selected but
 * its startup throws, a warning is emitted and the tools-core runtime is booted
 * instead — the server must never fail to start because of the opt-in native
 * path. Returns the runtime that actually started.
 */
export async function bootRuntime(options: {
  startNative: () => Promise<void>;
  startToolsCore: () => Promise<void>;
  env?: NodeJS.ProcessEnv;
  warn?: (message: string) => void;
}): Promise<RuntimeKind> {
  const env = options.env ?? process.env;
  const warn =
    options.warn ?? (message => process.stderr.write(`${message}\n`));

  if (selectRuntime(env) === 'native') {
    try {
      await options.startNative();
      return 'native';
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      warn(
        `Native runtime unavailable, falling back to tools-core: ${message}`
      );
    }
  }

  await options.startToolsCore();
  return 'tools-core';
}
