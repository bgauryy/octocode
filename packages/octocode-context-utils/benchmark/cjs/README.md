# CommonJS (.cjs)

Source sample: `cjs/apidom-babel.config.cjs`

Strategy: `terser`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 3184 | - | - |
| content-view | 1605 | 49.6% | 0.438 ms |
| applyMinification | 1605 | 49.6% | 0.112 ms |
| sync minify | 1605 | 49.6% | 0.04 ms |
| async minify | 1605 | 49.6% | 0.041 ms |
| symbols | 3792 | -19.1% | 1.698 ms |

## Notes

- engine-backed or parser-backed path.

## Before Excerpt

```js
const path = require('node:path');

module.exports = {
  babelrcRoots: ['packages/*'],
  ignore: ['**/*.d.ts'],
  env: {
    cjs: {
      browserslistEnv: 'isomorphic-production',
      presets: [
        [
          '@babel/preset-env',
          {
            debug: false,
            modules: 'commonjs',
            loose: true,
            useBuiltIns: false,
            forceAllTransforms: false,
            ignoreBrowserslistConfig: false,
            exclude: ['transform-function-name'],
          },
        ],
        [
          '@babel/preset-typescript',
          {
            allowDeclareFields: true,
          },
        ],
      ],
      plugins: [
        ['babel-plugin-transform-import-meta'],
        [
          '@babel/plugin-transform-runtime',
          {
            corejs: { version: 3, proposals: false },
            absoluteRuntime: false,
            helpers: true,
            regenerator: false,
            version: '^7.22.15',
          },
        ],
        process.env.NODE_ENV !== 'test'
          ? [
              path.join(__dirname, './scripts/babel-plugin-add-import-extension.cjs'),
              { extension: 'cjs' },
            ]
          : false,
      ].filter(Boolea

... [truncated 1384 chars] ...

: ['transform-function-name'], // this is here because of https://github.com/babel/babel/discussions/12874
          },
        ],
        [
          '@babel/preset-typescript',
          {
            allowDeclareFields: true,
          },
        ],
      ],
      plugins: [
        [
          '@babel/plugin-transform-runtime',
          {
            corejs: { version: 3, proposals: false },
            absoluteRuntime: false,
            helpers: true,
            regenerator: false,
            version: '^7.22.15',
          },
        ],
      ],
    },
  },
};

```

## Content-View Excerpt

```js
const path=require("node:path");module.exports={babelrcRoots:[`packages/*`],ignore:[`**/*.d.ts`],env:{cjs:{browserslistEnv:`isomorphic-production`,presets:[[`@babel/preset-env`,{debug:!1,modules:`commonjs`,loose:!0,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:[`transform-function-name`]}],[`@babel/preset-typescript`,{allowDeclareFields:!0}]],plugins:[[`babel-plugin-transform-import-meta`],[`@babel/plugin-transform-runtime`,{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,version:`^7.22.15`}],process.env.NODE_ENV===`test`?!1:[path.join(__dirname,`./scripts/babel-plugin-add-import-extension.cjs`),{extension:`cjs`}]].filter(Boolean)},es:{browserslistEnv:`isomorphic-production`,presets:[[`@babel/preset-env`,{debug:!1,modules:!1,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:[`transform-function-name`]}],[`@babel/preset-typescript`,{allowDeclareFields:!0}]],plugins:[[`@babel/plugin-transform-runtime`,{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,useESModules:!0,version:`^7.22.15`}],[path.join(__dirname,`./scripts/babel-plugin-add-import-extension.cjs`),{extension:`mjs`}]]},browser:{browserslistEnv:`browser-production`,presets:[[`@babel/preset-env`,{debug:!1,modules:`auto`,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:[`transform-function-name`]}],[`@babel/preset-typescript`,{allowDeclareFields:!0}]],plugins:[[`@babel/plugin-transform-runtime`,{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,version:`^7.22.15`}]]}}};
```

## Apply Minification Excerpt

```js
const path=require("node:path");module.exports={babelrcRoots:[`packages/*`],ignore:[`**/*.d.ts`],env:{cjs:{browserslistEnv:`isomorphic-production`,presets:[[`@babel/preset-env`,{debug:!1,modules:`commonjs`,loose:!0,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:[`transform-function-name`]}],[`@babel/preset-typescript`,{allowDeclareFields:!0}]],plugins:[[`babel-plugin-transform-import-meta`],[`@babel/plugin-transform-runtime`,{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,version:`^7.22.15`}],process.env.NODE_ENV===`test`?!1:[path.join(__dirname,`./scripts/babel-plugin-add-import-extension.cjs`),{extension:`cjs`}]].filter(Boolean)},es:{browserslistEnv:`isomorphic-production`,presets:[[`@babel/preset-env`,{debug:!1,modules:!1,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:[`transform-function-name`]}],[`@babel/preset-typescript`,{allowDeclareFields:!0}]],plugins:[[`@babel/plugin-transform-runtime`,{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,useESModules:!0,version:`^7.22.15`}],[path.join(__dirname,`./scripts/babel-plugin-add-import-extension.cjs`),{extension:`mjs`}]]},browser:{browserslistEnv:`browser-production`,presets:[[`@babel/preset-env`,{debug:!1,modules:`auto`,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:[`transform-function-name`]}],[`@babel/preset-typescript`,{allowDeclareFields:!0}]],plugins:[[`@babel/plugin-transform-runtime`,{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,version:`^7.22.15`}]]}}};
```

## Sync Minify Excerpt

```js
const path=require("node:path");module.exports={babelrcRoots:[`packages/*`],ignore:[`**/*.d.ts`],env:{cjs:{browserslistEnv:`isomorphic-production`,presets:[[`@babel/preset-env`,{debug:!1,modules:`commonjs`,loose:!0,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:[`transform-function-name`]}],[`@babel/preset-typescript`,{allowDeclareFields:!0}]],plugins:[[`babel-plugin-transform-import-meta`],[`@babel/plugin-transform-runtime`,{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,version:`^7.22.15`}],process.env.NODE_ENV===`test`?!1:[path.join(__dirname,`./scripts/babel-plugin-add-import-extension.cjs`),{extension:`cjs`}]].filter(Boolean)},es:{browserslistEnv:`isomorphic-production`,presets:[[`@babel/preset-env`,{debug:!1,modules:!1,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:[`transform-function-name`]}],[`@babel/preset-typescript`,{allowDeclareFields:!0}]],plugins:[[`@babel/plugin-transform-runtime`,{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,useESModules:!0,version:`^7.22.15`}],[path.join(__dirname,`./scripts/babel-plugin-add-import-extension.cjs`),{extension:`mjs`}]]},browser:{browserslistEnv:`browser-production`,presets:[[`@babel/preset-env`,{debug:!1,modules:`auto`,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:[`transform-function-name`]}],[`@babel/preset-typescript`,{allowDeclareFields:!0}]],plugins:[[`@babel/plugin-transform-runtime`,{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,version:`^7.22.15`}]]}}};
```

## Async Minify Excerpt

```js
const path=require("node:path");module.exports={babelrcRoots:[`packages/*`],ignore:[`**/*.d.ts`],env:{cjs:{browserslistEnv:`isomorphic-production`,presets:[[`@babel/preset-env`,{debug:!1,modules:`commonjs`,loose:!0,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:[`transform-function-name`]}],[`@babel/preset-typescript`,{allowDeclareFields:!0}]],plugins:[[`babel-plugin-transform-import-meta`],[`@babel/plugin-transform-runtime`,{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,version:`^7.22.15`}],process.env.NODE_ENV===`test`?!1:[path.join(__dirname,`./scripts/babel-plugin-add-import-extension.cjs`),{extension:`cjs`}]].filter(Boolean)},es:{browserslistEnv:`isomorphic-production`,presets:[[`@babel/preset-env`,{debug:!1,modules:!1,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:[`transform-function-name`]}],[`@babel/preset-typescript`,{allowDeclareFields:!0}]],plugins:[[`@babel/plugin-transform-runtime`,{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,useESModules:!0,version:`^7.22.15`}],[path.join(__dirname,`./scripts/babel-plugin-add-import-extension.cjs`),{extension:`mjs`}]]},browser:{browserslistEnv:`browser-production`,presets:[[`@babel/preset-env`,{debug:!1,modules:`auto`,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:[`transform-function-name`]}],[`@babel/preset-typescript`,{allowDeclareFields:!0}]],plugins:[[`@babel/plugin-transform-runtime`,{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,version:`^7.22.15`}]]}}};
```

## Symbols

```txt
  1| const path = require('node:path');
  3| module.exports = {
  4|   babelrcRoots: ['packages/*'],
  5|   ignore: ['**/*.d.ts'],
  6|   env: {
  7|     cjs: {
  8|       browserslistEnv: 'isomorphic-production',
  9|       presets: [
 10|         [
 11|           '@babel/preset-env',
 12|           {
 13|             debug: false,
 14|             modules: 'commonjs',
 15|             loose: true,
 16|             useBuiltIns: false,
 17|             forceAllTransforms: false,
 18|             ignoreBrowserslistConfig: false,
 19|             exclude: ['transform-function-name'],
 20|           },
 21|         ],
 22|         [
 23|           '@babel/preset-typescript',
 24|           {
 25|             allowDeclareFields: true,
 26|           },
 27|         ],
 28|       ],
 29|       plugins: [
 30|         ['babel-plugin-transform-import-meta'],
 31|         [
 32|           '@babel/plugin-transform-runtime',
 33|           {
 34|             corejs: { version: 3, proposals: false },
 35|             absoluteRuntime: false,
 36|             helpers: true,
 37|             regenerator: false,
 38|             version: '^7.22.15',
 39|           },
 40|         ],
 41|         process.env.NODE_ENV !== 'test'
 42|           ? [
 43|               path.join(__dirname, './scripts/babel-plugin-add-import-extension.cjs'),
 44|               { extension: 'cjs' },
 45|             ]
 46|           : false,
 47|       ].filter(Boolean),
 48|     },
 49|     es: {
 50|       browserslistEnv: 'isomorphic-production',
 51|       presets: [
 52|         [
 53|           '@babel/preset-env',
 54|           {
 55|             debug: false,
 56|             modules: false,
 57|             useBuiltIns: false,
 58|             forceAllTransforms: fal

... [truncated 1192 chars] ...

seBuiltIns: false,
 97|             forceAllTransforms: false,
 98|             ignoreBrowserslistConfig: false,
 99|             exclude: ['transform-function-name'], // this is here because of https://github.com/babel/babel/discussions/12874
100|           },
101|         ],
102|         [
103|           '@babel/preset-typescript',
104|           {
105|             allowDeclareFields: true,
106|           },
107|         ],
108|       ],
109|       plugins: [
110|         [
111|           '@babel/plugin-transform-runtime',
112|           {
113|             corejs: { version: 3, proposals: false },
114|             absoluteRuntime: false,
115|             helpers: true,
116|             regenerator: false,
117|             version: '^7.22.15',
118|           },
119|         ],
120|       ],
121|     },
122|   },
123| };
```
