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
        statements: 88,
        branches: 82,
        functions: 87,
        lines: 88,
      },
    },
    restoreMocks: true,
    clearMocks: true,
  },
});
