#!/usr/bin/env node
/**
 * Read one source the way verify-claims.mjs reads it, so a quote copied from this output verifies.
 * With --grep, prints each match with surrounding text to copy; without it, prints the page text.
 */
import { fail } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';
import { VIA_MODES, loadSource } from './verify-claims.mjs';

const USAGE = `Usage: read-page.mjs --url <https-url> [--grep <text>]... [--context <chars>] [--max-matches <n>] [--max-chars <n>] [--via auto|exa|tavily|none]

Fetches the page through the same reader, network guard and provider-copy fallback as verify-claims.mjs, and prints
JSON: url, finalUrl, title, published, modified, via (when a provider copy was read), chars, and either
matches[] ({grep, index, text}: each match with --context characters around it, default 300) or text (the page
text, first --max-chars characters, default 20000, with truncated:true when cut). Copy quotes from that text.
--grep matches case- and typography-insensitively; repeat it to search several phrases. counts gives every phrase's total;
matches shows the first --max-matches per phrase (default 5; 0 shows all), with truncated:true when some are hidden.
The snippet text is the page's own text, so a quote copied from it is verbatim.
Exit 0 when the page was read (and, with --grep, every phrase matched), 1 when it could not be read or a phrase
is absent, 2 for invalid input.`;

function parseArgs(argv) {
  const opts = { url: null, grep: [], context: 300, maxChars: 20000, maxMatches: 5, via: 'auto' };
  for (let i = 0; i < argv.length; i += 2) {
    const [flag, value] = [argv[i], argv[i + 1]];
    if (!['--url', '--grep', '--context', '--max-chars', '--max-matches', '--via'].includes(flag)) fail('invalidArguments', `Unknown option: ${flag} (see --help)`, 2);
    if (value == null || value.startsWith('--')) fail('invalidArguments', `missing value for ${flag}`, 2);
    if (flag === '--grep') opts.grep.push(value);
    else if (flag === '--context') opts.context = Number(value);
    else if (flag === '--max-chars') opts.maxChars = Number(value);
    else if (flag === '--max-matches') opts.maxMatches = Number(value);
    else opts[flag.slice(2)] = value;
  }
  if (!opts.url || !/^https?:\/\//i.test(opts.url)) fail('invalidArguments', '--url must be an http(s) URL', 2);
  if (!Number.isSafeInteger(opts.context) || opts.context < 0 || opts.context > 5000) fail('invalidArguments', '--context must be 0-5000', 2);
  if (!Number.isSafeInteger(opts.maxChars) || opts.maxChars < 1) fail('invalidArguments', '--max-chars must be a positive integer', 2);
  if (!Number.isSafeInteger(opts.maxMatches) || opts.maxMatches < 0) fail('invalidArguments', '--max-matches must be 0 (all) or a positive integer', 2);
  if (!VIA_MODES.includes(opts.via)) fail('invalidArguments', `--via must be one of ${VIA_MODES.join(', ')}`, 2);
  return opts;
}

// Folds text the way the verifier's normalize() compares it (typography, case, emphasis marks, citation markers,
// whitespace) while recording, for each folded character, where it came from in the original text.
const FOLDS = [[/[‘’‚‛′`]/, "'"], [/[“”„‟″]/, '"'], [/[‐-―−]/, '-']];
export function foldWithMap(text) {
  let out = '';
  const map = [];
  let space = true;
  const push = (folded, at) => { for (const ch of folded) { out += ch; map.push(at); } };
  for (let i = 0; i < text.length;) {
    const cite = /^\[\d+\]/.exec(text.slice(i, i + 12));
    if (cite) { if (!space) { push(' ', i); space = true; } i += cite[0].length; continue; }
    const ch = String.fromCodePoint(text.codePointAt(i));
    if (/[​-‍﻿­*_]/.test(ch)) { i += ch.length; continue; }
    if (/\s/.test(ch)) { if (!space) { push(' ', i); space = true; } i += ch.length; continue; }
    let folded = ch.normalize('NFKC');
    for (const [pattern, replacement] of FOLDS) if (pattern.test(folded)) folded = replacement;
    push(folded.toLowerCase(), i);
    space = false;
    i += ch.length;
  }
  if (out.endsWith(' ')) { out = out.slice(0, -1); map.pop(); }
  return { text: out, map };
}

/** Each occurrence of the phrase, matched on folded text and cut from the page's own text around it. */
export function findMatches(text, phrase, context) {
  const flat = String(text).replace(/\s+/g, ' ').trim();
  const hay = foldWithMap(flat);
  const needle = foldWithMap(phrase).text;
  const out = [];
  if (!needle) return out;
  for (let at = hay.text.indexOf(needle); at !== -1; at = hay.text.indexOf(needle, at + needle.length)) {
    const from = hay.map[at];
    const last = hay.map[at + needle.length - 1];
    const to = last + String.fromCodePoint(flat.codePointAt(last)).length;
    out.push({ grep: phrase, index: from, text: flat.slice(Math.max(0, from - context), to + context) });
  }
  return out;
}

if (isMainModule(import.meta.url)) {
  if (process.argv.includes('--help') || process.argv.includes('-h')) { console.log(USAGE); process.exit(0); }
  const opts = parseArgs(process.argv.slice(2));
  const page = await loadSource(opts.url, opts.via);
  const meta = { url: opts.url, finalUrl: page.finalUrl || opts.url, title: page.title ?? null, published: page.published ?? null, modified: page.modified ?? null, ...(page.via ? { via: page.via } : {}) };
  if (page.status) fail(page.status, page.error || 'page could not be read', 1, meta);
  const flat = page.text.replace(/\s+/g, ' ').trim();
  if (opts.grep.length) {
    const found = opts.grep.map((phrase) => findMatches(page.text, phrase, opts.context));
    const counts = Object.fromEntries(opts.grep.map((phrase, i) => [phrase, found[i].length]));
    const matches = found.flatMap((list) => (opts.maxMatches ? list.slice(0, opts.maxMatches) : list));
    const missing = opts.grep.filter((phrase) => !counts[phrase]);
    const truncated = found.some((list) => opts.maxMatches && list.length > opts.maxMatches);
    console.log(JSON.stringify({ ok: missing.length === 0, ...meta, chars: flat.length, counts, matches, ...(truncated ? { truncated: true } : {}), ...(missing.length ? { missing } : {}) }, null, 2));
    process.exit(missing.length ? 1 : 0);
  }
  const truncated = flat.length > opts.maxChars;
  console.log(JSON.stringify({ ok: true, ...meta, chars: flat.length, text: flat.slice(0, opts.maxChars), ...(truncated ? { truncated: true } : {}) }, null, 2));
}
