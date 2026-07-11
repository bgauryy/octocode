#!/usr/bin/env node

import { die, loadEnv, splitList, normalizeApiKey } from './lib/web-search-common.mjs';

const ENDPOINT = 'https://api.exa.ai/search';

function parseArgs(argv) {
  const opts = {
    query: '', type: 'auto', maxResults: 10, category: '',
    includeDomains: [], excludeDomains: [], startDate: '', endDate: '',
    highlights: true, text: false,
    check: false, presenceOnly: false, help: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--help' || a === '-h') { opts.help = true; continue; }
    if (a === '--check') { opts.check = true; continue; }
    if (a === '--presence-only') { opts.presenceOnly = true; continue; }
    if (a === '--query' || a === '-q') { opts.query = argv[++i] || ''; continue; }
    if (a === '--type') { opts.type = argv[++i] || 'auto'; continue; }
    if (a === '--max-results') { opts.maxResults = Number(argv[++i]) || 10; continue; }
    if (a === '--category') { opts.category = argv[++i] || ''; continue; }
    if (a === '--include-domains') { opts.includeDomains = splitList(argv[++i]); continue; }
    if (a === '--exclude-domains') { opts.excludeDomains = splitList(argv[++i]); continue; }
    if (a === '--start-date') { opts.startDate = argv[++i] || ''; continue; }
    if (a === '--end-date') { opts.endDate = argv[++i] || ''; continue; }
    if (a === '--no-highlights') { opts.highlights = false; continue; }
    if (a === '--text') { opts.text = true; continue; }
    if (!opts.query) { opts.query = a; continue; }
    die(`Unknown argument: ${a}`); return null;
  }
  // Exa caps numResults at 100 (1–100 on /search); clamp to avoid a 400.
  opts.maxResults = Math.max(1, Math.min(100, opts.maxResults));
  return opts;
}

function buildContents(opts) {
  const contents = {};
  if (opts.highlights) contents.highlights = true;
  if (opts.text) contents.text = true;
  return contents;
}

async function validateKey(apiKey) {
  const res = await fetch(ENDPOINT, {
    method: 'POST',
    headers: { 'x-api-key': apiKey, 'content-type': 'application/json' },
    body: JSON.stringify({
      query: 'Exa API health check',
      numResults: 1,
      type: 'auto',
    }),
    signal: AbortSignal.timeout(30_000),
  });

  if (!res.ok) {
    const text = await res.text().catch(() => '');
    throw new Error(`Exa API ${res.status}: ${text}`);
  }
}

async function search(opts, apiKey) {
  const body = {
    query: opts.query,
    type: opts.type,
    numResults: opts.maxResults,
  };
  // Optional, all backed by the official Exa /search contract.
  if (opts.category) body.category = opts.category;
  if (opts.includeDomains.length) body.includeDomains = opts.includeDomains;
  if (opts.excludeDomains.length) body.excludeDomains = opts.excludeDomains;
  if (opts.startDate) body.startPublishedDate = opts.startDate;
  if (opts.endDate) body.endPublishedDate = opts.endDate;
  const contents = buildContents(opts);
  if (Object.keys(contents).length) body.contents = contents;

  const res = await fetch(ENDPOINT, {
    method: 'POST',
    headers: { 'x-api-key': apiKey, 'content-type': 'application/json' },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(30_000),
  });

  if (!res.ok) {
    const text = await res.text().catch(() => '');
    die(`Exa API ${res.status}: ${text}`);
    return null;
  }
  return res.json();
}

// Normalize Exa's shape to match tavily-search.mjs/serper-search.mjs (answer + results[{title,url,content}]).
function normalize(raw) {
  const out = { engine: 'exa', answer: raw.autopromptString || raw.resolvedSearchType || '', results: [] };

  for (const r of raw.results || []) {
    const highlightText = Array.isArray(r.highlights) && r.highlights.length ? r.highlights.join(' … ') : '';
    out.results.push({
      title: r.title || '',
      url: r.url || '',
      content: highlightText || r.summary || r.text || '',
      date: r.publishedDate || undefined,
      author: r.author || undefined,
      score: r.score,
    });
  }
  out.raw = raw;
  return out;
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (!opts) return;

  if (opts.help) {
    console.log(`Exa (AI-native) web search — octocode-brainstorming

Usage:
  node exa-search.mjs --query "query" [options]
  node exa-search.mjs --check

Options:
  --query, -q        Search query (required unless --check)
  --type             auto, neural, keyword, or fast (default: auto)
  --max-results      Number of results, 1-100 (default: 10; clamped to API max of 100)
  --category         Optional Exa category filter, e.g. "research paper", "news", "github", "company", "pdf"
  --include-domains  Comma-separated allowlist, e.g. "arxiv.org,github.com" (quality filter)
  --exclude-domains  Comma-separated blocklist, e.g. "pinterest.com,quora.com" (drop SEO/farm noise)
  --start-date       ISO 8601 lower bound on publish date (e.g. 2026-01-01)
  --end-date         ISO 8601 upper bound on publish date
  --no-highlights    Disable contents.highlights (enabled by default)
  --text             Also request contents.text (full extracted page text; larger payload)
  --check            Validate EXA_API_KEY with a live Exa request
  --presence-only    With --check, only verify a key is present locally

Environment: process env, then <workspace>/.octocode/.env, then <OCTOCODE_HOME>/.env`);
    return;
  }

  const envPath = await loadEnv();
  const apiKey = normalizeApiKey(process.env.EXA_API_KEY);

  if (opts.check) {
    if (!apiKey) {
      console.log(`exa: unavailable (EXA_API_KEY not set)`);
      console.log(`Add EXA_API_KEY to: ${envPath}`);
      process.exitCode = 1;
      return;
    }
    if (opts.presenceOnly) {
      console.log('exa: key present (not validated)');
      process.exitCode = 0;
      return;
    }
    try {
      await validateKey(apiKey);
      console.log('exa: available (validated)');
      process.exitCode = 0;
    } catch (err) {
      console.log('exa: unavailable (key failed live validation)');
      console.log(err.message || String(err));
      console.log(`Update EXA_API_KEY in: ${envPath}`);
      process.exitCode = 1;
    }
    return;
  }

  if (!apiKey) {
    die(`EXA_API_KEY is not set. Add it to ${envPath} or export it in your shell.`);
    return;
  }
  if (!opts.query) {
    die('--query is required. Use --help for usage.');
    return;
  }

  process.stderr.write(`Searching Exa: "${opts.query}" (type=${opts.type}, max=${opts.maxResults})\n`);
  const raw = await search(opts, apiKey);
  if (raw) {
    console.log(JSON.stringify(normalize(raw), null, 2));
  }
}

main().catch(err => die(err.message || String(err)));
