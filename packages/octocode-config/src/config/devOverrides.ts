/**
 * Development-only override gate shared by every interface that composes core
 * presentation with the native runtime (MCP server, CLI `schema`). One rule,
 * one message, so the surfaces cannot drift apart.
 */

/** Downgrades a core/native contract fingerprint mismatch to a warning. */
export const CONTRACT_DRIFT_OVERRIDE_ENV = 'OCTOCODE_ALLOW_CONTRACT_DRIFT';

export interface DevOverrideOptions {
  /**
   * True in a shipped bundle (set by the package's esbuild define), false when
   * running from source (vitest, tsx).
   */
  bundled: boolean;
}

type EnvLike = Readonly<Record<string, string | undefined>>;

/**
 * Dev-only overrides (`OCTOCODE_NATIVE_BINDING`, `OCTOCODE_ALLOW_CONTRACT_DRIFT`)
 * are never honoured under `NODE_ENV=production`. A shipped bundle is treated as
 * production by default — `npx` and registry installs leave `NODE_ENV` unset —
 * so it honours them only when `NODE_ENV` explicitly opts in with
 * `development` or `test`. Read `NODE_ENV` from the passed env at runtime: a
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

/** True when a contract fingerprint mismatch may run with a warning. */
export function contractDriftAllowed(
  env: EnvLike,
  options: DevOverrideOptions
): boolean {
  return (
    env[CONTRACT_DRIFT_OVERRIDE_ENV] === '1' &&
    devOverridesAllowed(env, options)
  );
}

/** The fail-closed contract-drift diagnostic, naming every override condition. */
export function contractDriftMessage(
  coreFingerprint: string,
  nativeFingerprint: string
): string {
  return (
    `Contract drift (fingerprint mismatch): @octocodeai/octocode-core ${coreFingerprint} ` +
    `!= native runtime ${nativeFingerprint}. Run \`yarn contracts:regen\` and rebuild ` +
    'native (or install matching octocode packages). To override while iterating ' +
    `locally, set ${CONTRACT_DRIFT_OVERRIDE_ENV}=1; a bundled build also needs ` +
    'NODE_ENV=development (or test), and NODE_ENV=production always fails closed.'
  );
}
