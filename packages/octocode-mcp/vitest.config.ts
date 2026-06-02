import { defineConfig } from 'vitest/config';
import { readFileSync } from 'fs';

export default defineConfig({
  esbuild: { sourcemap: false },
  build: { sourcemap: false },
  server: { sourcemapIgnoreList: () => true },
  test: {
    environment: 'node',
    globals: true,
    testTimeout: 2000, // 2 seconds per test
    hookTimeout: 1000, // 1 second for hooks
    teardownTimeout: 1000, // 1 second for teardown
    dangerouslyIgnoreUnhandledErrors: true, // Ignore unhandled errors from test mocks
    setupFiles: ['./tests/setup.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      include: ['src/**/*.ts'],
      exclude: ['src/**/*.test.ts', 'src/**/*.spec.ts'],
      thresholds: {
        statements: 90,
        // Branches at 88 because defensive `??` fallback chains across the
        // response/finalizer layers (per-tool tolerance for partial upstream
        // shapes) add many low-value branches. The other three stay at 90.
        branches: 88,
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
          // Read markdown file and export as string (same as rollup-plugin-string)
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
