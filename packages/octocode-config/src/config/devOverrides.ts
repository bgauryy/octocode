/**
 * Development-only override gate shared by the launchers that start the native
 * binary (`octocode`, `octocode-mcp`). One rule, so they cannot drift apart.
 */

export interface DevOverrideOptions {
  /**
   * True in a shipped bundle (set by the package's esbuild define), false when
   * running from source (vitest, tsx).
   */
  bundled: boolean;
}

type EnvLike = Readonly<Record<string, string | undefined>>;

/**
 * The dev-only override (`OCTOCODE_NATIVE_BIN`) is never honoured under
 * `NODE_ENV=production`. A shipped bundle is treated as production by default
 * — `npx` and registry installs leave `NODE_ENV` unset — so it honours it only
 * when `NODE_ENV` explicitly opts in with `development` or `test`. Read `NODE_ENV` from the passed env at runtime: a
 * bundle's compile-time `process.env.NODE_ENV` define must not decide this.
 */
export function devOverridesAllowed(
  env: EnvLike,
  { bundled }: DevOverrideOptions
): boolean {
  const nodeEnv = env['NODE_ENV'];
  if (nodeEnv === 'production') return false;
  if (!bundled) return true;
  return nodeEnv === 'development' || nodeEnv === 'test';
}
