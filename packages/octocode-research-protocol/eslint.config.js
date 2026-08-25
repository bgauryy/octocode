import eslint from "@eslint/js";
import tseslint from "typescript-eslint";

// Deliberately lighter than the monorepo root config: no prettier
// enforcement, so this package keeps its own established formatting
// (double-quoted strings, 2-space indent) instead of being reformatted to
// match root's prettier config. This is the only package with its own
// eslint.config.js for that reason.
export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    languageOptions: { parserOptions: { tsconfigRootDir: import.meta.dirname } },
    rules: {
      "@typescript-eslint/no-unused-vars": ["error", { args: "none", varsIgnorePattern: "^_" }],
      "@typescript-eslint/explicit-function-return-type": "off",
    },
  },
  { ignores: ["dist/**", "node_modules/**"] },
);
