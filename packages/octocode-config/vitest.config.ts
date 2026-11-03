import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    // Contract generation takes seconds on a busy machine; the checks build it once per file, and a
    // loaded host must not turn a slow run into a failure.
    testTimeout: 120_000,
    hookTimeout: 120_000,
    // Temp home, no credentials, dead proxy, guarded fetch and child processes.
    setupFiles: ['../../test-utils/external-effects-guard.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      include: ['src/**/*.ts'],
      exclude: ['src/**/*.generated.ts'],
      thresholds: {
        statements: 98,
        branches: 90,
        functions: 100,
        lines: 90,
      },
    },
  },
});
