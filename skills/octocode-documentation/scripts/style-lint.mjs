#!/usr/bin/env node
/**
 * style-lint — deterministic Google developer documentation style checks for Markdown.
 * Heuristic by design: it finds candidates, the skill's references decide.
 *
 * Usage:
 *   node scripts/style-lint.mjs <paths...> [--json] [--strict] [--only ids] [--skip ids]
 *                                          [--max-per-rule N] [--list-rules] [--help]
 * Levels: ERROR gates (misleads readers or breaks accessibility), WARN is mechanical and high-precision,
 * INFO is heuristic and needs judgment (passive voice, serial comma, word list, sentence length).
 * One finding per rule per line; --max-per-rule caps findings per rule per file (default 20).
 * Exit codes: 0 clean or INFO/WARN only, 1 any ERROR (or ERROR+WARN with --strict), 2 bad usage.
 */
import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const skillRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const WORD_LIST_PATH = join(skillRoot, 'assets/google-word-list.tsv');

const argv = process.argv.slice(2);
const flag = (n) => argv.includes(n);
const val = (n, d) => { const i = argv.indexOf(n); return i >= 0 && argv[i + 1] ? argv[i + 1] : d; };

const PROPER = new Set(['Google', 'GitHub', 'Git', 'Docker', 'Kubernetes', 'Node', 'JavaScript', 'TypeScript',
  'Markdown', 'HTML', 'CSS', 'API', 'APIs', 'CLI', 'MCP', 'JSON', 'YAML', 'README', 'AGENTS', 'Claude',
  'Octocode', 'Cloud', 'Linux', 'macOS', 'Windows', 'Chrome', 'OAuth', 'Anthropic', 'Diátaxis', 'Skill', 'Skills']);

/** id, level, test(line) -> {col, quote} | null, message */
const RULES = [
  ['missing-alt-text', 'ERROR', /!\[\s*\]\(/, 'Image has empty alt text. Describe the image, or mark it decorative in HTML with alt="".'],
  ['vague-link-text', 'ERROR', /\[\s*(click here|here|this|this (page|doc|document|link|guide)|read (this|more)|learn more|more|link)\s*\]\(/i, 'Link text must describe the destination — use the target page title or a descriptive phrase.'],
  ['bare-url-link-text', 'ERROR', /\[\s*https?:\/\//i, 'Link text is a raw URL. Use the page title or a descriptive phrase.'],
  ['non-inclusive-term', 'ERROR', /\b(black ?list(s|ed|ing)?|white ?list(s|ed|ing)?|gray ?list(s|ed|ing)?|slaves?|sanity check|dummy (value|variable|data)|man ?hours|mankind|chairman|grandfathered|crazy|insane|lunatic|ninjas?|rockstars?|gurus?|cripple[sd]?|you guys|guys\b|he\/she|his\/her|s\/he|master ?\/ ?slave)/i, 'Non-inclusive term. See the replacement table in references/style-global.md.'],
  ['pre-announcement', 'ERROR', /\b(coming soon|in a future (release|version)|we plan to|planned for a future|on our roadmap|will be (released|available) soon)\b/i, 'Do not document or hint at unreleased features (references/style-claims.md).'],
  ['master-term', 'WARN', /\bmaster\b/i, 'Prefer primary, main, parent, or controller; keep "master" only when it is a fixed code or Git identifier.'],
  ['unsupported-claim', 'WARN', /\b(the (best|fastest|easiest|simplest|most secure)|blazing[- ]fast|100% (secure|reliable)|guarantee[sd]?\b|never fails|always works)\b/i, 'Excessive or unverifiable claim. State the mechanism and cite data, or cut it.'],
  ['title-case-heading', 'WARN', null, 'Headings use sentence case: capitalize the first word and proper nouns only.'],
  ['heading-end-period', 'WARN', /^#{1,6}\s+.*[.]\s*$/, 'Remove the period at the end of a heading.'],
  ['heading-gerund', 'WARN', /^#{1,6}\s+(?!(?:String|Thing|Ring|Spring|King|Wing|Sibling|Ceiling|Setting|Mapping|During)\b)[A-Z][a-z]+ing\s+\S/, 'Task headings take the bare infinitive ("Create an instance"), concept headings a noun phrase — unless no better alternative exists ("Billing", "Pricing").'],
  ['heading-level-skip', 'WARN', null, 'Heading levels must not skip (h2 to h4).'],
  ['multiple-h1', 'WARN', null, 'One H1 per page.'],
  ['first-person', 'WARN', /\b(?:we|our|let's|let us)\b(?!\s+(?:don't\s+)?recommend)/i, 'Address the reader as "you"; "we" is only the organization as author ("we recommend" is fine).'],
  ['the-user', 'WARN', /\bthe user\b/i, 'Don\'t call the reader "the user" — use "you", or name the software\'s actor.'],
  ['future-tense', 'WARN', /\b(will|won't|would|wouldn't|shall)\b/i, 'Use present tense for current behavior; future tense only for genuinely later events.'],
  ['passive-voice', 'INFO', /\b(is|are|was|were|be|been|being)\s+(\w+ed|built|done|made|given|shown|sent|written|held|kept|read|set|run)\b(\s+by\b)?/i, 'Possible passive voice — make the doer the subject unless the object is the point.'],
  ['latin-abbreviation', 'WARN', /\b(e\.g\.|i\.e\.|viz\.|N\.B\.)/i, 'Write "for example" or "that is". (`etc.` is acceptable in some lists, so it is not flagged here.)'],
  ['prefer-plain-word', 'INFO', /\b(via|utilize[sd]?|leverage[sd]?|allows? (you|users?) to|in order to|ingest(s|ed)?|abort(s|ed)?|deselect|hit the|access the (console|page|file))\b/i, 'Plainer word available (references/style-words.md).'],
  ['filler-word', 'WARN', /\b(just|simply|simple|easy|easily|obviously|of course|please note|note that|basically|actually)\b/i, 'Cut the filler; never tell the reader a task is easy.'],
  ['please', 'WARN', /\bplease\b/i, 'Drop "please" from instructions and cross-references.'],
  ['time-anchored', 'WARN', /\b(currently|at present|as of (this|the time of) writing|for now|right now|newly|recently|eventually|soon|latest version|nowadays)\b/i, 'Timeless docs describe the product as it is; name a release if a date matters.'],
  ['directional-reference', 'WARN', /\b(the (table|image|figure|screenshot|section|diagram|list|example) (above|below)|see (above|below)|left[- ]hand|right[- ]hand|as you can see|the panel on the (left|right))\b/i, 'Replace direction with the element name, "preceding", or "the following".'],
  ['ambiguous-modal', 'INFO', /\b(should|may)\b/i, 'Prefer must / we recommend / can / might. `should` is allowed for a generally recognized recommendation; `may` for policy and legal text.'],
  ['visual-ui-reference', 'WARN', /\bclick the ([a-z]+ )?(icon|arrow|gear|hamburger|three dots|kebab|zippy)\b/i, 'Name the UI element from its label or tooltip, not its shape.'],
  ['optional-parenthetical', 'WARN', /\(\s*Optional\s*\)/, 'Optional steps start with "Optional:".'],
  ['ambiguous-date', 'WARN', /\b\d{1,2}\/\d{1,2}\/\d{2,4}\b/, 'Use "January 19, 2017" or ISO 8601 — numeric dates are read differently by region.'],
  ['missing-serial-comma', 'INFO', /\w+,\s+(?:[\w'’()-]+\s+){0,3}[\w'’()-]+\s+(and|or)\s+[\w'’(]/, 'Check for the serial comma before the final "and" or "or".'],
  ['exclamation-mark', 'WARN', /(?<![!=<>])!(?![=!])(\s|$)/, 'Drop the exclamation mark; concept and reference docs never use one (code operators and quoted literals are fine).'],
  ['parenthetical-plural', 'WARN', /\b[a-z]{3,}\(s\)|\(ren\)/i, 'Never put an optional plural in parentheses; pick one form or write "one or more".'],
  ['example-punctuation', 'INFO', /,\s*for example,\s*[^.;]*[.;]\s*$|;\s*for example[,:]/i, 'End-of-sentence examples take "such as", "like", or an em dash — not a comma-fenced or semicolon-introduced "for example".'],
  ['ui-label-quotes', 'WARN', /\b(click|select|choose|press)\s+(the\s+)?["“][^"”]{2,40}["”]/i, 'UI labels are bold, never quoted.'],
  ['metasyntactic-name', 'WARN', /\b(foo|bar|baz|foobar|qux)\b/i, 'Use meaningful placeholder names (references/style-code.md).'],
  ['placeholder-style', 'WARN', /\b(MY_[A-Z0-9_]{2,}|YOUR_[A-Z0-9_]{2,})\b|<[A-Z0-9_]{3,}>/, 'Placeholders are UPPERCASE_WITH_UNDERSCORES; no possessive MY_/YOUR_ prefix, and no brackets inside the placeholder itself.'],
  ['ampersand', 'WARN', /\s&\s/, 'Write "and" unless the UI label itself uses "&".'],
  ['long-sentence', 'INFO', null, 'Sentence is over 35 words — split it.'],
  ['word-list', 'INFO', null, `Term the guide says to avoid; guidance comes from assets/google-word-list.tsv (${existsSync(WORD_LIST_PATH) ? 'loaded' : 'MISSING'}).`],
];

/** dont-use / avoid entries from the scraped word list, longest term first. */
const WORD_LIST_STOP = new Set(['with', 'like', 'if', 'then', 'this', 'that', 'once', 'since', 'while', 'above',
  'below', 'over', 'under', 'and', 'or', 'but', 'we', 'you', 'it', 'its', 'they', 'them', 'there', 'here', 'up',
  'down', 'out', 'off', 'all', 'any', 'some', 'more', 'most', 'less', 'few', 'very', 'just', 'now', 'new', 'old',
  'only', 'also', 'may', 'use', 'using', 'used', 'one', 'two', 'set', 'get', 'run', 'see', 'time', 'times', 'want', 'path', 'check', 'possible',
  // ordinary technical vocabulary the guide qualifies rather than bans
  'cloud', 'console', 'higher', 'lower', 'listed', 'scale', 'functionality', 'compliance', 'healthy', 'health check',
  'firewalls', 'reservation', 'regex', 'pros', 'cons', 'comprise', 'text box', 'primitive', 'fat', 'blind', 'select',
  'click', 'press', 'enter', 'type', 'field', 'box', 'window', 'page', 'section', 'pane', 'panel', 'command', 'output',
  // owned by a dedicated rule above — flagging twice reports one problem as two
  'master', 'slave', 'please', "let's", 'we', 'our', 'us', 'via', 'in order to', 'leverage', 'utilize', 'allows you to',
  'abort', 'currently', 'soon', 'latest', 'eventually', 'at present', 'presently', 'future', 'in the future', 'now',
  'new', 'newer', 'older', 'recently', 'click here', 'sanity check', 'blacklist', 'whitelist', 'graylist', 'cripple',
  'man hours', 'mankind', 'mom test', 'grandma test', 'guys', 'you guys', 'crazy', 'insane', 'lunatic', 'ninja',
  'grandfathered', 'dummy variable', 'hamburger', 'kebab', 'kabob', 'e.g.', 'i.e.', 'etc.', 'just', 'simply', 'easy',
  'deselect', 'hit', 'foo', 'bar', 'baz', 'hang', 'native', 'first-class citizen', 'and/or', 'postmortem',
  'as of this writing', 'he/she', 'his/her']);

function loadWordList() {
  if (!existsSync(WORD_LIST_PATH)) return { re: null, guidance: new Map() };
  const guidance = new Map();
  for (const line of readFileSync(WORD_LIST_PATH, 'utf8').split('\n')) {
    if (!line || line.startsWith('#')) continue;
    const [term, verdict, text] = line.split('\t');
    if (!term || !['dont-use', 'avoid'].includes(verdict)) continue;
    const firstSentence = (text || '').split(/(?<=\.)\s/)[0] || '';
    // Conditional entries ("Don't use X when…", "OK to use…") are judgment calls, not lint hits.
    if (/ when | unless | except |\bsense\b| to refer to | as a | as an | generically | in isolation | to mean |\bstandalone\b/i.test(firstSentence)
      || /OK to use/i.test(text || '')) continue;
    if (/\((?:verb|noun|adjective)\)/i.test(term)) continue; // part-of-speech rules need judgment, not a regex
    for (const variant of term.split(',')) {
      const t = variant.replace(/\([^)]*\)/g, '').trim().toLowerCase();
      if (t.length < 3 || / versus /.test(t) || WORD_LIST_STOP.has(t) || !/^[a-z][a-z0-9'’\- ]*$/.test(t)) continue;
      if (!guidance.has(t)) guidance.set(t, (text || '').trim());
    }
  }
  const terms = [...guidance.keys()].sort((a, b) => b.length - a.length).map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
  return { re: terms.length ? new RegExp(`\\b(${terms.join('|')})\\b`, 'i') : null, guidance };
}
const WORD_LIST = loadWordList();

if (flag('--help') || (!argv.length && !flag('--self-test'))) {
  console.log(`style-lint — Google developer documentation style checks for Markdown

  node scripts/style-lint.mjs <paths...> [options]

  --json              machine-readable findings
  --strict            exit 1 on WARN as well as ERROR (INFO never fails)
  --only ids          comma-separated rule ids to run
  --skip ids          comma-separated rule ids to ignore
  --max-per-rule N    cap findings per rule per file (default 20; truncation is reported)
  --list-rules        print rule ids and levels (word-list rule needs assets/google-word-list.tsv)
  --self-test         lint built-in good/bad fixtures; fails if a rule stops firing

A file containing "<!-- style-lint: ignore-file -->" is skipped when found by directory
recursion, and still linted when you name it directly (fixtures, deliberate examples).
  --help              this text

Examples:
  node scripts/style-lint.mjs README.md docs/
  node scripts/style-lint.mjs docs/ --only vague-link-text,title-case-heading --json`);
  process.exit(argv.length ? 0 : 2);
}
if (flag('--list-rules')) {
  for (const [id, level, , msg] of RULES) console.log(`${level.padEnd(5)} ${id.padEnd(24)} ${msg}`);
  process.exit(0);
}

/** Rules about token shape must see code spans, which masking would hide. */
const RAW_RULES = new Set(['placeholder-style', 'metasyntactic-name', 'ambiguous-date', 'bare-url-link-text']);

const only = val('--only', '').split(',').filter(Boolean);
const skip = val('--skip', '').split(',').filter(Boolean);
const maxPerRule = Number(val('--max-per-rule', '20'));
const paths = argv.filter((a, i) => !a.startsWith('--') && !['--only', '--skip', '--max-per-rule'].includes(argv[i - 1]));

/** Explicit file arguments are always linted; discovered files may opt out with the ignore marker. */
function collect(p, explicit = true) {
  if (!existsSync(p)) { console.error(`style-lint: no such path: ${p}`); process.exit(2); }
  const st = statSync(p);
  if (st.isFile()) return /\.(md|mdx|markdown)$/i.test(p) ? [{ file: p, explicit }] : [];
  return readdirSync(p).flatMap((n) => (['node_modules', '.git', 'dist', 'build'].includes(n) ? [] : collect(join(p, n), false)));
}

const IGNORE_MARKER = '<!-- style-lint: ignore-file -->';

/** Blank out code fences, inline code, link targets, and frontmatter, preserving offsets. */
function mask(text) {
  const raw = text.split('\n');
  const blank = (s) => ' '.repeat(s.length);
  let inFence = false;
  let inFront = raw[0] === '---';
  return raw.map((line, i) => {
    if (inFront) { if (i > 0 && line === '---') inFront = false; return blank(line); }
    if (/^\s*(```|~~~)/.test(line)) { inFence = !inFence; return blank(line); }
    if (inFence) return blank(line);
    return line
      .replace(/`[^`]*`/g, blank)
      .replace(/\]\(([^)]*)\)/g, (m, target) => `](${' '.repeat(target.length)})`)
      .replace(/https?:\/\/\S+/g, blank);
  });
}

function headingWords(line) {
  return line.replace(/^#{1,6}\s+/, '').replace(/[*_]/g, '').trim().split(/\s+/);
}

function lintFile(file, explicit) {
  const text = readFileSync(file, 'utf8');
  if (!explicit && text.includes(IGNORE_MARKER)) return { findings: [], truncated: [], skipped: true };
  return lintText(text, file);
}

function lintText(text, file) {
  const rawLines = text.split('\n');
  const lines = mask(text);
  const findings = [];
  const counts = new Map();
  const push = (id, level, lineNo, col, quote, message) => {
    const key = id;
    const n = (counts.get(key) || 0) + 1;
    counts.set(key, n);
    if (n > maxPerRule) return;
    findings.push({ file, line: lineNo, col, rule: id, level, message, quote });
  };
  const enabled = ([id]) => (!only.length || only.includes(id)) && !skip.includes(id);

  let h1 = 0;
  let prevLevel = 0;
  lines.forEach((line, idx) => {
    const lineNo = idx + 1;
    const heading = line.match(/^(#{1,6})\s+\S/);
    if (heading) {
      const level = heading[1].length;
      if (level === 1 && ++h1 > 1 && enabled(['multiple-h1'])) push('multiple-h1', 'WARN', lineNo, 1, line.trim(), 'One H1 per page.');
      if (prevLevel && level > prevLevel + 1 && enabled(['heading-level-skip'])) push('heading-level-skip', 'WARN', lineNo, 1, line.trim(), `Heading jumps h${prevLevel} to h${level}.`);
      prevLevel = level;
      const words = headingWords(line);
      if (words.length >= 3 && enabled(['title-case-heading'])) {
        const capped = words.slice(1).filter((w) => /^[A-Z][a-z]{2,}$/.test(w) && !PROPER.has(w));
        if (capped.length >= 2) push('title-case-heading', 'WARN', lineNo, 1, line.trim(), `Sentence case: lowercase ${capped.slice(0, 3).join(', ')}.`);
      }
    }
    for (const [id, level, re, message] of RULES) {
      if (!re || !enabled([id])) continue;
      if ((id.startsWith('heading-') || id === 'multiple-h1') && !heading) continue;
      const target = RAW_RULES.has(id) ? (rawLines[idx] || '') : line;
      const m = target.match(re);
      if (m) push(id, level, lineNo, (m.index ?? 0) + 1, (m[0] || '').trim() || target.trim(), message);
    }
    if (WORD_LIST.re && enabled(['word-list'])) {
      const hit = line.match(WORD_LIST.re);
      if (hit) {
        const term = hit[0].toLowerCase();
        const note = WORD_LIST.guidance.get(term) || '';
        push('word-list', 'INFO', lineNo, (hit.index ?? 0) + 1, hit[0], `"${term}": ${note.slice(0, 180)}`.trim());
      }
    }
    if (enabled(['long-sentence']) && !heading && !/^\s*[|>-]/.test(line)) {
      for (const s of line.split(/(?<=[.!?])\s+/)) {
        const n = s.trim().split(/\s+/).filter(Boolean).length;
        if (n > 35) push('long-sentence', 'INFO', lineNo, 1, `${s.trim().slice(0, 60)}…`, `Sentence is ${n} words — split it.`);
      }
    }
  });
  const truncated = [...counts].filter(([, n]) => n > maxPerRule).map(([id, n]) => ({ rule: id, total: n, shown: maxPerRule }));
  return { findings, truncated };
}

/** Regression check: the rules must fire on known-bad text and stay quiet on known-good text. */
const DIRTY_FIXTURE = `# Setting Up The Widget Service

We will show you how to easily configure the widget service. Please note that the
whitelist is currently supported, and a dashboard is coming soon.

#### Deeply nested heading.

To learn more, click [here](https://example.com) or see [https://example.com/docs](https://example.com/docs).

![](diagram.png)

1. (Optional) Type a name, e.g. \`wsfc-1\`.
2. Click the gear icon in the panel on the right for values valid on 01/19/17.

Set \`MY_PROJECT_ID\` before the user runs the fastest sync in the industry.
Delete the API key(s) and click the "Next" button, for example, in the console.
The release ships in a future release!
`;

const CLEAN_FIXTURE = `# Configure the widget service

The widget service stores one document per tenant. To configure it, you set a
region and a retention window.

## Before you begin

Enable the API and note your project ID. Replace \`PROJECT_ID\` with your project ID.

## Configure a tenant

1. In the **Region** field, enter \`us-central1\`.
2. Optional: Enter a description of up to 120 characters.
3. Click **Save**. The tenant appears in the list within a few seconds.

For more information about retention windows, see
[Retention policy reference](https://example.com/retention).

![Request flow from the client through the proxy to the widget service](flow.svg)
`;

const EXPECTED = ['missing-alt-text', 'vague-link-text', 'bare-url-link-text', 'non-inclusive-term',
  'pre-announcement', 'title-case-heading', 'heading-end-period', 'heading-level-skip', 'first-person',
  'future-tense', 'please', 'time-anchored', 'ambiguous-date', 'optional-parenthetical', 'latin-abbreviation',
  'visual-ui-reference', 'placeholder-style', 'the-user', 'unsupported-claim', 'parenthetical-plural',
  'example-punctuation', 'ui-label-quotes', 'exclamation-mark', 'word-list'];

function selfTest() {
  const dirty = lintText(DIRTY_FIXTURE, '<dirty-fixture>');
  const clean = lintText(CLEAN_FIXTURE, '<clean-fixture>');
  const fired = new Set(dirty.findings.map((f) => f.rule));
  const missed = EXPECTED.filter((id) => !fired.has(id));
  const cleanErrors = clean.findings.filter((f) => f.level === 'ERROR');
  const cleanWarns = clean.findings.filter((f) => f.level === 'WARN');
  const errorRules = RULES.filter(([, level]) => level === 'ERROR').map(([id]) => id);
  const inertGates = errorRules.filter((id) => !fired.has(id));
  const checks = [
    { name: `dirty fixture fires ${EXPECTED.length} rules`, pass: missed.length === 0, detail: missed.join(',') },
    { name: 'every ERROR rule can fire', pass: inertGates.length === 0, detail: inertGates.join(',') },
    { name: 'clean fixture has 0 ERROR', pass: cleanErrors.length === 0, detail: cleanErrors.map((f) => f.rule).join(',') },
    { name: 'clean fixture has 0 WARN', pass: cleanWarns.length === 0, detail: cleanWarns.map((f) => f.rule).join(',') },
    { name: 'word-list data loaded', pass: Boolean(WORD_LIST.re) },
  ];
  const pass = checks.every((c) => c.pass);
  if (flag('--json')) console.log(JSON.stringify({ id: 'style-lint-self-test', pass, checks }, null, 2));
  else {
    console.log(`${pass ? 'PASS' : 'FAIL'} style-lint self-test (${dirty.findings.length} findings on the dirty fixture)`);
    for (const c of checks) console.log(`  ${c.pass ? '✓' : '✗'} ${c.name}${c.detail ? ` — ${c.detail}` : ''}`);
  }
  process.exit(pass ? 0 : 1);
}

if (flag('--self-test')) selfTest();

const targets = paths.flatMap((p) => collect(p));
const all = [];
const truncations = [];
const skipped = [];
for (const { file, explicit } of targets) {
  const { findings, truncated, skipped: off } = lintFile(file, explicit);
  if (off) { skipped.push(file); continue; }
  all.push(...findings);
  truncations.push(...truncated.map((t) => ({ file, ...t })));
}
const files = targets.filter((t) => !skipped.includes(t.file));
const errors = all.filter((f) => f.level === 'ERROR');
const warns = all.filter((f) => f.level === 'WARN');
const infos = all.filter((f) => f.level === 'INFO');

if (flag('--json')) {
  console.log(JSON.stringify({ files: files.length, skipped, errorCount: errors.length, warnCount: warns.length, infoCount: infos.length, truncations, findings: all }, null, 2));
} else {
  const cwd = process.cwd();
  for (const f of all) {
    console.log(`${relative(cwd, f.file) || f.file}:${f.line}:${f.col}  ${f.level}  ${f.rule}`);
    console.log(`  ${f.quote}`);
    console.log(`  → ${f.message}`);
  }
  for (const t of truncations) console.log(`note: ${relative(cwd, t.file)} ${t.rule} had ${t.total} hits; showed ${t.shown} (raise --max-per-rule)`);
  for (const f of skipped) console.log(`note: skipped ${relative(cwd, f)} (ignore-file marker)`);
  console.log(`\nstyle-lint: ${files.length} file(s), ${errors.length} ERROR, ${warns.length} WARN, ${infos.length} INFO`);
  if (!all.length) console.log('No style findings. Heuristics only — judgment rules still apply.');
}
process.exit(errors.length || (flag('--strict') && (errors.length + warns.length)) ? 1 : 0);
