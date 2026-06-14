# JSONC (.jsonc)

Source sample: `jsonc/grammy-deno.jsonc`

Strategy: `json`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 1427 | - | - |
| content-view | 1427 | 0% | 0.01 ms |
| applyMinification | 1210 | 15.2% | 0.007 ms |
| sync minify | 1210 | 15.2% | 0.005 ms |
| async minify | 1210 | 15.2% | 0.008 ms |
| symbols | n/a | n/a | 0.001 ms |

## Notes

- engine-backed or parser-backed path.
- content-view kept original because the readable output was not shorter.
- symbols are not implemented for this extension.

## Before Excerpt

```jsonc
{
    "lock": false,
    "nodeModulesDir": "none",
    "tasks": {
        "check": "deno cache --allow-import src/mod.ts",
        "backport": "deno --no-prompt --allow-read=. --allow-write=. https://deno.land/x/deno2node@v1.16.0/src/cli.ts tsconfig.json",
        "test": "deno test --seed=123456 --parallel --allow-import ./test/",
        "dev": "deno fmt && deno lint && deno task test && deno task check",
        "coverage": "rm -rf ./test/cov_profile && deno task test --coverage=./test/cov_profile && deno coverage --lcov --output=./coverage.lcov ./test/cov_profile",
        "report": "genhtml ./coverage.lcov --output-directory ./test/coverage/ && echo 'Point your browser to test/coverage/index.html to see the test coverage report.'",
        "bundle-web": "mkdir -p out deno_cache && cd bundling && deno -ENRW bundle-web.ts dev ../src/mod.ts",
        "contribs": "deno -ERS --allow-write=. --allow-net=api.github.com npm:all-contributors-cli",
        "update-contribs": "deno run --allow-net=api.github.com --allow-read=. --allow-write --allow-env=GITHUB_TOKEN,GITHUB_OUTPUT .github/scripts/update-contributors.ts"
    },
    "exclude": [
        "./bundling/bundles",
        "./deno_cache/",
        "./node_modules/",
        "./out/",
        "./package-lock.json",
        "./test/cov_profile"
    ],
    "fmt": {
        "indentWidth": 4,
        "proseWrap": "preserve"
    },
    "compilerOptions": {}
}

```

## Content-View Excerpt

```jsonc
{
    "lock": false,
    "nodeModulesDir": "none",
    "tasks": {
        "check": "deno cache --allow-import src/mod.ts",
        "backport": "deno --no-prompt --allow-read=. --allow-write=. https://deno.land/x/deno2node@v1.16.0/src/cli.ts tsconfig.json",
        "test": "deno test --seed=123456 --parallel --allow-import ./test/",
        "dev": "deno fmt && deno lint && deno task test && deno task check",
        "coverage": "rm -rf ./test/cov_profile && deno task test --coverage=./test/cov_profile && deno coverage --lcov --output=./coverage.lcov ./test/cov_profile",
        "report": "genhtml ./coverage.lcov --output-directory ./test/coverage/ && echo 'Point your browser to test/coverage/index.html to see the test coverage report.'",
        "bundle-web": "mkdir -p out deno_cache && cd bundling && deno -ENRW bundle-web.ts dev ../src/mod.ts",
        "contribs": "deno -ERS --allow-write=. --allow-net=api.github.com npm:all-contributors-cli",
        "update-contribs": "deno run --allow-net=api.github.com --allow-read=. --allow-write --allow-env=GITHUB_TOKEN,GITHUB_OUTPUT .github/scripts/update-contributors.ts"
    },
    "exclude": [
        "./bundling/bundles",
        "./deno_cache/",
        "./node_modules/",
        "./out/",
        "./package-lock.json",
        "./test/cov_profile"
    ],
    "fmt": {
        "indentWidth": 4,
        "proseWrap": "preserve"
    },
    "compilerOptions": {}
}

```

## Apply Minification Excerpt

```jsonc
{"compilerOptions":{},"exclude":["./bundling/bundles","./deno_cache/","./node_modules/","./out/","./package-lock.json","./test/cov_profile"],"fmt":{"indentWidth":4,"proseWrap":"preserve"},"lock":false,"nodeModulesDir":"none","tasks":{"backport":"deno --no-prompt --allow-read=. --allow-write=. https://deno.land/x/deno2node@v1.16.0/src/cli.ts tsconfig.json","bundle-web":"mkdir -p out deno_cache && cd bundling && deno -ENRW bundle-web.ts dev ../src/mod.ts","check":"deno cache --allow-import src/mod.ts","contribs":"deno -ERS --allow-write=. --allow-net=api.github.com npm:all-contributors-cli","coverage":"rm -rf ./test/cov_profile && deno task test --coverage=./test/cov_profile && deno coverage --lcov --output=./coverage.lcov ./test/cov_profile","dev":"deno fmt && deno lint && deno task test && deno task check","report":"genhtml ./coverage.lcov --output-directory ./test/coverage/ && echo 'Point your browser to test/coverage/index.html to see the test coverage report.'","test":"deno test --seed=123456 --parallel --allow-import ./test/","update-contribs":"deno run --allow-net=api.github.com --allow-read=. --allow-write --allow-env=GITHUB_TOKEN,GITHUB_OUTPUT .github/scripts/update-contributors.ts"}}
```

## Sync Minify Excerpt

```jsonc
{"compilerOptions":{},"exclude":["./bundling/bundles","./deno_cache/","./node_modules/","./out/","./package-lock.json","./test/cov_profile"],"fmt":{"indentWidth":4,"proseWrap":"preserve"},"lock":false,"nodeModulesDir":"none","tasks":{"backport":"deno --no-prompt --allow-read=. --allow-write=. https://deno.land/x/deno2node@v1.16.0/src/cli.ts tsconfig.json","bundle-web":"mkdir -p out deno_cache && cd bundling && deno -ENRW bundle-web.ts dev ../src/mod.ts","check":"deno cache --allow-import src/mod.ts","contribs":"deno -ERS --allow-write=. --allow-net=api.github.com npm:all-contributors-cli","coverage":"rm -rf ./test/cov_profile && deno task test --coverage=./test/cov_profile && deno coverage --lcov --output=./coverage.lcov ./test/cov_profile","dev":"deno fmt && deno lint && deno task test && deno task check","report":"genhtml ./coverage.lcov --output-directory ./test/coverage/ && echo 'Point your browser to test/coverage/index.html to see the test coverage report.'","test":"deno test --seed=123456 --parallel --allow-import ./test/","update-contribs":"deno run --allow-net=api.github.com --allow-read=. --allow-write --allow-env=GITHUB_TOKEN,GITHUB_OUTPUT .github/scripts/update-contributors.ts"}}
```

## Async Minify Excerpt

```jsonc
{"compilerOptions":{},"exclude":["./bundling/bundles","./deno_cache/","./node_modules/","./out/","./package-lock.json","./test/cov_profile"],"fmt":{"indentWidth":4,"proseWrap":"preserve"},"lock":false,"nodeModulesDir":"none","tasks":{"backport":"deno --no-prompt --allow-read=. --allow-write=. https://deno.land/x/deno2node@v1.16.0/src/cli.ts tsconfig.json","bundle-web":"mkdir -p out deno_cache && cd bundling && deno -ENRW bundle-web.ts dev ../src/mod.ts","check":"deno cache --allow-import src/mod.ts","contribs":"deno -ERS --allow-write=. --allow-net=api.github.com npm:all-contributors-cli","coverage":"rm -rf ./test/cov_profile && deno task test --coverage=./test/cov_profile && deno coverage --lcov --output=./coverage.lcov ./test/cov_profile","dev":"deno fmt && deno lint && deno task test && deno task check","report":"genhtml ./coverage.lcov --output-directory ./test/coverage/ && echo 'Point your browser to test/coverage/index.html to see the test coverage report.'","test":"deno test --seed=123456 --parallel --allow-import ./test/","update-contribs":"deno run --allow-net=api.github.com --allow-read=. --allow-write --allow-env=GITHUB_TOKEN,GITHUB_OUTPUT .github/scripts/update-contributors.ts"}}
```

## Symbols

```txt
No symbols returned for this sample.
```
