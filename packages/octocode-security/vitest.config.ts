import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    globals: false,
    environment: 'node',
    setupFiles: ['./tests/setup.ts'],
    include: ['tests/**/*.test.ts'],
    minWorkers: 1,
    maxWorkers: 1,
    coverage: {
      provider: 'v8',
      include: ['src/**/*.ts'],
      exclude: ['src/native.ts', 'src/regexes/types.ts'],
      thresholds: { statements: 85, branches: 80, functions: 85, lines: 85 },
      reporter: ['text', 'lcov'],
    },
    testTimeout: 15000,
  },
});
