import { defineConfig } from 'vitest/config';
import { readFileSync } from 'fs';
import { resolve } from 'path';

export default defineConfig({
  esbuild: { sourcemap: false },
  build: { sourcemap: false },
  server: { sourcemapIgnoreList: () => true },
  resolve: {
    alias: [
      {
        find: /^@octocodeai\/octocode-tools-core$/,
        replacement: resolve(__dirname, '../octocode-tools-core/src/index.ts'),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/direct$/,
        replacement: resolve(__dirname, '../octocode-tools-core/src/direct.ts'),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/zod$/,
        replacement: resolve(__dirname, '../octocode-tools-core/src/zod.ts'),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/config$/,
        replacement: resolve(
          __dirname,
          '../octocode-tools-core/src/shared/config/index.ts'
        ),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/credentials$/,
        replacement: resolve(
          __dirname,
          '../octocode-tools-core/src/shared/credentials/index.ts'
        ),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/platform$/,
        replacement: resolve(
          __dirname,
          '../octocode-tools-core/src/shared/platform/index.ts'
        ),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/session$/,
        replacement: resolve(
          __dirname,
          '../octocode-tools-core/src/shared/session/index.ts'
        ),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/paths$/,
        replacement: resolve(
          __dirname,
          '../octocode-tools-core/src/shared/paths.ts'
        ),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/fs-utils$/,
        replacement: resolve(
          __dirname,
          '../octocode-tools-core/src/shared/fs-utils.ts'
        ),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/testing$/,
        replacement: resolve(
          __dirname,
          '../octocode-tools-core/src/shared/credentials/testing.ts'
        ),
      },
      {
        find: /^octocode-security$/,
        replacement: resolve(
          __dirname,
          '../octocode-engine/dist/security/index.js'
        ),
      },
      {
        find: /^octocode-security\/(.+)$/,
        replacement: resolve(
          __dirname,
          '../octocode-engine/dist/security/$1.js'
        ),
      },
    ],
  },
  test: {
    environment: 'node',
    globals: true,
    include: [
      'tests/index.test.ts',
      'tests/index.shutdown.test.ts',
      'tests/serverConfig*.test.ts',
      'tests/tools/providerExecution.test.ts',
      'tests/tools/toolConfig*.test.ts',
      'tests/tools/toolFilters.test.ts',
      'tests/tools/directToolCatalog.test.ts',
      'tests/tools/all-tools.pagination-contract.test.ts',
      'tests/tools/lsp_new_public_tools.test.ts',
      'tests/tools/toolMetadata.final.test.ts',
      'tests/tools/registerBasicTool.test.ts',
      'tests/tools/registerRemoteTool*.test.ts',
      'tests/security/all-tools-sanitization.test.ts',
      'tests/scheme/**/*.test.ts',
      'tests/utils/bulkOperations.test.ts',
      'tests/flows/**/*.test.ts',
    ],
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
