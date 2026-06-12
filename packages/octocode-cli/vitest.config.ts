import { defineConfig } from 'vitest/config';
import { resolve } from 'path';
import { fileURLToPath } from 'url';

const __dirname = fileURLToPath(new URL('.', import.meta.url));

const securityMock = resolve(__dirname, 'tests/__mocks__/octocode-security.ts');

export default defineConfig({
  resolve: {
    alias: {
      // Redirect every octocode-security import to the test stub so
      // vitest never tries to dlopen the native Rust binary.
      'octocode-security': securityMock,
      'octocode-security/mask': securityMock,
      'octocode-security/contentSanitizer': securityMock,
      'octocode-security/pathValidator': securityMock,
      'octocode-security/commandValidator': securityMock,
    },
  },
  test: {
    globals: true,
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    setupFiles: ['tests/setup.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      include: ['src/**/*.ts'],
      exclude: [
        'src/index.ts',
        'src/types/**',
        'src/cli/types.ts',
        'src/ui/**',
        'src/prompts.ts',
        'src/spinner.ts',
        'src/cli/commands.ts',
        'src/cli/help.ts',
        'src/cli/index.ts',
        'src/configs/**',
        'src/features/github-oauth.ts',
        'src/utils/token-storage.ts',
      ],
      thresholds: {
        statements: 95,
        branches: 85,
        functions: 95,
        lines: 95,
      },
    },
    testTimeout: 30000,
    hookTimeout: 15000,
    restoreMocks: true,
    clearMocks: true,
  },
});
