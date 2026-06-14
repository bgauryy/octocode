import { defineConfig } from 'vitest/config';
import { readFileSync } from 'fs';
import { resolve } from 'path';

export default defineConfig({
  esbuild: { sourcemap: false },
  build: { sourcemap: false },
  server: { sourcemapIgnoreList: () => true },
  resolve: {
    alias: {
      '@octocodeai/octocode-tools-core': resolve(__dirname, '../octocode-tools-core/src/index.ts'),
    },
  },
  test: {
    environment: 'node',
    globals: true,
    testTimeout: 10000,
    hookTimeout: 1000,
    teardownTimeout: 1000,
    dangerouslyIgnoreUnhandledErrors: true,
    setupFiles: ['./tests/setup.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      include: ['src/**/*.ts'],
      exclude: ['src/**/*.test.ts', 'src/**/*.spec.ts'],
      thresholds: {
        statements: 90,
        branches: 87,
        functions: 90,
        lines: 90,
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
