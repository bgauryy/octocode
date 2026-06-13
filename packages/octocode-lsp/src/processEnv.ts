export const CORE_ALLOWED_ENV_VARS = [
  'PATH',
  'TMPDIR',
  'TMP',
  'TEMP',
  'SYSTEMROOT',
  'WINDIR',
  'COMSPEC',
  'PATHEXT',
] as const;

export const TOOLING_ALLOWED_ENV_VARS = [
  ...CORE_ALLOWED_ENV_VARS,
  'HOME',
  'USERPROFILE',
  'APPDATA',
  'LOCALAPPDATA',
] as const;

export function buildChildProcessEnv(
  envOverrides: Record<string, string | undefined> = {},
  allowEnvVars: readonly string[] = CORE_ALLOWED_ENV_VARS
): typeof process.env {
  const childEnv: Record<string, string | undefined> = {};

  for (const key of allowEnvVars) {
    const value = process.env[key];
    if (value !== undefined) {
      childEnv[key] = value;
    }
  }

  const allowSet = new Set<string>(allowEnvVars);
  for (const [key, value] of Object.entries(envOverrides)) {
    if (!allowSet.has(key)) continue;
    if (value === undefined) {
      delete childEnv[key];
    } else {
      childEnv[key] = value;
    }
  }

  return childEnv as typeof process.env;
}
