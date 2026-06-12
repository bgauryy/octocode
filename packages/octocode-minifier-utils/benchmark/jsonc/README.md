# JSONC (.jsonc)

Source sample: `jsonc/grammy-deno.jsonc`

Strategy: `json`

Agent rating: **8.3/10 (strong)**

Agent understanding from minified output: **9.2/10 (excellent)**

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
| applyMinification | 1210 | 15.2% | 0.057 ms | 8.3/10 |
| sync minify | 1210 | 15.2% | 0.053 ms | 8.3/10 |
| async minify | 1210 | 15.2% | 0.058 ms | 8.3/10 |
| symbols | 41 | 97.1% | 0.057 ms | n/a |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 10/10 |
| context budget | 5/10 |
| symbol context | 7/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 1427 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 1427 | 0% | 9.2/10 excellent | 10/10 | 10/10 |
| minify | 1210 | 15.2% | 9.5/10 excellent | 10/10 | 10/10 |
| symbols | 41 | 97.1% | 8.5/10 strong | 6.7/10 | 10/10 |

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
 1| {
27|     "compilerOptions": {}
28| }
```
