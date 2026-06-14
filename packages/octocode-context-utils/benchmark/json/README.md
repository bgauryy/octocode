# JSON (.json)

Source sample: `json/typescript-package.json`

Strategy: `json`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 3468 | - | - |
| content-view | 3468 | 0% | 0.035 ms |
| applyMinification | 2464 | 29% | 0.022 ms |
| sync minify | 2464 | 29% | 0.019 ms |
| async minify | 2464 | 29% | 0.025 ms |
| symbols | n/a | n/a | 0.001 ms |

## Notes

- engine-backed or parser-backed path.
- content-view kept original because the readable output was not shorter.
- symbols are not implemented for this extension.

## Before Excerpt

```json
{
    "name": "typescript",
    "author": "Microsoft Corp.",
    "homepage": "https://www.typescriptlang.org/",
    "version": "6.0.0",
    "license": "Apache-2.0",
    "description": "TypeScript is a language for application scale JavaScript development",
    "keywords": [
        "TypeScript",
        "Microsoft",
        "compiler",
        "language",
        "javascript"
    ],
    "bugs": {
        "url": "https://github.com/microsoft/TypeScript/issues"
    },
    "repository": {
        "type": "git",
        "url": "https://github.com/microsoft/TypeScript.git"
    },
    "main": "./lib/typescript.js",
    "typings": "./lib/typescript.d.ts",
    "bin": {
        "tsc": "./bin/tsc",
        "tsserver": "./bin/tsserver"
    },
    "engines": {
        "node": ">=14.17"
    },
    "files": [
        "bin",
        "lib",
        "!lib/enu",
        "LICENSE.txt",
        "README.md",
        "SECURITY.md",
        "ThirdPartyNoticeText.txt",
        "!**/.gitattributes"
    ],
    "devDependencies": {
        "@dprint/formatter": "^0.4.1",
        "@dprint/typescript": "0.93.4",
        "@esfx/canceltoken": "^1.0.0",
        "@eslint/js": "^10.0.1",
        "@octokit/rest": "^22.0.1",
        "@types/

... [truncated 1668 chars] ...

sts --no-typecheck",
        "clean": "hereby clean",
        "gulp": "hereby",
        "lint": "hereby lint",
        "knip": "hereby knip",
        "format": "dprint fmt",
        "setup-hooks": "node scripts/link-hooks.mjs"
    },
    "browser": {
        "fs": false,
        "os": false,
        "path": false,
        "crypto": false,
        "buffer": false,
        "source-map-support": false,
        "inspector": false,
        "perf_hooks": false
    },
    "packageManager": "npm@8.19.4",
    "volta": {
        "node": "22.22.0",
        "npm": "8.19.4"
    }
}

```

## Content-View Excerpt

```json
{
    "name": "typescript",
    "author": "Microsoft Corp.",
    "homepage": "https://www.typescriptlang.org/",
    "version": "6.0.0",
    "license": "Apache-2.0",
    "description": "TypeScript is a language for application scale JavaScript development",
    "keywords": [
        "TypeScript",
        "Microsoft",
        "compiler",
        "language",
        "javascript"
    ],
    "bugs": {
        "url": "https://github.com/microsoft/TypeScript/issues"
    },
    "repository": {
        "type": "git",
        "url": "https://github.com/microsoft/TypeScript.git"
    },
    "main": "./lib/typescript.js",
    "typings": "./lib/typescript.d.ts",
    "bin": {
        "tsc": "./bin/tsc",
        "tsserver": "./bin/tsserver"
    },
    "engines": {
        "node": ">=14.17"
    },
    "files": [
        "bin",
        "lib",
        "!lib/enu",
        "LICENSE.txt",
        "README.md",
        "SECURITY.md",
        "ThirdPartyNoticeText.txt",
        "!**/.gitattributes"
    ],
    "devDependencies": {
        "@dprint/formatter": "^0.4.1",
        "@dprint/typescript": "0.93.4",
        "@esfx/canceltoken": "^1.0.0",
        "@eslint/js": "^10.0.1",
        "@octokit/rest": "^22.0.1",
        "@types/

... [truncated 1668 chars] ...

sts --no-typecheck",
        "clean": "hereby clean",
        "gulp": "hereby",
        "lint": "hereby lint",
        "knip": "hereby knip",
        "format": "dprint fmt",
        "setup-hooks": "node scripts/link-hooks.mjs"
    },
    "browser": {
        "fs": false,
        "os": false,
        "path": false,
        "crypto": false,
        "buffer": false,
        "source-map-support": false,
        "inspector": false,
        "perf_hooks": false
    },
    "packageManager": "npm@8.19.4",
    "volta": {
        "node": "22.22.0",
        "npm": "8.19.4"
    }
}

```

## Apply Minification Excerpt

```json
{"author":"Microsoft Corp.","bin":{"tsc":"./bin/tsc","tsserver":"./bin/tsserver"},"browser":{"buffer":false,"crypto":false,"fs":false,"inspector":false,"os":false,"path":false,"perf_hooks":false,"source-map-support":false},"bugs":{"url":"https://github.com/microsoft/TypeScript/issues"},"description":"TypeScript is a language for application scale JavaScript development","devDependencies":{"@dprint/formatter":"^0.4.1","@dprint/typescript":"0.93.4","@esfx/canceltoken":"^1.0.0","@eslint/js":"^10.0.1","@octokit/rest":"^22.0.1","@types/chai":"^4.3.20","@types/minimist":"^1.2.5","@types/mocha":"^10.0.10","@types/ms":"^2.1.0","@types/node":"latest","@types/source-map-support":"^0.5.10","@types/which":"^3.0.4","@typescript-eslint/rule-tester":"^8.57.2","@typescript-eslint/type-utils":"^8.57.2","@typescript-eslint/utils":"^8.57.2","azure-devops-node-api":"^15.1.3","c8":"^10.1.3","chai":"^4.5.0","chokidar":"^4.0.3","diff":"^8.0.4","dprint":"^0.49.1","esbuild":"^0.27.4","eslint":"^10.1.0","eslint-plugin-regexp":"^3.1.0","fast-xml-parser":"^5.5.9","glob":"^10.5.0","globals":"^17.4.0","hereby":"^1.14.0","jsonc-parser":"^3.3.1","knip":"^5.88.1","minimist":"^1.2.8","mocha":"^10.8.2","mocha-fivemat-progress-reporter":"^0

... [truncated 664 chars] ...

l":"https://github.com/microsoft/TypeScript.git"},"scripts":{"build":"npm run build:compiler && npm run build:tests","build:compiler":"hereby local","build:tests":"hereby tests","build:tests:notypecheck":"hereby tests --no-typecheck","clean":"hereby clean","format":"dprint fmt","gulp":"hereby","knip":"hereby knip","lint":"hereby lint","setup-hooks":"node scripts/link-hooks.mjs","test":"hereby runtests-parallel --light=false","test:eslint-rules":"hereby run-eslint-rules-tests"},"typings":"./lib/typescript.d.ts","version":"6.0.0","volta":{"node":"22.22.0","npm":"8.19.4"}}
```

## Sync Minify Excerpt

```json
{"author":"Microsoft Corp.","bin":{"tsc":"./bin/tsc","tsserver":"./bin/tsserver"},"browser":{"buffer":false,"crypto":false,"fs":false,"inspector":false,"os":false,"path":false,"perf_hooks":false,"source-map-support":false},"bugs":{"url":"https://github.com/microsoft/TypeScript/issues"},"description":"TypeScript is a language for application scale JavaScript development","devDependencies":{"@dprint/formatter":"^0.4.1","@dprint/typescript":"0.93.4","@esfx/canceltoken":"^1.0.0","@eslint/js":"^10.0.1","@octokit/rest":"^22.0.1","@types/chai":"^4.3.20","@types/minimist":"^1.2.5","@types/mocha":"^10.0.10","@types/ms":"^2.1.0","@types/node":"latest","@types/source-map-support":"^0.5.10","@types/which":"^3.0.4","@typescript-eslint/rule-tester":"^8.57.2","@typescript-eslint/type-utils":"^8.57.2","@typescript-eslint/utils":"^8.57.2","azure-devops-node-api":"^15.1.3","c8":"^10.1.3","chai":"^4.5.0","chokidar":"^4.0.3","diff":"^8.0.4","dprint":"^0.49.1","esbuild":"^0.27.4","eslint":"^10.1.0","eslint-plugin-regexp":"^3.1.0","fast-xml-parser":"^5.5.9","glob":"^10.5.0","globals":"^17.4.0","hereby":"^1.14.0","jsonc-parser":"^3.3.1","knip":"^5.88.1","minimist":"^1.2.8","mocha":"^10.8.2","mocha-fivemat-progress-reporter":"^0

... [truncated 664 chars] ...

l":"https://github.com/microsoft/TypeScript.git"},"scripts":{"build":"npm run build:compiler && npm run build:tests","build:compiler":"hereby local","build:tests":"hereby tests","build:tests:notypecheck":"hereby tests --no-typecheck","clean":"hereby clean","format":"dprint fmt","gulp":"hereby","knip":"hereby knip","lint":"hereby lint","setup-hooks":"node scripts/link-hooks.mjs","test":"hereby runtests-parallel --light=false","test:eslint-rules":"hereby run-eslint-rules-tests"},"typings":"./lib/typescript.d.ts","version":"6.0.0","volta":{"node":"22.22.0","npm":"8.19.4"}}
```

## Async Minify Excerpt

```json
{"author":"Microsoft Corp.","bin":{"tsc":"./bin/tsc","tsserver":"./bin/tsserver"},"browser":{"buffer":false,"crypto":false,"fs":false,"inspector":false,"os":false,"path":false,"perf_hooks":false,"source-map-support":false},"bugs":{"url":"https://github.com/microsoft/TypeScript/issues"},"description":"TypeScript is a language for application scale JavaScript development","devDependencies":{"@dprint/formatter":"^0.4.1","@dprint/typescript":"0.93.4","@esfx/canceltoken":"^1.0.0","@eslint/js":"^10.0.1","@octokit/rest":"^22.0.1","@types/chai":"^4.3.20","@types/minimist":"^1.2.5","@types/mocha":"^10.0.10","@types/ms":"^2.1.0","@types/node":"latest","@types/source-map-support":"^0.5.10","@types/which":"^3.0.4","@typescript-eslint/rule-tester":"^8.57.2","@typescript-eslint/type-utils":"^8.57.2","@typescript-eslint/utils":"^8.57.2","azure-devops-node-api":"^15.1.3","c8":"^10.1.3","chai":"^4.5.0","chokidar":"^4.0.3","diff":"^8.0.4","dprint":"^0.49.1","esbuild":"^0.27.4","eslint":"^10.1.0","eslint-plugin-regexp":"^3.1.0","fast-xml-parser":"^5.5.9","glob":"^10.5.0","globals":"^17.4.0","hereby":"^1.14.0","jsonc-parser":"^3.3.1","knip":"^5.88.1","minimist":"^1.2.8","mocha":"^10.8.2","mocha-fivemat-progress-reporter":"^0

... [truncated 664 chars] ...

l":"https://github.com/microsoft/TypeScript.git"},"scripts":{"build":"npm run build:compiler && npm run build:tests","build:compiler":"hereby local","build:tests":"hereby tests","build:tests:notypecheck":"hereby tests --no-typecheck","clean":"hereby clean","format":"dprint fmt","gulp":"hereby","knip":"hereby knip","lint":"hereby lint","setup-hooks":"node scripts/link-hooks.mjs","test":"hereby runtests-parallel --light=false","test:eslint-rules":"hereby run-eslint-rules-tests"},"typings":"./lib/typescript.d.ts","version":"6.0.0","volta":{"node":"22.22.0","npm":"8.19.4"}}
```

## Symbols

```txt
No symbols returned for this sample.
```
