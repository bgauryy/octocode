# ESM JavaScript (.mjs)

Source sample: `mjs/llhttp-eslint.config.mjs`

Strategy: `terser`

Agent rating: **7.6/10 (good)**

Agent understanding from minified output: **9.9/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 1259 | - | - | - |
| content-view | 879 | 30.2% | 0.312 ms | 9/10 |
| applyMinification | 879 | 30.2% | 0.221 ms | 9/10 |
| sync minify | 879 | 30.2% | 0.171 ms | 9/10 |
| async minify | 879 | 30.2% | 0.184 ms | 9/10 |
| symbols | 1443 | -14.6% | 4.157 ms | 5/10 |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 10/10 |
| context budget | 9/10 |
| symbol context | 10/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 1259 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 879 | 30.2% | 9.9/10 excellent | 10/10 | 10/10 |
| minify | 879 | 30.2% | 9.9/10 excellent | 10/10 | 10/10 |
| symbols | 1443 | -14.6% | 9/10 excellent | 10/10 | 10/10 |

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
// @ts-check
import eslint from"@eslint/js";import tseslint from"typescript-eslint";import stylistic from"@stylistic/eslint-plugin";export default tseslint.config(eslint.configs.recommended,...tseslint.configs.recommended,{ignores:[`build`,`lib`]},{files:[`bin/**/*.ts`,`bench/**/*.ts`,`src/**/*.ts`,`scripts/**/*.ts`,`test/**/*.ts`,`eslint.config.js`],plugins:{"@stylistic":stylistic},languageOptions:{parser:tseslint.parser,parserOptions:{project:`./tsconfig.test.json`}},rules:{"@stylistic/max-len":[2,{code:120,ignoreComments:!0}],"@stylistic/array-bracket-spacing":[`error`,`always`],"@stylistic/operator-linebreak":[`error`,`after`],"@stylistic/linebreak-style":[`error`,`unix`],"@stylistic/brace-style":[`error`,`1tbs`,{allowSingleLine:!0}],"@stylistic/indent":[`error`,2,{SwitchCase:1,FunctionDeclaration:{parameters:`first`},FunctionExpression:{parameters:`first`}}]}});
```

## Apply Minification Excerpt

```js
// @ts-check
import eslint from"@eslint/js";import tseslint from"typescript-eslint";import stylistic from"@stylistic/eslint-plugin";export default tseslint.config(eslint.configs.recommended,...tseslint.configs.recommended,{ignores:[`build`,`lib`]},{files:[`bin/**/*.ts`,`bench/**/*.ts`,`src/**/*.ts`,`scripts/**/*.ts`,`test/**/*.ts`,`eslint.config.js`],plugins:{"@stylistic":stylistic},languageOptions:{parser:tseslint.parser,parserOptions:{project:`./tsconfig.test.json`}},rules:{"@stylistic/max-len":[2,{code:120,ignoreComments:!0}],"@stylistic/array-bracket-spacing":[`error`,`always`],"@stylistic/operator-linebreak":[`error`,`after`],"@stylistic/linebreak-style":[`error`,`unix`],"@stylistic/brace-style":[`error`,`1tbs`,{allowSingleLine:!0}],"@stylistic/indent":[`error`,2,{SwitchCase:1,FunctionDeclaration:{parameters:`first`},FunctionExpression:{parameters:`first`}}]}});
```

## Sync Minify Excerpt

```js
// @ts-check
import eslint from"@eslint/js";import tseslint from"typescript-eslint";import stylistic from"@stylistic/eslint-plugin";export default tseslint.config(eslint.configs.recommended,...tseslint.configs.recommended,{ignores:[`build`,`lib`]},{files:[`bin/**/*.ts`,`bench/**/*.ts`,`src/**/*.ts`,`scripts/**/*.ts`,`test/**/*.ts`,`eslint.config.js`],plugins:{"@stylistic":stylistic},languageOptions:{parser:tseslint.parser,parserOptions:{project:`./tsconfig.test.json`}},rules:{"@stylistic/max-len":[2,{code:120,ignoreComments:!0}],"@stylistic/array-bracket-spacing":[`error`,`always`],"@stylistic/operator-linebreak":[`error`,`after`],"@stylistic/linebreak-style":[`error`,`unix`],"@stylistic/brace-style":[`error`,`1tbs`,{allowSingleLine:!0}],"@stylistic/indent":[`error`,2,{SwitchCase:1,FunctionDeclaration:{parameters:`first`},FunctionExpression:{parameters:`first`}}]}});
```

## Async Minify Excerpt

```js
// @ts-check
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
