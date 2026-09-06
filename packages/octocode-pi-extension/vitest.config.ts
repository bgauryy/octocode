import os from 'node:os';
import path from 'node:path';
import { defineConfig } from 'vitest/config';

const TEST_OCTOCODE_HOME = path.join(os.tmpdir(), 'octocode-pi-ext-test-home', '.octocode');

export default defineConfig({
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    setupFiles: ['../../test-utils/external-effects-guard.ts'],
    // Clear shell-inherited env vars that alter extension behaviour under test.
    // OCTOCODE_PI_SUBAGENT=1 causes registerAgentTools() to early-return,
    // so spawnAgent / AgentMessage would never be registered.
    env: {
      OCTOCODE_PI_SUBAGENT: '',
      OCTOCODE_HOME: TEST_OCTOCODE_HOME,
      OCTOCODE_CHROME_DEBUG_E2E: '',
      RUN_CHROME_LIVE: '',
      RUN_MCP_LIVE: '',
    },
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      include: ['src/**/*.ts'],
      exclude: [
        'src/ambient.d.ts',
        'src/types.ts',
      ],
      thresholds: {
        // Package exception to the repo-wide 90% branch target: CDP and bash
        // subprocess branches require live Chrome/OS integration harnesses, so
        // 65% is the realistic floor for unit coverage. Raise this as new
        // integration harnesses land; never lower it without code-reviewed proof.
        // statements/lines/functions cover pure logic; keep these higher.
        branches: 65,
        functions: 80,
        lines: 85,
        statements: 80,
      },
    },
  },
});
