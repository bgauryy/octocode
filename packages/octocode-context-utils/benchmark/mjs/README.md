# ESM JavaScript (.mjs)

Source sample: `mjs/llhttp-eslint.config.mjs`

Strategy: `terser`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 1259 | - | - |
| content-view | 866 | 31.2% | 0.055 ms |
| applyMinification | 866 | 31.2% | 0.018 ms |
| sync minify | 866 | 31.2% | 0.01 ms |
| async minify | 866 | 31.2% | 0.013 ms |
| symbols | 1443 | -14.6% | 1.194 ms |

## Notes

- engine-backed or parser-backed path.

## Before Excerpt

```js
// @ts-check

import eslint from "@eslint/js";
import tseslint from "typescript-eslint";
import stylistic from "@stylistic/eslint-plugin";

export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    ignores: ["build", "lib"],
  },
  {
    files: [
      "bin/**/*.ts",
      "bench/**/*.ts",
      "src/**/*.ts",
      "scripts/**/*.ts",
      "test/**/*.ts",
      "eslint.config.js",
    ],
    plugins: {
      "@stylistic": stylistic,
    },
    languageOptions: {
      parser: tseslint.parser,
      parserOptions: {
        project: "./tsconfig.test.json",
      },
    },
    rules: {
      "@stylistic/max-len": [
        2,
        {
          code: 120,
          ignoreComments: true,
        },
      ],
      "@stylistic/array-bracket-spacing": ["error", "always"],
      "@stylistic/operator-linebreak": ["error", "after"],
      "@stylistic/linebreak-style": ["error", "unix"],
      "@stylistic/brace-style": ["error", "1tbs", { allowSingleLine: true }],
      "@stylistic/indent": [
        "error",
        2,
        {
          SwitchCase: 1,
          FunctionDeclaration: { parameters: "first" },
          FunctionExpression: { parameters: "first" },
        },
      ],
    },
  }
);

```

## Content-View Excerpt

```js
import eslint from"@eslint/js";import tseslint from"typescript-eslint";import stylistic from"@stylistic/eslint-plugin";export default tseslint.config(eslint.configs.recommended,...tseslint.configs.recommended,{ignores:[`build`,`lib`]},{files:[`bin/**/*.ts`,`bench/**/*.ts`,`src/**/*.ts`,`scripts/**/*.ts`,`test/**/*.ts`,`eslint.config.js`],plugins:{"@stylistic":stylistic},languageOptions:{parser:tseslint.parser,parserOptions:{project:`./tsconfig.test.json`}},rules:{"@stylistic/max-len":[2,{code:120,ignoreComments:!0}],"@stylistic/array-bracket-spacing":[`error`,`always`],"@stylistic/operator-linebreak":[`error`,`after`],"@stylistic/linebreak-style":[`error`,`unix`],"@stylistic/brace-style":[`error`,`1tbs`,{allowSingleLine:!0}],"@stylistic/indent":[`error`,2,{SwitchCase:1,FunctionDeclaration:{parameters:`first`},FunctionExpression:{parameters:`first`}}]}});
```

## Apply Minification Excerpt

```js
import eslint from"@eslint/js";import tseslint from"typescript-eslint";import stylistic from"@stylistic/eslint-plugin";export default tseslint.config(eslint.configs.recommended,...tseslint.configs.recommended,{ignores:[`build`,`lib`]},{files:[`bin/**/*.ts`,`bench/**/*.ts`,`src/**/*.ts`,`scripts/**/*.ts`,`test/**/*.ts`,`eslint.config.js`],plugins:{"@stylistic":stylistic},languageOptions:{parser:tseslint.parser,parserOptions:{project:`./tsconfig.test.json`}},rules:{"@stylistic/max-len":[2,{code:120,ignoreComments:!0}],"@stylistic/array-bracket-spacing":[`error`,`always`],"@stylistic/operator-linebreak":[`error`,`after`],"@stylistic/linebreak-style":[`error`,`unix`],"@stylistic/brace-style":[`error`,`1tbs`,{allowSingleLine:!0}],"@stylistic/indent":[`error`,2,{SwitchCase:1,FunctionDeclaration:{parameters:`first`},FunctionExpression:{parameters:`first`}}]}});
```

## Sync Minify Excerpt

```js
import eslint from"@eslint/js";import tseslint from"typescript-eslint";import stylistic from"@stylistic/eslint-plugin";export default tseslint.config(eslint.configs.recommended,...tseslint.configs.recommended,{ignores:[`build`,`lib`]},{files:[`bin/**/*.ts`,`bench/**/*.ts`,`src/**/*.ts`,`scripts/**/*.ts`,`test/**/*.ts`,`eslint.config.js`],plugins:{"@stylistic":stylistic},languageOptions:{parser:tseslint.parser,parserOptions:{project:`./tsconfig.test.json`}},rules:{"@stylistic/max-len":[2,{code:120,ignoreComments:!0}],"@stylistic/array-bracket-spacing":[`error`,`always`],"@stylistic/operator-linebreak":[`error`,`after`],"@stylistic/linebreak-style":[`error`,`unix`],"@stylistic/brace-style":[`error`,`1tbs`,{allowSingleLine:!0}],"@stylistic/indent":[`error`,2,{SwitchCase:1,FunctionDeclaration:{parameters:`first`},FunctionExpression:{parameters:`first`}}]}});
```

## Async Minify Excerpt

```js
import eslint from"@eslint/js";import tseslint from"typescript-eslint";import stylistic from"@stylistic/eslint-plugin";export default tseslint.config(eslint.configs.recommended,...tseslint.configs.recommended,{ignores:[`build`,`lib`]},{files:[`bin/**/*.ts`,`bench/**/*.ts`,`src/**/*.ts`,`scripts/**/*.ts`,`test/**/*.ts`,`eslint.config.js`],plugins:{"@stylistic":stylistic},languageOptions:{parser:tseslint.parser,parserOptions:{project:`./tsconfig.test.json`}},rules:{"@stylistic/max-len":[2,{code:120,ignoreComments:!0}],"@stylistic/array-bracket-spacing":[`error`,`always`],"@stylistic/operator-linebreak":[`error`,`after`],"@stylistic/linebreak-style":[`error`,`unix`],"@stylistic/brace-style":[`error`,`1tbs`,{allowSingleLine:!0}],"@stylistic/indent":[`error`,2,{SwitchCase:1,FunctionDeclaration:{parameters:`first`},FunctionExpression:{parameters:`first`}}]}});
```

## Symbols

```txt
 3| import eslint from "@eslint/js";
 4| import tseslint from "typescript-eslint";
 5| import stylistic from "@stylistic/eslint-plugin";
 7| export default tseslint.config(
 8|   eslint.configs.recommended,
 9|   ...tseslint.configs.recommended,
10|   {
11|     ignores: ["build", "lib"],
12|   },
13|   {
14|     files: [
15|       "bin/**/*.ts",
16|       "bench/**/*.ts",
17|       "src/**/*.ts",
18|       "scripts/**/*.ts",
19|       "test/**/*.ts",
20|       "eslint.config.js",
21|     ],
22|     plugins: {
23|       "@stylistic": stylistic,
24|     },
25|     languageOptions: {
26|       parser: tseslint.parser,
27|       parserOptions: {
28|         project: "./tsconfig.test.json",
29|       },
30|     },
31|     rules: {
32|       "@stylistic/max-len": [
33|         2,
34|         {
35|           code: 120,
36|           ignoreComments: true,
37|         },
38|       ],
39|       "@stylistic/array-bracket-spacing": ["error", "always"],
40|       "@stylistic/operator-linebreak": ["error", "after"],
41|       "@stylistic/linebreak-style": ["error", "unix"],
42|       "@stylistic/brace-style": ["error", "1tbs", { allowSingleLine: true }],
43|       "@stylistic/indent": [
44|         "error",
45|         2,
46|         {
47|           SwitchCase: 1,
48|           FunctionDeclaration: { parameters: "first" },
49|           FunctionExpression: { parameters: "first" },
50|         },
51|       ],
52|     },
53|   }
54| );
```
