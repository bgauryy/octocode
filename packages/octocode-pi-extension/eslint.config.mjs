import tseslint from 'typescript-eslint';

// Keep modules small: split a file before it outgrows these limits.
export default tseslint.config(
  { ignores: ['dist/**', 'coverage/**', 'node_modules/**', 'themes/**', 'subagents/**'] },
  {
    files: ['src/**/*.ts', 'tests/**/*.ts'],
    languageOptions: { parser: tseslint.parser },
    plugins: { '@typescript-eslint': tseslint.plugin },
  },
  {
    files: ['src/**/*.ts'],
    rules: {
      'max-lines': ['error', { max: 400 }],
      // Long prose (strings, template literals, comments, URLs) may run on; code may not.
      'max-len': ['error', { code: 200, ignoreStrings: true, ignoreTemplateLiterals: true, ignoreComments: true, ignoreUrls: true, ignoreRegExpLiterals: true }],
    },
  },
  { files: ['tests/**/*.ts'], rules: { 'max-lines': ['error', { max: 600 }] } },
);
