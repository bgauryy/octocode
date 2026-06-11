# CommonJS (.cjs)

Source sample: `cjs/apidom-babel.config.cjs`

Strategy: `terser`

Agent rating: **9.7/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 3184 | - | - | - |
| content-view | 3031 | 4.8% | 0.656 ms | 9.5/10 |
| applyMinification | 1603 | 49.7% | 9.921 ms | 9.5/10 |
| sync minify | 1603 | 49.7% | 2.229 ms | 9.5/10 |
| async minify | 1603 | 49.7% | 2.492 ms | 9.5/10 |
| symbols | 71 | 97.8% | 7.385 ms | 10/10 |

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

... [truncated 1231 chars] ...

orms: false,
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

## Apply Minification Excerpt

```js
const path=require("node:path");module.exports={babelrcRoots:["packages/*"],ignore:["**/*.d.ts"],env:{cjs:{browserslistEnv:"isomorphic-production",presets:[["@babel/preset-env",{debug:!1,modules:"commonjs",loose:!0,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:["transform-function-name"]}],["@babel/preset-typescript",{allowDeclareFields:!0}]],plugins:[["babel-plugin-transform-import-meta"],["@babel/plugin-transform-runtime",{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,version:"^7.22.15"}],"test"!==process.env.NODE_ENV&&[path.join(__dirname,"./scripts/babel-plugin-add-import-extension.cjs"),{extension:"cjs"}]].filter(Boolean)},es:{browserslistEnv:"isomorphic-production",presets:[["@babel/preset-env",{debug:!1,modules:!1,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:["transform-function-name"]}],["@babel/preset-typescript",{allowDeclareFields:!0}]],plugins:[["@babel/plugin-transform-runtime",{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,useESModules:!0,version:"^7.22.15"}],[path.join(__dirname,"./scripts/babel-plugin-add-import-extension.cjs"),{extension:"mjs"}]]},browser:{browserslistEnv:"browser-production",presets:[["@babel/preset-env",{debug:!1,modules:"auto",useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:["transform-function-name"]}],["@babel/preset-typescript",{allowDeclareFields:!0}]],plugins:[["@babel/plugin-transform-runtime",{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,version:"^7.22.15"}]]}}};
```

## Sync Minify Excerpt

```js
const path=require("node:path");module.exports={babelrcRoots:["packages/*"],ignore:["**/*.d.ts"],env:{cjs:{browserslistEnv:"isomorphic-production",presets:[["@babel/preset-env",{debug:!1,modules:"commonjs",loose:!0,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:["transform-function-name"]}],["@babel/preset-typescript",{allowDeclareFields:!0}]],plugins:[["babel-plugin-transform-import-meta"],["@babel/plugin-transform-runtime",{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,version:"^7.22.15"}],"test"!==process.env.NODE_ENV&&[path.join(__dirname,"./scripts/babel-plugin-add-import-extension.cjs"),{extension:"cjs"}]].filter(Boolean)},es:{browserslistEnv:"isomorphic-production",presets:[["@babel/preset-env",{debug:!1,modules:!1,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:["transform-function-name"]}],["@babel/preset-typescript",{allowDeclareFields:!0}]],plugins:[["@babel/plugin-transform-runtime",{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,useESModules:!0,version:"^7.22.15"}],[path.join(__dirname,"./scripts/babel-plugin-add-import-extension.cjs"),{extension:"mjs"}]]},browser:{browserslistEnv:"browser-production",presets:[["@babel/preset-env",{debug:!1,modules:"auto",useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:["transform-function-name"]}],["@babel/preset-typescript",{allowDeclareFields:!0}]],plugins:[["@babel/plugin-transform-runtime",{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,version:"^7.22.15"}]]}}};
```

## Async Minify Excerpt

```js
const path=require("node:path");module.exports={babelrcRoots:["packages/*"],ignore:["**/*.d.ts"],env:{cjs:{browserslistEnv:"isomorphic-production",presets:[["@babel/preset-env",{debug:!1,modules:"commonjs",loose:!0,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:["transform-function-name"]}],["@babel/preset-typescript",{allowDeclareFields:!0}]],plugins:[["babel-plugin-transform-import-meta"],["@babel/plugin-transform-runtime",{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,version:"^7.22.15"}],"test"!==process.env.NODE_ENV&&[path.join(__dirname,"./scripts/babel-plugin-add-import-extension.cjs"),{extension:"cjs"}]].filter(Boolean)},es:{browserslistEnv:"isomorphic-production",presets:[["@babel/preset-env",{debug:!1,modules:!1,useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:["transform-function-name"]}],["@babel/preset-typescript",{allowDeclareFields:!0}]],plugins:[["@babel/plugin-transform-runtime",{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,useESModules:!0,version:"^7.22.15"}],[path.join(__dirname,"./scripts/babel-plugin-add-import-extension.cjs"),{extension:"mjs"}]]},browser:{browserslistEnv:"browser-production",presets:[["@babel/preset-env",{debug:!1,modules:"auto",useBuiltIns:!1,forceAllTransforms:!1,ignoreBrowserslistConfig:!1,exclude:["transform-function-name"]}],["@babel/preset-typescript",{allowDeclareFields:!0}]],plugins:[["@babel/plugin-transform-runtime",{corejs:{version:3,proposals:!1},absoluteRuntime:!1,helpers:!0,regenerator:!1,version:"^7.22.15"}]]}}};
```

## Symbols

```txt
  1| const path = require('node:path');
  3| module.exports = {
123| };
```
