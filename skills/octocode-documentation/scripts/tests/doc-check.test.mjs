import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  CHECKS, VERDICTS, checkDocs, formatResult, githubSlug, loadWordList, lookupWords, namedForms, parseArgs,
  parseMarkdown, renderHeading, scopeOf, titleCaseWords,
} from '../doc-check.mjs';

const SCRIPT = fileURLToPath(new URL('../doc-check.mjs', import.meta.url));
const FENCE = '```';

/** A throwaway repository (a .git marker) holding the given files; removed when the test ends. */
function fixture(t, files) {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'doc-check-')));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, '.git'));
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), text);
  }
  return root;
}

function run(args, cwd) {
  const child = spawnSync(process.execPath, [SCRIPT, ...args], { cwd, encoding: 'utf8', env: { PATH: process.env.PATH } });
  let json = null;
  try { json = JSON.parse(child.stdout); } catch { /* --help prints text */ }
  return { status: child.status, stdout: child.stdout, json };
}

const rules = (list) => list.map((item) => item.rule);

test('GitHub slugs: punctuation dropped, code and link text kept, spaces become hyphens', () => {
  assert.equal(githubSlug(renderHeading('Voice and tone ([tone](https://example.com/t), [voice](https://example.com/v))')), 'voice-and-tone-tone-voice');
  assert.equal(githubSlug(renderHeading('The `--renamed` option')), 'the---renamed-option');
  assert.equal(githubSlug(renderHeading('**Bold** _and_ snake_case')), 'bold-and-snake_case');
  assert.equal(githubSlug(renderHeading('Émoji 🎉 test')), 'émoji--test');
  assert.equal(githubSlug('already-a-slug'), 'already-a-slug');
  assert.equal(githubSlug(renderHeading('kubectl delete: < v1.4.0 client vs >=v1.4.0 <em>cluster</em>')), 'kubectl-delete--v140-client-vs-v140-cluster');
});

test('heading text drops each HTML tag once, as GitHub renders it: text that only looks like a tag stays', () => {
  for (const [heading, shown] of [
    ['Use Vec<Option<T>> here', 'Use Vec<Option> here'],
    ['Map<string, Array<T>>', 'Map<string, Array>'],
    ['<b>Bold</b> and <span class="x">span</span>', 'Bold and span'],
    ['<a href="#x">Link</a> <br/> end', 'Link  end'],
    ['a <scr<b>ipt> b', 'a <script> b'],
    ['a < b > c', 'a < b > c'],
    ['x<y and <1> and </>', 'x<y and <1> and </>'],
    ['unclosed <div', 'unclosed <div'],
    ['<<b>>', '<>'],
  ]) assert.equal(renderHeading(heading), shown, heading);
});

test('the prose model blanks code, comments and front matter without moving offsets', () => {
  const text = `---\ntitle: X\n---\n# Title\n\nSee \`[a](b.md)\` and <!-- [c](d.md) -->\n\n${FENCE}md\n[e](f.md)\n${FENCE}\n`;
  const doc = parseMarkdown(text);
  assert.equal(doc.prose.length, text.length);
  assert.equal(doc.links.length, 0);
  assert.equal(doc.hasTitle, true);
  assert.deepEqual(doc.headings.map((heading) => heading.raw), ['Title']);
  assert.deepEqual(doc.spans.map((span) => span.text), ['[a](b.md)']);
  const unclosed = parseMarkdown(`# T\n\n- item\n  ${FENCE}sh\n  echo\n- next item\n\n## After\n`);
  assert.deepEqual(unclosed.headings.map((heading) => heading.raw), ['T', 'After'], 'a list item ends its unclosed fence');
  assert.ok(parseMarkdown('# `kubectl debug` graduates\n').anchors.has('kubectl-debug-graduates'), 'a heading may start with code');
});

test('links: missing files, letter case, undefined references; code, comments and URLs are not links', (t) => {
  const root = fixture(t, {
    'docs/intro.md': '# Intro\n\n## Start here\n\nText.\n',
    'docs/guide.md': [
      '# Guide', '', '## Set up the tool', '',
      'See [intro](intro.md), [missing](nope.md), [case](INTRO.md), [ok](intro.md#start-here), [bad](intro.md#Start-Here),',
      '[self](#set-up-the-tool), [dup](#notes-1), [angle](<intro.md#start-here>), [web](https://example.com) and ![img](img/none.png).',
      '[ref link][known] and [broken ref][unknown].', '',
      '[known]: intro.md#nowhere', '',
      '`[code](skip.md)` and', '', `${FENCE}md`, '[fenced](skip.md)', FENCE, '<!-- [comment](skip.md) -->',
      '<a href="gone.md">old</a>', '', '## Notes', '', '## Notes', '',
    ].join('\n'),
  });
  const result = checkDocs({ paths: ['docs/guide.md'], checks: ['links', 'anchors'], cwd: root });
  const caseReason = existsSync(join(root, 'docs/INTRO.md')) ? 'case-mismatch' : 'missing';
  assert.deepEqual(result.brokenLinks.map(({ link, reason }) => [link, reason]), [
    ['nope.md', 'missing'], ['INTRO.md', caseReason], ['img/none.png', 'missing'], ['[unknown]', 'undefined-reference'], ['gone.md', 'missing'],
  ]);
  assert.deepEqual(result.missingAnchors.map(({ link, suggestion }) => [link, suggestion]), [['intro.md#Start-Here', 'start-here'], ['intro.md#nowhere', undefined]]);
  assert.equal(result.counts.externalLinks, 1);
  assert.ok(!JSON.stringify(result).includes('skip.md'));
  assert.equal(result.brokenLinks.every((item) => item.file === 'docs/guide.md' && item.line > 0), true);
  assert.deepEqual(result.checks, ['links', 'anchors']);
  assert.equal('headingIssues' in result, false);
  assert.equal(result.ok, false);
});

test('headings: H1 count, skipped levels, end punctuation and title case beside proper nouns', (t) => {
  const root = fixture(t, {
    'a.md': 'Intro line.\n\n# Getting Started\n\nUse Octocode here; it reads Markdown.\n\n### Deep Level.\n\n## Install Octocode\n\n# Second title\n',
    'b.md': 'No heading at all.\n',
    'c.md': '---\ntitle: Front matter title\n---\n\nBody.\n',
  });
  const result = checkDocs({ paths: ['.'], checks: ['headings'], cwd: root });
  const byFile = (file) => result.headingIssues.filter((issue) => issue.file === file);
  assert.deepEqual(rules(byFile('a.md')), ['multiple-h1', 'sentence-case', 'skipped-level', 'end-punctuation', 'sentence-case']);
  assert.deepEqual(byFile('a.md').filter((issue) => issue.rule === 'sentence-case').map((issue) => issue.words), [['Started'], ['Level']]);
  assert.deepEqual(rules(byFile('b.md')), ['missing-h1']);
  assert.deepEqual(byFile('c.md'), []);
  assert.deepEqual(titleCaseWords('Use the API: Next Steps', new Set()), ['Steps']);
});

test('word list: listed terms in prose with guidance; code, sense-only entries and style overlaps are skipped', (t) => {
  const root = fixture(t, {
    'w.md': '# Words\n\nUtilize the repo, then just click on Save. Keep `repo` in code.\nUse the CLI to check the box.\n',
  });
  const plain = checkDocs({ paths: ['w.md'], checks: ['words', 'style'], cwd: root });
  assert.deepEqual(plain.wordListHits.map(({ word, rule }) => [word, rule]), [['repo', 'dont-use'], ['just', 'avoid']]);
  assert.ok(plain.wordGuidance.repo.length > 0);
  assert.deepEqual(plain.styleIssues.filter((issue) => issue.rule === 'word-choice').map(({ text, replacement }) => [text, replacement]), [['Utilize', 'use'], ['click on', 'click']]);
  const senses = checkDocs({ paths: ['w.md'], checks: ['words'], senses: true, cwd: root });
  const sense = senses.wordListHits.filter((hit) => hit.scope === 'sense').map((hit) => hit.word);
  assert.ok(sense.includes('CLI') && sense.includes('check'));
  const all = checkDocs({ paths: ['w.md'], checks: ['words'], verdicts: [...VERDICTS], cwd: root });
  assert.ok(all.wordListHits.length >= plain.wordListHits.length);
});

test('style: filler, modal verbs, quotes, dates, times, link text, placeholders and example names', (t) => {
  const root = fixture(t, {
    'w.md': [
      '# Style', '',
      'Simply run it! You should see “quoted” output.',
      'Meet on 01/19/17 at 3pm or at 3 PM. Read [here](https://example.com) or [the setup guide](https://example.com).',
      'Set `MY_TOKEN`, then `foo`. Charts accept `bar`.', '',
      `${FENCE}bash`, 'export NAME=bar foo', FENCE, '',
    ].join('\n'),
  });
  const result = checkDocs({ paths: ['w.md'], checks: ['style'], cwd: root });
  const found = result.styleIssues.map(({ rule, text }) => `${rule}:${text}`);
  for (const expected of ['filler:Simply', 'filler:!', 'should-may:should', 'curly-quote:“', 'curly-quote:”', 'date-format:01/19/17', 'time-format:3pm', 'vague-link-text:here', 'placeholder-prefix:MY_TOKEN', 'example-name:foo']) {
    assert.ok(found.includes(expected), `${expected} in ${found.join(', ')}`);
  }
  assert.equal(found.filter((item) => item === 'example-name:bar').length, 1, 'bar counts only beside foo');
  assert.ok(!found.some((item) => item.includes('3 PM') || item.includes('setup guide')));
  for (const rule of new Set(rules(result.styleIssues))) assert.ok(result.styleGuidance[rule], rule);
});

test('commands: missing scripts and package scripts; skill folders, cd, inline code and code flags resolve', (t) => {
  const root = fixture(t, {
    'package.json': JSON.stringify({ scripts: { build: 'true' } }),
    'skill/SKILL.md': '---\nname: x\n---\n# X\n',
    'skill/scripts/run.mjs': '',
    'skill/docs/sub/local.mjs': '',
    'skill/docs/use.md': [
      '# Use', '', `${FENCE}bash`,
      'node <skill-folder>/scripts/run.mjs --help',
      'node scripts/missing.mjs',
      'node scripts/run.mjs --from-skill-root',
      'node -e "require(\'./nope.js\')"',
      'cd sub && node local.mjs',
      'npm run build && npm run nope',
      'yarn run -T oxlint',
      'bash -lc "./never/run.sh"',
      FENCE, '', 'Run `node scripts/also-missing.mjs` here.', '',
    ].join('\n'),
  });
  const result = checkDocs({ paths: ['skill/docs/use.md'], checks: ['commands'], cwd: root });
  assert.deepEqual(result.missingCommands.map(({ missing, reason }) => [missing, reason]), [
    ['scripts/missing.mjs', 'missing-file'], ['nope', 'missing-script'], ['scripts/also-missing.mjs', 'missing-file'],
  ]);
  assert.deepEqual(result.missingCommands.find((item) => item.reason === 'missing-script').packageJson, ['package.json']);
});

test('commands resolve from the package above the file and from declared workspaces', (t) => {
  const root = fixture(t, {
    'package.json': JSON.stringify({ workspaces: ['packages/*'] }),
    'packages/native/package.json': JSON.stringify({ scripts: { deny: 'true' } }),
    'packages/native/scripts/pack.cjs': '',
    'packages/native/docs/publish.md': `# Publish\n\n${FENCE}bash\nnode scripts/pack.cjs --out dist\nyarn deny\nyarn absent\n${FENCE}\n\nThe \`./schema\` entry is a path.\n`,
  });
  const result = checkDocs({ paths: ['packages'], checks: ['commands'], cwd: root });
  assert.deepEqual(result.missingCommands.map(({ missing, reason }) => [missing, reason]), [['absent', 'missing-script']]);
  assert.deepEqual(result.missingCommands[0].packageJson, ['packages/native/package.json', 'package.json']);
});

const renameFiles = {
  'docs/a.md': '# A\n\n## New name\n\nBody.\n',
  'docs/new.md': '# New\n\n## X\n',
  'docs/b.md': '# B\n\nSee [old](a.md#old-name) and [moved](./old.md#x).\n',
  'guide/c.md': '# C\n\n[up](../docs/old.md) and [section](/docs/a.md#old-name).\n',
};
const renames = ['docs/a.md#Old name=New name', 'docs/old.md=docs/new.md'];

test('renames: inbound links to a renamed heading or file get a replacement instead of a broken-link finding', (t) => {
  const root = fixture(t, renameFiles);
  const result = checkDocs({ paths: ['.'], renamed: renames, checks: ['links', 'anchors'], cwd: root });
  assert.deepEqual(result.staleInboundAnchors.map(({ file, link, replacement, replacementOk }) => [file, link, replacement, replacementOk]), [
    ['docs/b.md', 'a.md#old-name', 'a.md#new-name', true],
    ['docs/b.md', './old.md#x', './new.md#x', true],
    ['guide/c.md', '../docs/old.md', '../docs/new.md', true],
    ['guide/c.md', '/docs/a.md#old-name', '/docs/a.md#new-name', true],
  ]);
  assert.deepEqual([result.brokenLinks, result.missingAnchors], [[], []]);
  assert.equal(result.ok, false);
});

test('--write rewrites resolvable stale links only, then reports clean', (t) => {
  const root = fixture(t, { ...renameFiles, 'docs/d.md': '# D\n\n[ghost](a.md#old-name)\n' });
  const ghost = run(['docs/d.md', '--renamed', 'docs/a.md#Old name=Ghost', '--write'], root);
  assert.equal(ghost.status, 1);
  assert.equal(ghost.json.staleInboundAnchors[0].replacementOk, false);
  assert.equal(readFileSync(join(root, 'docs/d.md'), 'utf8'), '# D\n\n[ghost](a.md#old-name)\n');
  const fixed = run(['docs/b.md', 'guide/c.md', '--checks', 'links,anchors', ...renames.flatMap((spec) => ['--renamed', spec]), '--write'], root);
  assert.equal(fixed.status, 0, fixed.stdout);
  assert.deepEqual(fixed.json.written, [{ file: 'docs/b.md', links: 2 }, { file: 'guide/c.md', links: 2 }]);
  assert.equal(readFileSync(join(root, 'docs/b.md'), 'utf8'), '# B\n\nSee [old](a.md#new-name) and [moved](./new.md#x).\n');
  assert.equal(readFileSync(join(root, 'guide/c.md'), 'utf8'), '# C\n\n[up](../docs/new.md) and [section](/docs/a.md#new-name).\n');
  const again = run(['docs/a.md', 'docs/b.md', 'docs/new.md', 'guide', '--checks', 'links,anchors'], root);
  assert.equal(again.status, 0, again.stdout);
});

test('word list data: every row parses with a known verdict and scope; named forms replace a preferred term', () => {
  const entries = loadWordList();
  assert.ok(entries.length > 0);
  assert.ok(entries.every((entry) => VERDICTS.includes(entry.verdict) && ['word', 'sense'].includes(entry.scope) && entry.variants.length));
  assert.equal(scopeOf('check', "Don't use to refer to marking a checkbox."), 'sense');
  assert.equal(scopeOf('repo', "Don't use. Instead, use repository."), 'word');
  assert.equal(scopeOf('access (verb)', 'Avoid when you can.'), 'sense');
  assert.deepEqual(namedForms(['path'], 'Avoid using filepath, file path, pathname, or path name if possible.'), ['filepath', 'file path', 'pathname', 'path name']);
  assert.equal(namedForms(['slice and dice'], "Don't use the phrase slice and dice."), null);
  assert.equal(namedForms(['man hours'], 'Avoid using gendered terms.'), null);
});

test('lookup returns the entries that name a word or phrase', () => {
  const { lookup } = lookupWords(['repo', 'file path', 'zzzz']);
  assert.equal(lookup[0].entries[0].term, 'repo');
  assert.ok(lookup[1].entries.some((entry) => entry.term === 'path'));
  assert.deepEqual(lookup[2].entries, []);
});

test('arguments: options, lists and inline values parse; bad values fail as input errors', () => {
  const parsed = parseArgs(['docs', '--checks=links,words', '--verdicts', 'all', '--renamed', 'a.md=b.md', '--senses']);
  assert.deepEqual([parsed.paths, parsed.checks, parsed.verdicts, parsed.renamed, parsed.senses], [['docs'], ['links', 'words'], VERDICTS, ['a.md=b.md'], true]);
  assert.deepEqual(parseArgs(['x.md']).checks, CHECKS);
  assert.throws(() => parseArgs(['--checks', 'bogus']), { code: 'invalid-checks' });
  assert.throws(() => parseArgs(['--root']), { code: 'missing-value' });
});

test('CLI: help, exit codes and JSON errors', (t) => {
  const root = fixture(t, { 'ok.md': '# Clean page\n\nThe page links to [the intro](intro.md).\n', 'intro.md': '# Intro\n' });
  const help = run(['--help'], root);
  assert.equal(help.status, 0);
  for (const flag of ['--renamed', '--write', '--root', '--checks', '--verdicts', '--senses', '--lookup', 'Exit:']) assert.ok(help.stdout.includes(flag), flag);
  const clean = run(['ok.md'], root);
  assert.deepEqual([clean.status, clean.json.ok], [0, true]);
  for (const [args, code] of [[[], 'missing-input'], [['--bogus'], 'unknown-option'], [['gone.md'], 'path-not-found'], [['ok.md', '--renamed', 'nohash'], 'invalid-renamed'], [['ok.md', '--renamed', 'a.md=b.md#x'], 'invalid-renamed'], [['--lookup'], 'missing-input'], [['ok.md', '--root', 'gone'], 'invalid-root']]) {
    const failed = run(args, root);
    assert.deepEqual([failed.status, failed.json.ok, failed.json.error.code], [2, false, code], args.join(' '));
  }
  const lookup = run(['--lookup', 'utilize'], root);
  assert.deepEqual([lookup.status, lookup.json.lookup[0].entries[0].term], [0, 'utilize, utilization']);
});

test('CLI runs through a symlink and prints one finding per line', (t) => {
  const root = fixture(t, { 'bad.md': '# Bad\n\n[a](a.md) [b](b.md)\n' });
  const link = join(root, 'doc-check-link.mjs');
  symlinkSync(SCRIPT, link);
  const child = spawnSync(process.execPath, [link, 'bad.md', '--checks', 'links'], { cwd: root, encoding: 'utf8' });
  assert.equal(child.status, 1);
  const lines = child.stdout.split('\n').filter((line) => line.includes('"reason"'));
  assert.equal(lines.length, 2);
  const parsed = JSON.parse(child.stdout);
  assert.equal(formatResult(parsed), child.stdout.trimEnd());
});
