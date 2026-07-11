import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';
import eslintPluginPrettier from 'eslint-plugin-prettier/recommended';

export default tseslint.config(
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
      'skills/**',
    ],
  },

  eslint.configs.recommended,
  ...tseslint.configs.recommended,

  eslintPluginPrettier,

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

  {
    files: ['packages/octocode-mcp/src/**/*.ts', 'packages/octocode-mcp/tests/**/*.ts'],
    rules: {
      '@typescript-eslint/no-explicit-any': 'error',
      'no-console': 'error',
      'prettier/prettier': 'error',
    },
  },

  {
    files: ['packages/octocode/src/**/*.ts', 'packages/octocode/tests/**/*.ts'],
    rules: {
      'no-console': 'off',
      'no-control-regex': 'off',
    },
  },

  {
    files: ['packages/octocode/src/**/*.ts'],
    rules: {
      'max-lines': ['error', { max: 400 }],
    },
  },

  {
    files: ['packages/octocode/tests/**/*.ts'],
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
    },
  },

  {
    files: ['packages/octocode-vscode/src/**/*.ts'],
    rules: {
      'no-console': 'off',
      semi: ['error', 'always'],
      quotes: ['error', 'single', { avoidEscape: true }],
    },
  },

  {
    files: ['packages/octocode-tools-core/src/shared/**/*.ts', 'packages/octocode-tools-core/tests/shared/**/*.ts'],
    rules: {
      '@typescript-eslint/explicit-function-return-type': 'off',
      'no-console': 'off',
    },
  },

  {
    files: ['packages/octocode-tools-core/src/**/*.ts'],
    rules: {
      'max-lines': ['error', { max: 400 }],
    },
  },

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
