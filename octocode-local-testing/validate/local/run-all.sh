#!/bin/bash
# Re-runs every suite; raw JSON lands in raw/*.json, console tables in raw/*-log.txt
cd "$(dirname "$0")"
{ octocode --version; ./bin/rg --version | head -1; ast-grep --version; rust-analyzer --version; echo "typescript-language-server $(./tools/node_modules/.bin/typescript-language-server --version)"; echo "typescript $(node -p "require('./tools/node_modules/typescript/package.json').version")"; echo "pyright $(node -p "require('./tools/node_modules/pyright/package.json').version")"; clangd --version | head -1; } </dev/null > raw/versions.txt 2>&1
for s in ${SUITES:-search fetch structure ast lsp topo rewrite edge}; do echo "== $s $(date +%T)"; node $s.mjs </dev/null > raw/$s-log.txt 2>&1; echo "exit $?"; done
echo DONE
