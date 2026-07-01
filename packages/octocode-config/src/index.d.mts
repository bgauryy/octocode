/**
 * Resolve the Octocode home directory.
 * Reads OCTOCODE_HOME, then falls back to platform default.
 */
export function getOctocodeHome(env?: Record<string, string | undefined>): string;

/** Keys that .env files must never override (infra + auth tokens). */
export const PROTECTED_KEYS: ReadonlySet<string>;

/**
 * Parse dotenv-format text into a key→value map.
 * Handles `#` comments, `export ` prefix, surrounding quotes. No shell expansion.
 */
export function parseEnv(text: string | null | undefined): Record<string, string>;

/**
 * Load merged env from global `<home>/.env` then project `<cwd>/.octocode/.env`.
 * Project file is only read when `trusted = true`.
 */
export function loadOctocodeEnv(opts?: {
  home?: string;
  cwd?: string;
  trusted?: boolean;
}): {
  map: Record<string, string>;
  sources: Record<string, 'global' | 'project'>;
};

/**
 * Apply a parsed env map into `env` (default: `process.env`).
 * Skips already-set keys and PROTECTED_KEYS.
 * Returns key names only — never values.
 */
export function applyOctocodeEnv(
  map: Record<string, string> | null | undefined,
  opts?: { env?: Record<string, string | undefined> },
): {
  applied: string[];
  skippedProtected: string[];
  skippedExisting: string[];
};

/**
 * Convenience: load + apply in one call.
 * Returns names-only metadata; never leaks values.
 */
export function propagateOctocodeEnv(opts?: {
  home?: string;
  cwd?: string;
  trusted?: boolean;
  env?: Record<string, string | undefined>;
}): {
  applied: string[];
  skippedProtected: string[];
  skippedExisting: string[];
  sources: Record<string, 'global' | 'project'>;
  keys: string[];
};

/**
 * Read and parse `<home>/.octocoderc` (JSON with line/block comments and trailing commas).
 * Returns `{}` when absent or invalid. No secrets belong here.
 */
export function loadOctocoderc(home?: string): Record<string, unknown>;
