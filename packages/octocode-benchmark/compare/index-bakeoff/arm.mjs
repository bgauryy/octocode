#!/usr/bin/env node

import { relative, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const [operation, requestJson] = process.argv.slice(2);
if (!operation || !requestJson) {
  throw new Error('usage: arm.mjs <build|index-query|live-query> <request-json>');
}

const request = JSON.parse(requestJson);
const engine = await import(pathToFileURL(resolve(request.engine)).href);
const EXCLUSIONS = [
  '.git',
  '.octocode-clone-meta.json',
  'node_modules',
  'target',
  'dist',
  'out',
  '.next',
];

function normalizedPath(root, path) {
  const value = path.replaceAll('\\', '/');
  const rootValue = root.replaceAll('\\', '/').replace(/\/$/, '');
  if (value === rootValue) return '.';
  if (value.startsWith(`${rootValue}/`)) return value.slice(rootValue.length + 1);
  return relative(root, path).replaceAll('\\', '/');
}

function unique(items) {
  return [...new Set(items)].sort();
}

async function build() {
  const result = await engine.buildIndex({
    store: request.store,
    exclusions: EXCLUSIONS,
    maxFiles: request.maxFiles ?? 50_000,
    maxEntries: request.maxEntries ?? 100_000,
    maxDepth: request.maxDepth ?? 128,
    maxFileBytes: request.maxFileBytes ?? 1_048_576,
    maxSourceBytes: request.maxSourceBytes ?? 2_147_483_648,
  });
  return { status: result.usable ? 'ok' : 'incomplete', items: [], metadata: result };
}

async function indexQuery() {
  if (request.case.kind === 'path') {
    return {
      status: 'unsupported',
      items: [],
      diagnostic: 'The current linear JSON index has no path-query operation.',
    };
  }
  const items = [];
  let offset = 0;
  let generation;
  let snapshot;
  for (;;) {
    const result = await engine.queryIndex({
      store: request.store,
      text: request.case.query,
      kind: request.case.kind,
      caseSensitive: request.case.caseSensitive,
      offset,
      limit: 1_000,
      expectedGeneration: generation,
      freshnessMaxEntries: 100_000,
      freshnessMaxDepth: 128,
    });
    generation = result.generation;
    snapshot = result.snapshot;
    if (!result.usable) {
      return {
        status: 'stale',
        items: [],
        diagnostic: result.diagnostic,
        generation,
        snapshot,
      };
    }
    for (const match of result.matches) {
      if (request.case.kind === 'content') {
        items.push(`${match.path}:${match.line}:${match.column}`);
      } else {
        items.push(
          `${match.path}:${match.line}:${match.column}:${match.value}:${match.symbolKind ?? ''}`
        );
      }
    }
    if (result.nextOffset === undefined) break;
    offset = result.nextOffset;
  }
  return { status: 'ok', items: unique(items), generation, snapshot };
}

async function liveContent() {
  const result = await engine.searchRipgrep({
    path: request.root,
    pattern: request.case.query,
    fixedString: true,
    caseSensitive: request.case.caseSensitive,
    caseInsensitive: !request.case.caseSensitive,
    onlyMatching: true,
    excludeDir: EXCLUSIONS,
    maxCollectedFiles: 100_000,
  });
  const items = [];
  for (const file of result.files) {
    const path = normalizedPath(request.root, file.path);
    for (const match of file.matches) {
      items.push(`${path}:${match.line}:${match.column}`);
    }
  }
  return {
    status: result.stats.capped ? 'incomplete' : 'ok',
    items: unique(items),
    metadata: { stats: result.stats },
  };
}

async function liveSymbol() {
  const result = await engine.scanGraphFacts({
    path: request.root,
    excludeDir: EXCLUSIONS,
    maxFiles: 100_000,
    maxFileBytes: 1_048_576,
  });
  const items = [];
  const expected = request.case.caseSensitive
    ? request.case.query
    : request.case.query.toLocaleLowerCase('en-US');
  for (const entry of result.entries) {
    const facts = JSON.parse(entry.factsJson);
    for (const declaration of facts.declarations ?? []) {
      const name = request.case.caseSensitive
        ? declaration.name
        : declaration.name.toLocaleLowerCase('en-US');
      if (name !== expected) continue;
      const start = declaration.selectionRange.start;
      items.push(
        `${entry.relativePath}:${start.line + 1}:${start.character}:${declaration.name}:${declaration.kind}`
      );
    }
  }
  return {
    status: result.truncated ? 'incomplete' : 'ok',
    items: unique(items),
    metadata: { skipped: result.skipped.length },
  };
}

async function livePath() {
  const result = await engine.queryFileSystem({
    path: request.root,
    recursive: true,
    maxDepth: 256,
    showHidden: false,
    names: [request.case.query],
    entryType: 'f',
    excludeDir: EXCLUSIONS,
    stopAtLimit: false,
    limit: 100_000,
  });
  return {
    status: result.wasCapped ? 'incomplete' : 'ok',
    items: unique(result.entries.map(entry => entry.relativePath)),
    metadata: { warnings: result.warnings },
  };
}

async function liveQuery() {
  if (request.case.kind === 'content') return liveContent();
  if (request.case.kind === 'symbol') return liveSymbol();
  if (request.case.kind === 'path') return livePath();
  throw new Error(`unsupported case kind: ${request.case.kind}`);
}

let result;
if (operation === 'build') result = await build();
else if (operation === 'index-query') result = await indexQuery();
else if (operation === 'live-query') result = await liveQuery();
else throw new Error(`unknown operation: ${operation}`);

process.stdout.write(`${JSON.stringify(result)}\n`);
