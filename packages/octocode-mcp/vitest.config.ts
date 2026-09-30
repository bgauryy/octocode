import { defineConfig } from 'vitest/config';
import { readFileSync } from 'fs';

export default defineConfig({
  build: { sourcemap: false },
  server: { sourcemapIgnoreList: () => true },
  test: {
    environment: 'node',
    globals: true,
    include: ['tests/**/*.test.ts'],
    // CI never builds the native addon (OCTOCODE_NO_NATIVE=1); these suites load it.
    exclude: process.env.OCTOCODE_NO_NATIVE === '1' ? ['tests/native/**', '**/node_modules/**'] : ['**/node_modules/**'],
    testTimeout: 10000,
    hookTimeout: 1000,
    teardownTimeout: 1000,
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      include: ['src/**/*.ts'],
      exclude: ['src/**/*.test.ts', 'src/**/*.spec.ts'],
      // Floors are measured over the full suite (native included); a no-native
      // run covers ~2% of src/ and reports coverage without gating it.
      thresholds:
        process.env.OCTOCODE_NO_NATIVE === '1'
          ? undefined
          : {
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
