# ESM JavaScript (.mjs)

Source sample: `mjs/llhttp-eslint.config.mjs`

Strategy: `terser`

Agent rating: **9/10 (excellent)**

Agent understanding from minified output: **9.6/10 (excellent)**

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
| content-view | 1240 | 1.5% | 0.162 ms | 9/10 |
| applyMinification | 866 | 31.2% | 0.857 ms | 9/10 |
| sync minify | 866 | 31.2% | 0.875 ms | 9/10 |
| async minify | 866 | 31.2% | 3.933 ms | 9/10 |
| symbols | 268 | 78.7% | 4.524 ms | 9/10 |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 10/10 |
| context budget | 6/10 |
| symbol context | 10/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 1259 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 1240 | 1.5% | 9.6/10 excellent | 10/10 | 10/10 |
| minify | 866 | 31.2% | 9.9/10 excellent | 10/10 | 10/10 |
| symbols | 268 | 78.7% | 9/10 excellent | 10/10 | 5/10 |

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

## Apply Minification Excerpt

```js
import eslint from"@eslint/js";import tseslint from"typescript-eslint";import stylistic from"@stylistic/eslint-plugin";export default tseslint.config(eslint.configs.recommended,...tseslint.configs.recommended,{ignores:["build","lib"]},{files:["bin/**/*.ts","bench/**/*.ts","src/**/*.ts","scripts/**/*.ts","test/**/*.ts","eslint.config.js"],plugins:{"@stylistic":stylistic},languageOptions:{parser:tseslint.parser,parserOptions:{project:"./tsconfig.test.json"}},rules:{"@stylistic/max-len":[2,{code:120,ignoreComments:!0}],"@stylistic/array-bracket-spacing":["error","always"],"@stylistic/operator-linebreak":["error","after"],"@stylistic/linebreak-style":["error","unix"],"@stylistic/brace-style":["error","1tbs",{allowSingleLine:!0}],"@stylistic/indent":["error",2,{SwitchCase:1,FunctionDeclaration:{parameters:"first"},FunctionExpression:{parameters:"first"}}]}});
```

## Sync Minify Excerpt

```js
import eslint from"@eslint/js";import tseslint from"typescript-eslint";import stylistic from"@stylistic/eslint-plugin";export default tseslint.config(eslint.configs.recommended,...tseslint.configs.recommended,{ignores:["build","lib"]},{files:["bin/**/*.ts","bench/**/*.ts","src/**/*.ts","scripts/**/*.ts","test/**/*.ts","eslint.config.js"],plugins:{"@stylistic":stylistic},languageOptions:{parser:tseslint.parser,parserOptions:{project:"./tsconfig.test.json"}},rules:{"@stylistic/max-len":[2,{code:120,ignoreComments:!0}],"@stylistic/array-bracket-spacing":["error","always"],"@stylistic/operator-linebreak":["error","after"],"@stylistic/linebreak-style":["error","unix"],"@stylistic/brace-style":["error","1tbs",{allowSingleLine:!0}],"@stylistic/indent":["error",2,{SwitchCase:1,FunctionDeclaration:{parameters:"first"},FunctionExpression:{parameters:"first"}}]}});
```

## Async Minify Excerpt

```js
import eslint from"@eslint/js";import tseslint from"typescript-eslint";import stylistic from"@stylistic/eslint-plugin";export default tseslint.config(eslint.configs.recommended,...tseslint.configs.recommended,{ignores:["build","lib"]},{files:["bin/**/*.ts","bench/**/*.ts","src/**/*.ts","scripts/**/*.ts","test/**/*.ts","eslint.config.js"],plugins:{"@stylistic":stylistic},languageOptions:{parser:tseslint.parser,parserOptions:{project:"./tsconfig.test.json"}},rules:{"@stylistic/max-len":[2,{code:120,ignoreComments:!0}],"@stylistic/array-bracket-spacing":["error","always"],"@stylistic/operator-linebreak":["error","after"],"@stylistic/linebreak-style":["error","unix"],"@stylistic/brace-style":["error","1tbs",{allowSingleLine:!0}],"@stylistic/indent":["error",2,{SwitchCase:1,FunctionDeclaration:{parameters:"first"},FunctionExpression:{parameters:"first"}}]}});
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
13|   {
54| );
```
