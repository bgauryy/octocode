#!/usr/bin/env node
/**
 * refresh-word-list — rebuild assets/google-word-list.tsv from the live word list page.
 * Run when the guide changes; the TSV is the data behind style-lint's word-list rule.
 *
 * Usage:
 *   node scripts/refresh-word-list.mjs [--dry-run] [--json] [--out <path>] [--url <url>]
 * Exit codes: 0 written or unchanged, 1 fetch/parse failure, 2 bad usage.
 */
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const argv = process.argv.slice(2);
const flag = (n) => argv.includes(n);
const val = (n, d) => { const i = argv.indexOf(n); return i >= 0 && argv[i + 1] ? argv[i + 1] : d; };
const skillRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

if (flag('--help')) {
  console.log(`refresh-word-list — rebuild the word-list data behind style-lint

  node scripts/refresh-word-list.mjs [--dry-run] [--json] [--out <path>] [--url <url>]

  --dry-run   report the diff, write nothing
  --json      machine-readable summary
  --out       target TSV (default assets/google-word-list.tsv)
  --url       source page (default https://developers.google.com/style/word-list)`);
  process.exit(0);
}

const url = val('--url', 'https://developers.google.com/style/word-list');
const out = resolve(process.cwd(), val('--out', join(skillRoot, 'assets/google-word-list.tsv')));

function toText(html) {
  const article = html.match(/<article[\s\S]*?<\/article>/i) || html.match(/<main[\s\S]*?<\/main>/i);
  return (article ? article[0] : html)
    .replace(/<(script|style|nav)[\s\S]*?<\/\1>/gi, ' ')
    .replace(/<\/(p|div|li|tr|h1|h2|h3|h4|pre|dt|dd|section)>/gi, '\n')
    .replace(/<li[^>]*>/gi, '\n')
    .replace(/<h([1-6])[^>]*>/gi, '\n### ')
    .replace(/<code[^>]*>|<\/code>/gi, '`')
    .replace(/<[^>]+>/g, '')
    .replace(/&nbsp;/g, ' ').replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"').replace(/&#39;|&rsquo;|&lsquo;/g, "'").replace(/&mdash;/g, '—')
    .replace(/&ndash;/g, '–').replace(/&hellip;/g, '…').replace(/&[a-z]+;/g, ' ')
    .replace(/[ \t]+/g, ' ');
}

/**
 * Entries render as a term followed by the sentinel word "link" — either on the same line
 * ("and/or link") or on its own line when the term wraps ("crazy, bonkers, mad," / "loony" / "link").
 * A blank or whitespace-only line always precedes an entry, so the term is every consecutive
 * non-empty line since the last blank.
 */
function parseEntries(text) {
  const entries = [];
  let cur = null;
  let pending = [];
  let started = false; // entries begin at the alphabet index; skip the page preamble
  for (const line of text.split('\n')) {
    const t = line.trim();
    if (!t) { pending = []; continue; }
    if (/^### /.test(t)) {
      if (/^### (Numbers and Symbols|[A-Z])$/.test(t)) started = true;
      pending = [];
      continue;
    }
    if (!started) continue;
    if (t === 'link' || /\slink$/.test(t)) {
      const tail = t === 'link' ? pending : [...pending, t.replace(/\s*link$/, '')];
      const term = tail.join(' ').replace(/\s+/g, ' ').trim();
      pending = [];
      if (term && term.length <= 120) { cur = { term, body: [] }; entries.push(cur); continue; }
    }
    if (cur) cur.body.push(t);
    // A term never ends a sentence and is never a long clause: keep guidance out of the buffer.
    if (!/[.!?:]$/.test(t) && t.split(/\s+/).length <= 14) pending.push(t);
    else pending = [];
  }
  // A wrapped term's lines also land in the previous entry's guidance: strip that tail.
  for (let i = 0; i < entries.length - 1; i++) {
    const next = entries[i + 1].term;
    const body = entries[i].body;
    while (body.length && next.startsWith(body[body.length - 1])) body.pop();
  }
  return entries;
}

function verdict(body) {
  const b = body.join(' ');
  // "Don't hyphenate/abbreviate/capitalize/..." are spelling rules, not bans on the term.
  if (/^Don't (hyphenate|abbreviate|capitalize|spell|shorten|pluralize|italicize|put a period|use a period)/i.test(b)) return 'usage';
  if (/^(Don't use|Never use|Don't)\b/i.test(b)) return 'dont-use';
  if (/^(Avoid|Use with caution)\b/i.test(b)) return 'avoid';
  // Prescriptive replacements without a keyword: "Use <x> instead", "Use more widely understood terms".
  if (/^Use (more |non-gendered|a term like|terms like|widely)/i.test(b) || /\bInstead,? use\b/i.test(b.slice(0, 160))
    || /\buse (non-gendered|inclusive|more widely understood)\b/i.test(b.slice(0, 200))) return 'avoid';
  if (/\b(Don't use|Avoid)\b/i.test(b)) return 'caution';
  return 'usage';
}

let html;
try {
  const res = await fetch(url, { headers: { 'user-agent': 'octocode-documentation/refresh-word-list' } });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  html = await res.text();
} catch (err) {
  console.error(`refresh-word-list: cannot fetch ${url}: ${err.message}`);
  process.exit(1);
}

const entries = parseEntries(toText(html));
if (entries.length < 300) {
  console.error(`refresh-word-list: parsed only ${entries.length} entries from ${url}; page layout probably changed — not writing.`);
  process.exit(1);
}

const seen = new Map();
const rows = entries.map((e) => {
  const body = e.body.join(' ').replace(/\s+/g, ' ').replace(/\t/g, ' ');
  // Truncate on a sentence boundary, and always keep sentences carrying the operative rule.
  let short = body;
  if (body.length > 520) {
    const sentences = body.split(/(?<=\.)\s+/);
    const keep = [];
    for (const sentence of sentences) {
      const operative = /\b(Instead|Recommended|Not recommended|code font|Don't use|Avoid)\b/i.test(sentence);
      if (keep.join(' ').length > 520 && !operative) continue;
      keep.push(sentence);
      if (keep.join(' ').length > 900) break;
    }
    short = keep.join(' ');
  }
  const term = e.term.replace(/\s+/g, ' ');
  const count = (seen.get(term) || 0) + 1;
  seen.set(term, count);
  const key = count > 1 ? `${term} (${count})` : term;
  return [key, verdict(e.body), short || `Spelling and usage: ${term}.`].join('\t');
});
const header = `# Google developer documentation style guide word list — ${url}\n# term\tverdict(dont-use|avoid|caution|usage)\tguidance\n`;
const next = `${header}${rows.join('\n')}\n`;

const previous = existsSync(out) ? readFileSync(out, 'utf8') : '';
const terms = (text) => new Set(text.split('\n').filter((l) => l && !l.startsWith('#')).map((l) => l.split('\t')[0]));
const before = terms(previous);
const after = terms(next);
const summary = {
  url,
  out,
  entries: entries.length,
  added: [...after].filter((t) => !before.has(t)),
  removed: [...before].filter((t) => !after.has(t)),
  changed: previous !== next,
  written: false,
};

if (summary.changed && !flag('--dry-run')) { writeFileSync(out, next); summary.written = true; }

if (flag('--json')) console.log(JSON.stringify(summary, null, 2));
else {
  console.log(`refresh-word-list: ${summary.entries} entries from ${url}`);
  console.log(`  +${summary.added.length} terms, -${summary.removed.length} terms, ${summary.changed ? 'content changed' : 'no change'}`);
  for (const t of summary.added.slice(0, 10)) console.log(`  + ${t}`);
  for (const t of summary.removed.slice(0, 10)) console.log(`  - ${t}`);
  console.log(summary.written ? `  wrote ${out}` : flag('--dry-run') ? '  dry run, nothing written' : '  nothing to write');
}
