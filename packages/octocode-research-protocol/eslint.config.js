import eslint from "@eslint/js";
import tseslint from "typescript-eslint";
import { importsFirstConfig } from "../../eslint.imports-first.config.mjs";

export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  importsFirstConfig,
  {
    languageOptions: { parserOptions: { tsconfigRootDir: import.meta.dirname } },
    rules: {
      "@typescript-eslint/no-unused-vars": ["error", { args: "none", varsIgnorePattern: "^_" }],
      "@typescript-eslint/explicit-function-return-type": "off",
    },
  },
  { ignores: ["dist/**", "node_modules/**"] },
);
