import { defineConfig } from 'vitest/config';
import { readFileSync } from 'fs';
import { resolve } from 'path';

export default defineConfig({
  build: { sourcemap: false },
  server: { sourcemapIgnoreList: () => true },
  resolve: {
    alias: [
      {
        find: /^@octocodeai\/octocode-tools-core$/,
        replacement: resolve(
          import.meta.dirname,
          '../octocode-tools-core/src/index.ts'
        ),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/direct$/,
        replacement: resolve(
          import.meta.dirname,
          '../octocode-tools-core/src/direct.ts'
        ),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/zod$/,
        replacement: resolve(
          import.meta.dirname,
          '../octocode-tools-core/src/zod.ts'
        ),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/config$/,
        replacement: resolve(
          import.meta.dirname,
          '../octocode-tools-core/src/shared/config/index.ts'
        ),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/credentials$/,
        replacement: resolve(
          import.meta.dirname,
          '../octocode-tools-core/src/shared/credentials/index.ts'
        ),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/platform$/,
        replacement: resolve(
          import.meta.dirname,
          '../octocode-tools-core/src/shared/platform/index.ts'
        ),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/session$/,
        replacement: resolve(
          import.meta.dirname,
          '../octocode-tools-core/src/shared/session/index.ts'
        ),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/paths$/,
        replacement: resolve(
          import.meta.dirname,
          '../octocode-tools-core/src/shared/paths.ts'
        ),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/fs-utils$/,
        replacement: resolve(
          import.meta.dirname,
          '../octocode-tools-core/src/shared/fs-utils.ts'
        ),
      },
      {
        find: /^@octocodeai\/octocode-tools-core\/testing$/,
        replacement: resolve(
          import.meta.dirname,
          '../octocode-tools-core/src/shared/credentials/testing.ts'
        ),
      },
      {
        find: /^octocode-security$/,
        replacement: resolve(
          import.meta.dirname,
          '../octocode-engine/dist/security/index.js'
        ),
      },
      {
        find: /^octocode-security\/(.+)$/,
        replacement: resolve(
          import.meta.dirname,
          '../octocode-engine/dist/security/$1.js'
        ),
      },
    ],
  },
  test: {
    environment: 'node',
    globals: true,
    include: ['tests/**/*.test.ts'],
    testTimeout: 10000,
    hookTimeout: 1000,
    teardownTimeout: 1000,
    setupFiles: ['./tests/setup.ts'],
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
