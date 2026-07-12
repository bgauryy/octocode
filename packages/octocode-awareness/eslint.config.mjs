import tseslint from 'typescript-eslint';

const MAX_FILE_LINES = 400;

export default tseslint.config(
  {
    ignores: ['coverage/**', 'out/**', '.out-build-*/**', '.out-backup-*/**', 'dist/**', 'node_modules/**'],
  },
  {
    files: ['**/*.ts', '**/*.mjs'],
    languageOptions: {
      parser: tseslint.parser,
    },
    rules: {
      'max-lines': ['error', { max: MAX_FILE_LINES }],
    },
  },
);
