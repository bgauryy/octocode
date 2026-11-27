import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    globals: true,
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      include: ['src/**/*.ts'],
      exclude: ['src/extension.ts'],
      thresholds: {
        statements: 97,
        branches: 95,
        functions: 96,
        lines: 97,
      },
    },
    restoreMocks: true,
    clearMocks: true,
  },
});
