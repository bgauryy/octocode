// The soak call mix: local calls (this repo + the benchmark corpora), GitHub/registry calls,
// clasify, and the call the --inject-slow-ms hook gives a tiny deadline.
import fs from 'node:fs';
import path from 'node:path';
import { REPOS_REL, ROOT } from '../paths.mjs';

// The first five stay in this repo (`--light`); heavy cold LSP indexing comes last
// and `cli: false` keeps it out of the 100-start CLI loop.
export const LOCAL = [
  { tool: 'localSearch', args: { matchString: 'startServer', path: 'packages/octocode-benchmark/local/e2e/harness' } },
  { tool: 'localFetch', args: { path: 'packages/octocode-benchmark/local/e2e/harness/mcp-client.mjs', matchString: 'export async function startServer', block: true } },
  { tool: 'structureSearch', args: { operation: 'files', path: 'packages/octocode-config/src' } },
  { tool: 'astSearch', args: { operation: 'symbols', path: 'packages/octocode-benchmark/local/e2e/harness/mcp-client.mjs' } },
  { tool: 'lspSearch', args: { operation: 'documentSymbols', path: 'packages/octocode-benchmark/local/e2e/harness/mcp-client.mjs' } },
  { tool: 'localSearch', args: { matchString: 'budget', path: `${REPOS_REL}/rust/tokio/src`, caseMode: 'insensitive' } },
  { tool: 'localFetch', args: { path: `${REPOS_REL}/rust/tokio/src/runtime/builder.rs`, matchString: 'cannot be set to 0', contextLines: 3 } },
  { tool: 'structureSearch', args: { operation: 'files', path: `${REPOS_REL}/go/scrape`, include: ['*_test.go'] } },
  { tool: 'astSearch', args: { operation: 'match', path: `${REPOS_REL}/rust/tokio/src`, pattern: 'Pin::new_unchecked($A)', language: 'rust' } },
  { tool: 'localSearch', args: { matchString: 'run_on_commit', path: `${REPOS_REL}/python/django/db` } },
  { tool: 'localFetch', args: { path: `${REPOS_REL}/python/django/db/backends/base/base.py`, ranges: ['95-105', '255-265', '340-350', '410-420'] } },
  { tool: 'structureSearch', args: { operation: 'files', path: `${REPOS_REL}/python/django/db`, extensions: ['py'] } },
  { tool: 'astSearch', args: { operation: 'match', path: `${REPOS_REL}/rust/tokio/src/fs`, pattern: '$X.unwrap()', language: 'rust' } },
  { tool: 'localSearch', args: { matchString: 'fn merge_near_windows', path: 'packages/octocode-native/crates' } },
  { tool: 'lspSearch', cli: false, args: { operation: 'references', path: `${REPOS_REL}/rust/tokio/src/runtime/task/harness.rs`, symbolName: 'try_read_output', line: 281 } },
  { tool: 'lspSearch', cli: false, args: { operation: 'callers', path: `${REPOS_REL}/tsx/packages/common/src/url.ts`, symbolName: 'toValidURL', line: 21 } },
];
const STARLETTE = { owner: 'Kludex', repo: 'starlette', ref: '63c5760d8a672cee96e1e523d84bfa1c77d9ee4c' };
const TOKIO_AT = { owner: 'tokio-rs', repo: 'tokio', ref: 'facc6fc47eb2ba13a465a3179f8e34db620561ec' };
export const GITHUB = [
  { tool: 'ghSearchRepo', args: { keywords: ['tokio'], language: 'rust', sort: 'stars', pageSize: 5 } },
  { tool: 'ghSearchCode', args: { owner: 'tokio-rs', repo: 'tokio', keywords: ['max_blocking_threads'], pageSize: 5 } },
  { tool: 'ghStructure', args: { ...STARLETTE, path: 'starlette' } },
  { tool: 'ghGetFileContent', args: { ...TOKIO_AT, path: 'tokio/src/runtime/builder.rs', matchString: 'max_blocking_threads: 512', contextLines: 0 } },
  { tool: 'ghSearchHistory', args: { operation: 'commit', owner: 'fastapi', repo: 'fastapi', path: 'fastapi/routing.py', until: '2026-09-01', pageSize: 5 } },
  { tool: 'ghGetHistoryItem', args: { operation: 'pullRequest', owner: 'fastapi', repo: 'fastapi', number: 16403 } },
  { tool: 'artifactSearch', args: { ecosystem: 'npm', packageName: 'express', version: '4.21.2' } },
  { tool: 'ghSearchCode', args: { owner: 'Kludex', repo: 'starlette', keywords: ['wrap_app_handling_exceptions'] } },
  { tool: 'ghStructure', args: { ...STARLETTE, include: ['**/_exception_handler.py'] } },
  { tool: 'ghGetFileContent', args: { ...TOKIO_AT, path: 'tokio/src/runtime/builder.rs', matchString: '^\\s*pub fn \\w+', regex: 'rust', contextLines: 0 } },
  { tool: 'ghSearchHistory', args: { operation: 'pullRequest', owner: 'pydantic', repo: 'pydantic', keywords: ['13786'] } },
  { tool: 'ghGetHistoryItem', args: { operation: 'issue', owner: 'cli', repo: 'cli', number: 14404 } },
  { tool: 'artifactSearch', args: { ecosystem: 'pypi', packageName: 'requests', version: '2.31.0' } },
];
export const CLASIFY = [
  { tool: 'clasify', args: { id: 't', source: { tool: 'localFetch', query: { path: `${REPOS_REL}/rust/tokio/src/runtime/builder.rs`, ranges: ['560-600'] } }, questions: { t: { type: 'noul', instructions: 'Does the selection reject a worker thread count of zero?' } } } },
];
export const INJECTED = { tool: 'localSearch', label: 'inject-slow', args: { matchString: 'budget', path: `${REPOS_REL}/rust/tokio/src`, caseMode: 'insensitive' } };

/** The local calls a run uses (`light`: the five in this repo), present in this checkout. */
export const localCalls = light => (light ? LOCAL.slice(0, 5) : LOCAL).filter(exists);
/** The call's path exists in this checkout (corpora are optional). */
export const exists = spec => [spec.args.path].filter(Boolean).every(p => fs.existsSync(path.resolve(ROOT, p)));
