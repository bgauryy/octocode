#!/usr/bin/env node

import { readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const SKILL_DIR = resolve(__dirname, '..');
const ENV_PATH = resolve(SKILL_DIR, '.env');

const ENDPOINT = 'https://google.serper.dev/search';

// Serper time filters use Google's `tbs` qdr tokens.
const TIME_RANGE_TBS = { day: 'qdr:d', week: 'qdr:w', month: 'qdr:m', year: 'qdr:y' };

function die(msg, code = 1) {
  process.stderr.write(`ERROR: ${msg}\n`);
  process.exitCode = code;
}

function loadEnvFile() {
  try {
    const lines = readFileSync(ENV_PATH, 'utf8').split('\n');
    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith('#')) continue;
      const normalized = trimmed.startsWith('export ') ? trimmed.slice('export '.length).trim() : trimmed;
      const eqIdx = normalized.indexOf('=');
      if (eqIdx === -1) continue;
      const key = normalized.slice(0, eqIdx).trim();
      const val = normalized.slice(eqIdx + 1).trim().replace(/^["']|["']$/g, '');
      if (!process.env[key]) process.env[key] = val;
    }
  } catch { /* .env not present */ }
}

// Unified env loading via octocode-config.mjs (injected by skills/scripts/sync.mjs).
//
// Priority (highest → lowest):
//   1. process.env already set (shell / MCP client / pi-extension session_start)
//   2. <workspace>/.octocode/.env   (project-level, WORKSPACE_ROOT or cwd)
//   3. ~/.octocode/.env             (global octocode home)
//   4. <skill-dir>/.env             (legacy skill-local fallback, used in source/dev)
//
// Project env wins over global; already-set process.env vars always win over both.
async function loadEnv() {
  try {
    const { propagateOctocodeEnv, getOctocodeHome } = await import(new URL('./octocode-config.mjs', import.meta.url).href);
    propagateOctocodeEnv({
      home: getOctocodeHome(),                               // ~/.octocode/.env
      cwd: process.env.WORKSPACE_ROOT || process.cwd(),     // <workspace>/.octocode/.env (wins)
      trusted: true,
    });
  } catch { /* octocode-config.mjs not present — local .env fallback below */ }
  loadEnvFile(); // skill-local .env: last resort, only sets keys not already in process.env
}

function parseArgs(argv) {
  const opts = {
    query: '', maxResults: 8, timeRange: 'year', gl: 'us', hl: 'en',
    check: false, presenceOnly: false, help: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--help' || a === '-h') { opts.help = true; continue; }
    if (a === '--check') { opts.check = true; continue; }
    if (a === '--presence-only') { opts.presenceOnly = true; continue; }
    if (a === '--query' || a === '-q') { opts.query = argv[++i] || ''; continue; }
    if (a === '--max-results') { opts.maxResults = Number(argv[++i]) || 8; continue; }
    if (a === '--time-range') { opts.timeRange = argv[++i] || 'year'; continue; }
    if (a === '--gl') { opts.gl = argv[++i] || 'us'; continue; }
    if (a === '--hl') { opts.hl = argv[++i] || 'en'; continue; }
    if (!opts.query) { opts.query = a; continue; }
    die(`Unknown argument: ${a}`); return null;
  }
  return opts;
}

async function validateKey(apiKey, opts) {
  const res = await fetch(ENDPOINT, {
    method: 'POST',
    headers: { 'X-API-KEY': apiKey, 'Content-Type': 'application/json' },
    body: JSON.stringify({
      q: 'Serper API health check',
      num: 1,
      gl: opts.gl,
      hl: opts.hl,
    }),
    signal: AbortSignal.timeout(30_000),
  });

  if (!res.ok) {
    const text = await res.text().catch(() => '');
    throw new Error(`Serper API ${res.status}: ${text}`);
  }
}

async function search(opts, apiKey) {
  const body = {
    q: opts.query,
    num: opts.maxResults,
    gl: opts.gl,
    hl: opts.hl,
  };
  const tbs = TIME_RANGE_TBS[opts.timeRange];
  if (tbs) body.tbs = tbs;

  const res = await fetch(ENDPOINT, {
    method: 'POST',
    headers: { 'X-API-KEY': apiKey, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(30_000),
  });

  if (!res.ok) {
    const text = await res.text().catch(() => '');
    die(`Serper API ${res.status}: ${text}`);
    return null;
  }
  return res.json();
}

// Normalize Serper's shape to match tavily-search.mjs (answer + results[{title,url,content}]).
function normalize(raw) {
  const out = { engine: 'serper', answer: '', results: [] };

  if (raw.answerBox) {
    const ab = raw.answerBox;
    out.answer = ab.answer || ab.snippet || ab.title || '';
  } else if (raw.knowledgeGraph?.description) {
    out.answer = raw.knowledgeGraph.description;
  }

  for (const r of raw.organic || []) {
    out.results.push({
      title: r.title || '',
      url: r.link || '',
      content: r.snippet || '',
      date: r.date || undefined,
      position: r.position,
    });
  }

  if (Array.isArray(raw.relatedSearches) && raw.relatedSearches.length) {
    out.relatedSearches = raw.relatedSearches.map(s => s.query).filter(Boolean);
  }
  out.raw = raw;
  return out;
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (!opts) return;

  if (opts.help) {
    console.log(`Serper (Google) web search — octocode-brainstorming

Usage:
  node serper-search.mjs --query "query" [options]
  node serper-search.mjs --check

Options:
  --query, -q      Search query (required unless --check)
  --max-results    Number of results (default: 8)
  --time-range     day, week, month, or year (default: year)
  --gl             Country code (default: us)
  --hl             Language code (default: en)
  --check          Validate SERPER_API_KEY with a live Serper request
  --presence-only  With --check, only verify a key is present locally

.env file: ${ENV_PATH}`);
    return;
  }

  await loadEnv();
  const apiKey = process.env.SERPER_API_KEY;

  if (opts.check) {
    if (!apiKey) {
      console.log(`serper: unavailable (SERPER_API_KEY not set)`);
      console.log(`Add SERPER_API_KEY to: ${ENV_PATH}`);
      process.exitCode = 1;
      return;
    }
    if (opts.presenceOnly) {
      console.log('serper: key present (not validated)');
      process.exitCode = 0;
      return;
    }
    try {
      await validateKey(apiKey, opts);
      console.log('serper: available (validated)');
      process.exitCode = 0;
    } catch (err) {
      console.log('serper: unavailable (key failed live validation)');
      console.log(err.message || String(err));
      console.log(`Update SERPER_API_KEY in: ${ENV_PATH}`);
      process.exitCode = 1;
    }
    return;
  }

  if (!apiKey) {
    die(`SERPER_API_KEY is not set. Add it to ${ENV_PATH} or export it in your shell.`);
    return;
  }
  if (!opts.query) {
    die('--query is required. Use --help for usage.');
    return;
  }

  process.stderr.write(`Searching Serper: "${opts.query}" (max=${opts.maxResults}, time=${opts.timeRange})\n`);
  const raw = await search(opts, apiKey);
  if (raw) {
    console.log(JSON.stringify(normalize(raw), null, 2));
  }
}

main().catch(err => die(err.message || String(err)));
