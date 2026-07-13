import { defineConfig } from 'vitest/config';
import { resolve } from 'path';
import { fileURLToPath } from 'url';

const __dirname = fileURLToPath(new URL('.', import.meta.url));

const securityMock = resolve(
  __dirname,
  'tests/__mocks__/octocode-engine-security.ts'
);
const mcpPublicSource = resolve(__dirname, '../octocode-mcp/src/public.ts');

export default defineConfig({
  resolve: {
    alias: {
      'octocode-mcp/public': mcpPublicSource,
      // Redirect every octocode-engine security import to the test stub so
      // vitest never tries to dlopen the native Rust binary.
      '@octocodeai/octocode-engine/mask': securityMock,
      '@octocodeai/octocode-engine/contentSanitizer': securityMock,
      '@octocodeai/octocode-engine/pathValidator': securityMock,
      '@octocodeai/octocode-engine/pathUtils': securityMock,
      '@octocodeai/octocode-engine/commandValidator': securityMock,
      '@octocodeai/octocode-engine/registry': securityMock,
      '@octocodeai/octocode-engine/withSecurityValidation': securityMock,
      '@octocodeai/octocode-engine/security': securityMock,
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
      thresholds: {
        statements: 78,
        branches: 69,
        functions: 84,
        lines: 78,
      },
      exclude: [
        'src/index.ts',
        'src/types/**',
        'src/cli/types.ts',
        'src/cli/options.ts',
        'src/cli/routing.ts',
        'src/ui/**',
        'src/prompts.ts',
        'src/spinner.ts',
        'src/cli/commands.ts',
        'src/cli/help.ts',
        'src/cli/index.ts',
        'src/cli/commands/direct-tool-output.ts',
        'src/cli/commands/cat.ts',
        'src/cli/commands/ls.ts',
        'src/cli/commands/binary.ts',
        'src/cli/commands/clone.ts',
        'src/cli/commands/history.ts',
        'src/cli/commands/index.ts',
        'src/cli/commands/repo.ts',
        'src/cli/commands/symbol-outline.ts',
        'src/cli/commands/symbols.ts',
        'src/configs/**',
        'src/features/github-oauth.ts',
        'src/utils/token-storage.ts',
      ],
    },
    testTimeout: 30000,
    hookTimeout: 15000,
    restoreMocks: true,
    clearMocks: true,
  },
});
