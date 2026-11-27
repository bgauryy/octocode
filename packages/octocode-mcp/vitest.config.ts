import { defineConfig } from 'vitest/config';

export default defineConfig({
  build: { sourcemap: false },
  server: { sourcemapIgnoreList: () => true },
  test: {
    environment: 'node',
    globals: true,
    include: ['tests/**/*.test.ts'],
    // Temp home, no credentials, dead proxy, guarded fetch and child processes.
    setupFiles: ['../../test-utils/external-effects-guard.ts'],
    testTimeout: 10000,
    hookTimeout: 1000,
    teardownTimeout: 1000,
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      include: ['src/**/*.ts'],
      exclude: ['src/**/*.test.ts', 'src/**/*.spec.ts'],
      thresholds: {
        statements: 78,
        branches: 64,
        functions: 79,
        lines: 80,
      },
    },
  },
});
