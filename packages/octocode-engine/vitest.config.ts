import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    coverage: {
      provider: 'v8',
      // Measure the maintained TypeScript runtime surface. Build scripts and
      // CommonJS loader shims are exercised by package/build smoke tests rather
      // than unit-coverage thresholds.
      include: ['src/**/*.ts'],
      exclude: [
        'target/**',
        'coverage/**',
        'node_modules/**',
        '*.node',
        '*.d.ts',
      ],
      reporter: ['text'],
      thresholds: {
        statements: 83,
        branches: 74,
        functions: 95,
        lines: 85,
      },
    },
  },
});
