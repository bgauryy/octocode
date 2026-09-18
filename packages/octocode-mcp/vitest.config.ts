import { defineConfig } from 'vitest/config';
import { readFileSync } from 'fs';

export default defineConfig({
  build: { sourcemap: false },
  server: { sourcemapIgnoreList: () => true },
  test: {
    environment: 'node',
    globals: true,
    include: ['tests/**/*.test.ts'],
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
  plugins: [
    {
      name: 'markdown-loader',
      transform(_code, id) {
        if (id.endsWith('.md')) {
          const content = readFileSync(id, 'utf-8');
          return {
            code: `export default ${JSON.stringify(content)};`,
            map: null,
          };
        }
      },
    },
  ],
});
