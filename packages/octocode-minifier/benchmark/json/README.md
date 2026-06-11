# JSON (.json)

Source sample: `json/typescript-package.json`

Strategy: `json`

Agent rating: **9/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 3468 | - | - | - |
| content-view | 3468 | 0% | 0.116 ms | 9/10 |
| applyMinification | 2464 | 29% | 0.03 ms | 9/10 |
| sync minify | 2464 | 29% | 0.012 ms | 9/10 |
| async minify | 2464 | 29% | 0.02 ms | 9/10 |
| symbols | n/a | n/a | 0.002 ms | n/a |

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
{"name":"typescript","author":"Microsoft Corp.","homepage":"https://www.typescriptlang.org/","version":"6.0.0","license":"Apache-2.0","description":"TypeScript is a language for application scale JavaScript development","keywords":["TypeScript","Microsoft","compiler","language","javascript"],"bugs":{"url":"https://github.com/microsoft/TypeScript/issues"},"repository":{"type":"git","url":"https://github.com/microsoft/TypeScript.git"},"main":"./lib/typescript.js","typings":"./lib/typescript.d.ts","bin":{"tsc":"./bin/tsc","tsserver":"./bin/tsserver"},"engines":{"node":">=14.17"},"files":["bin","lib","!lib/enu","LICENSE.txt","README.md","SECURITY.md","ThirdPartyNoticeText.txt","!**/.gitattributes"],"devDependencies":{"@dprint/formatter":"^0.4.1","@dprint/typescript":"0.93.4","@esfx/canceltoken":"^1.0.0","@eslint/js":"^10.0.1","@octokit/rest":"^22.0.1","@types/chai":"^4.3.20","@types/minimist":"^1.2.5","@types/mocha":"^10.0.10","@types/ms":"^2.1.0","@types/node":"latest","@types/source-map-support":"^0.5.10","@types/which":"^3.0.4","@typescript-eslint/rule-tester":"^8.57.2","@typescript-eslint/type-utils":"^8.57.2","@typescript-eslint/utils":"^8.57.2","azure-devops-node-api":"^15.1.3","c8":"^10.1.3","chai":"^4

... [truncated 664 chars] ...

nt-rules":"hereby run-eslint-rules-tests","build":"npm run build:compiler && npm run build:tests","build:compiler":"hereby local","build:tests":"hereby tests","build:tests:notypecheck":"hereby tests --no-typecheck","clean":"hereby clean","gulp":"hereby","lint":"hereby lint","knip":"hereby knip","format":"dprint fmt","setup-hooks":"node scripts/link-hooks.mjs"},"browser":{"fs":false,"os":false,"path":false,"crypto":false,"buffer":false,"source-map-support":false,"inspector":false,"perf_hooks":false},"packageManager":"npm@8.19.4","volta":{"node":"22.22.0","npm":"8.19.4"}}
```

## Sync Minify Excerpt

```json
{"name":"typescript","author":"Microsoft Corp.","homepage":"https://www.typescriptlang.org/","version":"6.0.0","license":"Apache-2.0","description":"TypeScript is a language for application scale JavaScript development","keywords":["TypeScript","Microsoft","compiler","language","javascript"],"bugs":{"url":"https://github.com/microsoft/TypeScript/issues"},"repository":{"type":"git","url":"https://github.com/microsoft/TypeScript.git"},"main":"./lib/typescript.js","typings":"./lib/typescript.d.ts","bin":{"tsc":"./bin/tsc","tsserver":"./bin/tsserver"},"engines":{"node":">=14.17"},"files":["bin","lib","!lib/enu","LICENSE.txt","README.md","SECURITY.md","ThirdPartyNoticeText.txt","!**/.gitattributes"],"devDependencies":{"@dprint/formatter":"^0.4.1","@dprint/typescript":"0.93.4","@esfx/canceltoken":"^1.0.0","@eslint/js":"^10.0.1","@octokit/rest":"^22.0.1","@types/chai":"^4.3.20","@types/minimist":"^1.2.5","@types/mocha":"^10.0.10","@types/ms":"^2.1.0","@types/node":"latest","@types/source-map-support":"^0.5.10","@types/which":"^3.0.4","@typescript-eslint/rule-tester":"^8.57.2","@typescript-eslint/type-utils":"^8.57.2","@typescript-eslint/utils":"^8.57.2","azure-devops-node-api":"^15.1.3","c8":"^10.1.3","chai":"^4

... [truncated 664 chars] ...

nt-rules":"hereby run-eslint-rules-tests","build":"npm run build:compiler && npm run build:tests","build:compiler":"hereby local","build:tests":"hereby tests","build:tests:notypecheck":"hereby tests --no-typecheck","clean":"hereby clean","gulp":"hereby","lint":"hereby lint","knip":"hereby knip","format":"dprint fmt","setup-hooks":"node scripts/link-hooks.mjs"},"browser":{"fs":false,"os":false,"path":false,"crypto":false,"buffer":false,"source-map-support":false,"inspector":false,"perf_hooks":false},"packageManager":"npm@8.19.4","volta":{"node":"22.22.0","npm":"8.19.4"}}
```

## Async Minify Excerpt

```json
{"name":"typescript","author":"Microsoft Corp.","homepage":"https://www.typescriptlang.org/","version":"6.0.0","license":"Apache-2.0","description":"TypeScript is a language for application scale JavaScript development","keywords":["TypeScript","Microsoft","compiler","language","javascript"],"bugs":{"url":"https://github.com/microsoft/TypeScript/issues"},"repository":{"type":"git","url":"https://github.com/microsoft/TypeScript.git"},"main":"./lib/typescript.js","typings":"./lib/typescript.d.ts","bin":{"tsc":"./bin/tsc","tsserver":"./bin/tsserver"},"engines":{"node":">=14.17"},"files":["bin","lib","!lib/enu","LICENSE.txt","README.md","SECURITY.md","ThirdPartyNoticeText.txt","!**/.gitattributes"],"devDependencies":{"@dprint/formatter":"^0.4.1","@dprint/typescript":"0.93.4","@esfx/canceltoken":"^1.0.0","@eslint/js":"^10.0.1","@octokit/rest":"^22.0.1","@types/chai":"^4.3.20","@types/minimist":"^1.2.5","@types/mocha":"^10.0.10","@types/ms":"^2.1.0","@types/node":"latest","@types/source-map-support":"^0.5.10","@types/which":"^3.0.4","@typescript-eslint/rule-tester":"^8.57.2","@typescript-eslint/type-utils":"^8.57.2","@typescript-eslint/utils":"^8.57.2","azure-devops-node-api":"^15.1.3","c8":"^10.1.3","chai":"^4

... [truncated 664 chars] ...

nt-rules":"hereby run-eslint-rules-tests","build":"npm run build:compiler && npm run build:tests","build:compiler":"hereby local","build:tests":"hereby tests","build:tests:notypecheck":"hereby tests --no-typecheck","clean":"hereby clean","gulp":"hereby","lint":"hereby lint","knip":"hereby knip","format":"dprint fmt","setup-hooks":"node scripts/link-hooks.mjs"},"browser":{"fs":false,"os":false,"path":false,"crypto":false,"buffer":false,"source-map-support":false,"inspector":false,"perf_hooks":false},"packageManager":"npm@8.19.4","volta":{"node":"22.22.0","npm":"8.19.4"}}
```

## Symbols

```txt
No symbols returned for this sample.
```
