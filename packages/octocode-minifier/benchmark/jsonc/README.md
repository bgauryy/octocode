# JSONC (.jsonc)

Source sample: `jsonc/grammy-deno.jsonc`

Strategy: `json`

Agent rating: **8.3/10 (strong)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 1427 | - | - | - |
| content-view | 1427 | 0% | 0.036 ms | 8.3/10 |
| applyMinification | 1210 | 15.2% | 0.009 ms | 8.3/10 |
| sync minify | 1210 | 15.2% | 0.004 ms | 8.3/10 |
| async minify | 1210 | 15.2% | 0.012 ms | 8.3/10 |
| symbols | n/a | n/a | 0.002 ms | n/a |

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
{"lock":false,"nodeModulesDir":"none","tasks":{"check":"deno cache --allow-import src/mod.ts","backport":"deno --no-prompt --allow-read=. --allow-write=. https://deno.land/x/deno2node@v1.16.0/src/cli.ts tsconfig.json","test":"deno test --seed=123456 --parallel --allow-import ./test/","dev":"deno fmt && deno lint && deno task test && deno task check","coverage":"rm -rf ./test/cov_profile && deno task test --coverage=./test/cov_profile && deno coverage --lcov --output=./coverage.lcov ./test/cov_profile","report":"genhtml ./coverage.lcov --output-directory ./test/coverage/ && echo 'Point your browser to test/coverage/index.html to see the test coverage report.'","bundle-web":"mkdir -p out deno_cache && cd bundling && deno -ENRW bundle-web.ts dev ../src/mod.ts","contribs":"deno -ERS --allow-write=. --allow-net=api.github.com npm:all-contributors-cli","update-contribs":"deno run --allow-net=api.github.com --allow-read=. --allow-write --allow-env=GITHUB_TOKEN,GITHUB_OUTPUT .github/scripts/update-contributors.ts"},"exclude":["./bundling/bundles","./deno_cache/","./node_modules/","./out/","./package-lock.json","./test/cov_profile"],"fmt":{"indentWidth":4,"proseWrap":"preserve"},"compilerOptions":{}}
```

## Sync Minify Excerpt

```jsonc
{"lock":false,"nodeModulesDir":"none","tasks":{"check":"deno cache --allow-import src/mod.ts","backport":"deno --no-prompt --allow-read=. --allow-write=. https://deno.land/x/deno2node@v1.16.0/src/cli.ts tsconfig.json","test":"deno test --seed=123456 --parallel --allow-import ./test/","dev":"deno fmt && deno lint && deno task test && deno task check","coverage":"rm -rf ./test/cov_profile && deno task test --coverage=./test/cov_profile && deno coverage --lcov --output=./coverage.lcov ./test/cov_profile","report":"genhtml ./coverage.lcov --output-directory ./test/coverage/ && echo 'Point your browser to test/coverage/index.html to see the test coverage report.'","bundle-web":"mkdir -p out deno_cache && cd bundling && deno -ENRW bundle-web.ts dev ../src/mod.ts","contribs":"deno -ERS --allow-write=. --allow-net=api.github.com npm:all-contributors-cli","update-contribs":"deno run --allow-net=api.github.com --allow-read=. --allow-write --allow-env=GITHUB_TOKEN,GITHUB_OUTPUT .github/scripts/update-contributors.ts"},"exclude":["./bundling/bundles","./deno_cache/","./node_modules/","./out/","./package-lock.json","./test/cov_profile"],"fmt":{"indentWidth":4,"proseWrap":"preserve"},"compilerOptions":{}}
```

## Async Minify Excerpt

```jsonc
{"lock":false,"nodeModulesDir":"none","tasks":{"check":"deno cache --allow-import src/mod.ts","backport":"deno --no-prompt --allow-read=. --allow-write=. https://deno.land/x/deno2node@v1.16.0/src/cli.ts tsconfig.json","test":"deno test --seed=123456 --parallel --allow-import ./test/","dev":"deno fmt && deno lint && deno task test && deno task check","coverage":"rm -rf ./test/cov_profile && deno task test --coverage=./test/cov_profile && deno coverage --lcov --output=./coverage.lcov ./test/cov_profile","report":"genhtml ./coverage.lcov --output-directory ./test/coverage/ && echo 'Point your browser to test/coverage/index.html to see the test coverage report.'","bundle-web":"mkdir -p out deno_cache && cd bundling && deno -ENRW bundle-web.ts dev ../src/mod.ts","contribs":"deno -ERS --allow-write=. --allow-net=api.github.com npm:all-contributors-cli","update-contribs":"deno run --allow-net=api.github.com --allow-read=. --allow-write --allow-env=GITHUB_TOKEN,GITHUB_OUTPUT .github/scripts/update-contributors.ts"},"exclude":["./bundling/bundles","./deno_cache/","./node_modules/","./out/","./package-lock.json","./test/cov_profile"],"fmt":{"indentWidth":4,"proseWrap":"preserve"},"compilerOptions":{}}
```

## Symbols

```txt
No symbols returned for this sample.
```
