import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    // Clear shell-inherited env vars that alter extension behaviour under test.
    // OCTOCODE_PI_SUBAGENT=1 causes registerAgentTools() to early-return,
    // so spawnAgent / AgentMessage would never be registered.
    env: {
      OCTOCODE_PI_SUBAGENT: '',
    },
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      include: ['src/**/*.ts'],
      exclude: ['src/ambient.d.ts', 'src/types.ts'],
    },
  },
});
