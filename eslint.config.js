import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';
import eslintPluginPrettier from 'eslint-plugin-prettier/recommended';

export default tseslint.config(
  // Global ignores
  {
    ignores: [
      '**/build/**',
      '**/dist/**',
      '**/out/**',
      '**/node_modules/**',
      '**/coverage/**',
      '**/*.js',
      '**/*.mjs',
      '**/*.d.ts',
      '**/examples/**',
      // Skills have their own eslint configs
      'skills/**',
    ],
  },

  // Base config for all TypeScript files
  eslint.configs.recommended,
  ...tseslint.configs.recommended,

  // Prettier integration
  eslintPluginPrettier,

  // Common rules for all packages
  {
    languageOptions: {
      ecmaVersion: 2022,
      sourceType: 'module',
      parserOptions: {
        project: null,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      // Allow unused vars/types that start with _ or are exported (may be used externally)
      // Downgraded to warn since many exported types are used by external consumers
      '@typescript-eslint/no-unused-vars': [
        'warn',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          ignoreRestSiblings: true,
        },
      ],
      '@typescript-eslint/no-explicit-any': 'warn',
      '@typescript-eslint/no-require-imports': 'off',
      '@typescript-eslint/ban-ts-comment': 'off',
      '@typescript-eslint/no-non-null-asserted-optional-chain': 'warn',
      '@typescript-eslint/no-unsafe-function-type': 'warn',
      'prefer-const': 'error',
      'no-var': 'error',
      'no-unused-vars': 'off', // Use TypeScript version instead
    },
  },

  // octocode-mcp specific rules
  {
    files: ['packages/octocode-mcp/src/**/*.ts', 'packages/octocode-mcp/tests/**/*.ts'],
    rules: {
      '@typescript-eslint/no-explicit-any': 'error',
      'no-console': 'error',
      'prettier/prettier': 'error',
    },
  },

  // octocode-cli specific rules
  {
    files: ['packages/octocode-cli/src/**/*.ts', 'packages/octocode-cli/tests/**/*.ts'],
    rules: {
      'no-console': 'off',
      'no-control-regex': 'off',
    },
  },

  // octocode-cli tests - allow any
  {
    files: ['packages/octocode-cli/tests/**/*.ts'],
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
    },
  },

  // octocode-vscode specific rules
  {
    files: ['packages/octocode-vscode/src/**/*.ts'],
    rules: {
      'no-console': 'off',
      semi: ['error', 'always'],
      quotes: ['error', 'single', { avoidEscape: true }],
    },
  },

  // octocode-shared specific rules
  {
    files: ['packages/octocode-shared/src/**/*.ts', 'packages/octocode-shared/tests/**/*.ts'],
    rules: {
      '@typescript-eslint/explicit-function-return-type': 'off',
      'no-console': 'off',
    },
  },

  // octocode-security-utils specific rules
  {
    files: [
      'packages/octocode-security-utils/src/**/*.ts',
      'packages/octocode-security-utils/tests/**/*.ts',
    ],
    rules: {
      '@typescript-eslint/no-explicit-any': 'error',
      'no-console': 'error',
    },
  },

  // Test files - add globals for vitest and relax rules
  {
    files: ['**/tests/**/*.ts', '**/tests/**/*.tsx'],
    languageOptions: {
      globals: {
        vi: 'readonly',
        describe: 'readonly',
        it: 'readonly',
        expect: 'readonly',
        beforeEach: 'readonly',
        afterEach: 'readonly',
        beforeAll: 'readonly',
        afterAll: 'readonly',
      },
    },
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unused-vars': 'off',
      '@typescript-eslint/no-unsafe-function-type': 'off',
      'no-console': 'off',
    },
  }
);
