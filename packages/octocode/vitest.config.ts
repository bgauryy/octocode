import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    globals: true,
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    // Against the real native binary: `yarn test:e2e`, in verify:own after a build.
    exclude: [
      'tests/e2e/launcher.e2e.test.ts',
      'tests/e2e/config-view.e2e.test.ts',
    ],
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
      exclude: ['src/index.ts'],
    },
    testTimeout: 30000,
    hookTimeout: 15000,
    restoreMocks: true,
    clearMocks: true,
  },
});
