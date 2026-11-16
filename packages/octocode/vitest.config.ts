import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    globals: true,
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    // Temp home, no credentials, dead proxy, guarded fetch and child processes.
    setupFiles: [
      '../../test-utils/external-effects-guard.ts',
      'tests/setup.ts',
    ],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      include: ['src/**/*.ts'],
      thresholds: {
        statements: 70,
        branches: 60,
        functions: 80,
        lines: 71,
      },
      // Entry points: their side effects run in the packaged install checks.
      exclude: ['src/index.ts', 'src/install.ts'],
    },
    testTimeout: 30000,
    hookTimeout: 15000,
    restoreMocks: true,
    clearMocks: true,
  },
});
