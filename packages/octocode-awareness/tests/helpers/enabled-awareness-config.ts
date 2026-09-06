import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const ENABLED_TEST_CONFIG = JSON.stringify({
  version: 1,
  features: {
    hooks: true,
    notifications: true,
    verificationGate: true,
    sessionCapture: true,
    maintenanceReminders: true,
  },
}, null, 2);

/** Give hook subprocess tests the explicit onboarding that production requires. */
export function withEnabledAwarenessConfig(
  env: Record<string, string | undefined>,
): Record<string, string | undefined> {
  const home = env.OCTOCODE_HOME ?? env.OCTOCODE_AGENT_DIR;
  if (!home) return env;
  mkdirSync(home, { recursive: true });
  const configPath = join(home, 'awareness.json');
  if (!existsSync(configPath)) writeFileSync(configPath, `${ENABLED_TEST_CONFIG}\n`, { mode: 0o600 });
  return { ...env, OCTOCODE_HOME: home };
}
