#!/usr/bin/env node


import { readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const SKILL_DIR = resolve(__dirname, '..');
const ENV_PATH = resolve(SKILL_DIR, '.env');

try {
  const lines = readFileSync(ENV_PATH, 'utf8').split('\n');
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const eqIdx = trimmed.indexOf('=');
    if (eqIdx === -1) continue;
    const key = trimmed.slice(0, eqIdx).trim();
    const val = trimmed.slice(eqIdx + 1).trim().replace(/^["']|["']$/g, '');
    if (!process.env[key]) process.env[key] = val;
  }
} catch { /* .env not present */ }

const ENDPOINT = 'https://api.tavily.com/search';

function die(msg, code = 1) {
  process.stderr.write(`ERROR: ${msg}\n`);
  process.exitCode = code;
}

function parseArgs(argv) {
  const opts = { query: '', searchDepth: 'advanced', maxResults: 8, topic: 'general', timeRange: 'year', check: false, help: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--help' || a === '-h') { opts.help = true; continue; }
    if (a === '--check') { opts.check = true; continue; }
    if (a === '--query' || a === '-q') { opts.query = argv[++i] || ''; continue; }
    if (a === '--depth') { opts.searchDepth = argv[++i] || 'advanced'; continue; }
    if (a === '--max-results') { opts.maxResults = Number(argv[++i]) || 8; continue; }
    if (a === '--topic') { opts.topic = argv[++i] || 'general'; continue; }
    if (a === '--time-range') { opts.timeRange = argv[++i] || 'year'; continue; }
    if (!opts.query) { opts.query = a; continue; }
    die(`Unknown argument: ${a}`); return null;
  }
  return opts;
}

async function search(opts, apiKey) {
  const body = {
    query: opts.query,
    search_depth: opts.searchDepth,
    topic: opts.topic,
    max_results: opts.maxResults,
    include_answer: true,
    include_raw_content: false,
    time_range: opts.timeRange,
  };

  const res = await fetch(ENDPOINT, {
    method: 'POST',
    headers: { Authorization: `Bearer ${apiKey}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(30_000),
  });

  if (!res.ok) {
    const text = await res.text().catch(() => '');
    die(`Tavily API ${res.status}: ${text}`);
    return null;
  }
  return res.json();
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (!opts) return;

  if (opts.help) {
    console.log(`Tavily web search — octocode-brainstorming

Usage:
  node tavily-search.mjs --query "query" [options]
  node tavily-search.mjs --check

Options:
  --query, -q      Search query (required unless --check)
  --depth          basic or advanced (default: advanced)
  --max-results    Number of results (default: 8)
  --topic          general or news (default: general)
  --time-range     day, week, month, or year (default: year)
  --check          Exit 0 if TAVILY_API_KEY is set, 1 otherwise

.env file: ${ENV_PATH}`);
    return;
  }

  const apiKey = process.env.TAVILY_API_KEY;

  if (opts.check) {
    if (apiKey) {
      console.log('tavily: available');
      process.exitCode = 0;
    } else {
      console.log(`tavily: unavailable (TAVILY_API_KEY not set)`);
      console.log(`Add TAVILY_API_KEY to: ${ENV_PATH}`);
      process.exitCode = 1;
    }
    return;
  }

  if (!apiKey) {
    die(`TAVILY_API_KEY is not set. Add it to ${ENV_PATH} or export it in your shell.`);
    return;
  }
  if (!opts.query) {
    die('--query is required. Use --help for usage.');
    return;
  }

  process.stderr.write(`Searching Tavily: "${opts.query}" (depth=${opts.searchDepth}, max=${opts.maxResults})\n`);
  const result = await search(opts, apiKey);
  if (result) {
    result.engine = 'tavily';
    console.log(JSON.stringify(result, null, 2));
  }
}

main().catch(err => die(err.message || String(err)));
