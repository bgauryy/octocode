#!/usr/bin/env node
// Structural checks for Agent Skill folders. Trigger quality and prose require human review.
import { existsSync, realpathSync, statSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync, mkdirSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { frontmatter, jsonDescriptionProblem } from './lib/frontmatter.mjs';

function filesUnder(root) {
  const files = [];
  function visit(dir) {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (entry.name === 'node_modules' || entry.name === '.git') continue;
      const full = join(dir, entry.name);
      if (entry.isDirectory()) visit(full);
      else if (entry.isFile() || entry.isSymbolicLink()) files.push(relative(root, full));
    }
  }
  visit(root);
  return files;
}

const lineAt = (text, index) => text.slice(0, index).split('\n').length;

// Every inline or reference-style link to a file, with the line it is on. A URL scheme (https:, mailto:)
// or a bare #anchor is not a file; a root-absolute /path is kept, flagged `rooted`.
function linkTargets(text) {
  const links = [];
  const pattern = /\]\(\s*(?:<([^>]+)>|([^\s)]+))(?:\s+["'][^\n]*?["'])?\s*\)|^\s*\[[^\]]+\]:\s*(?:<([^>]+)>|(\S+))/gm;
  for (const match of text.matchAll(pattern)) {
    const raw = match[1] || match[2] || match[3] || match[4];
    const target = raw.split('#')[0];
    if (!target || /^(?:[a-z][a-z0-9+.-]*:|\/\/)/i.test(target)) continue;
    let decoded = target;
    try { decoded = decodeURIComponent(target); } catch { /* keep the spelling as written */ }
    links.push({ target: decoded, rooted: target.startsWith('/'), line: lineAt(text, match.index + match[0].indexOf(raw)) });
  }
  return links;
}

function localPaths(text) {
  return new Set(linkTargets(text).filter((link) => !link.rooted).map((link) => link.target));
}

// GitHub-style heading slugs, so a renamed section breaks its #anchor links visibly.
function headingAnchors(text) {
  const anchors = new Set();
  const seen = new Map();
  const body = text.replace(/^(```|~~~)[^\n]*\n[\s\S]*?^\1[^\n]*$/gm, "");
  for (const match of body.matchAll(/^ {0,3}#{1,6}\s+(.+?)\s*#*\s*$/gm)) {
    const base = match[1].replace(/\[([^\]]*)\]\([^)]*\)/g, "$1").toLowerCase().replace(/[^\p{L}\p{M}\p{N}\p{Pc} -]/gu, "").replace(/ /g, "-");
    const count = seen.get(base) || 0;
    seen.set(base, count + 1);
    anchors.add(count ? `${base}-${count}` : base);
  }
  for (const match of text.matchAll(/<a\s+(?:name|id)=["']([^"']+)["']/gi)) anchors.add(match[1]);
  return anchors;
}

function anchorLinks(text) {
  const links = [];
  for (const match of text.matchAll(/\]\(\s*(?:<([^>]+)>|([^\s)]+))(?:\s+["'][^\n]*?["'])?\s*\)/g)) {
    const raw = match[1] || match[2];
    const hash = raw.indexOf("#");
    if (hash < 0) continue;
    const target = raw.slice(0, hash);
    if (/^(?:[a-z]+:|\/)/i.test(target) || /[<{*…]/.test(raw)) continue;
    links.push({ target, anchor: raw.slice(hash + 1) });
  }
  return links;
}

// Fenced blocks hold commands and examples, not Markdown links. Their lines stay as blank lines, so
// line numbers in the rest of the file still match the source.
function withoutFences(text) {
  return text.replace(/^ {0,3}(```|~~~)[^\n]*\n[\s\S]*?^ {0,3}\1[^\n]*$/gm, (block) => block.replace(/[^\n]/g, ''));
}

function supportPaths(text) {
  return [...text.matchAll(/(?:^|[^A-Za-z0-9./])((?:scripts|references|assets|docs|benchmarks)\/[A-Za-z0-9._/-]+\.(?:mjs|md|tsv|json|html|js|sh|py))/gm)].map((match) => match[1]);
}

const MAX_SKILL_LINES = 50;
// The same cap for every document a skill routes: a longer one is split by load condition, never packed.
const MAX_DOC_LINES = 50;

function checkDocLength({ texts, add }) {
  for (const [path, text] of texts) {
    if (path !== 'output.md' && !path.startsWith('references/')) continue;
    const lines = text.replace(/\n$/, '').split('\n').length;
    if (lines > MAX_DOC_LINES) add('WARN', 'doc-length', `${path} has ${lines} lines; keep each document at ${MAX_DOC_LINES} or fewer by splitting it by load condition.`, { file: path });
  }
}

function checkFrontmatter({ dir, name, skill, add }) {
  const fm = frontmatter(skill);
  if (!fm) {
    add('ERROR', 'frontmatter-missing', 'SKILL.md needs YAML frontmatter.');
    add('ERROR', 'description-missing', 'Description is required.');
    return;
  }
  if (fm.name === null || fm.description === null) add('WARN', 'yaml-unchecked', 'Advanced or invalid YAML: validate with a YAML-aware Agent Skills validator.');
  if (fm.name !== null && fm.name !== name) add('ERROR', 'name-mismatch', `Frontmatter name must match folder ${name}.`);
  if (fm.name && (fm.name.length > 64 || !/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(fm.name))) {
    add('ERROR', 'name-format', 'Name must follow the Agent Skills identifier syntax.');
  }
  if (fm.description !== null && !fm.description) add('ERROR', 'description-missing', 'Description is required.');
  if (basename(dirname(realpathSync(dir))) === 'skills') {
    const problem = jsonDescriptionProblem(skill);
    if (problem) add('ERROR', 'description-json', problem);
  }
  if (fm.description && [...fm.description].length > 1024) add('ERROR', 'description-length', 'Description exceeds the Agent Skills limit of 1024 characters (code points).');
  const lines = skill.replace(/\n$/, '').split('\n').length;
  if (lines > MAX_SKILL_LINES) add('WARN', 'skill-length', `SKILL.md has ${lines} lines; keep it at ${MAX_SKILL_LINES} or fewer and move branch detail into references.`);
}

function checkLayout({ dir, files, texts, skill, add }) {
  // Result contracts may stay inline; route a separate output guide when one exists.
  if (files.includes('output.md') && ![...localPaths(skill)].some((path) => resolve(dir, path) === join(dir, 'output.md'))) add('WARN', 'output-route-missing', 'Link the output format from SKILL.md using any descriptive label.');
  const common = resolve(dir, '..', 'common');
  for (const resource of ['scripts/render-report.mjs', 'references/report-output.md']) {
    const source = join(common, resource), installed = join(dir, resource);
    if (existsSync(installed) && existsSync(source) && !readFileSync(installed).equals(readFileSync(source))) add('ERROR', 'shared-file-drift', `${resource} differs from skills/common; run node skills/common/build.mjs.`);
  }
  if (!existsSync(join(dir, 'scripts', 'render-report.mjs')) && [...texts.values()].some(text => text.includes('scripts/render-report.mjs'))) add('ERROR', 'missing-script', 'Docs reference scripts/render-report.mjs but the skill does not ship it; run node skills/common/build.mjs.');
  const output = texts.get('output.md');
  if (output === undefined) return;
  if (files.includes('scripts/render-report.mjs') && !/^## Saved report\b[\s\S]*(?:--manifest|references\/report-output\.md)/m.test(output)) add('WARN', 'saved-report-missing', 'output.md needs a Saved report section linking shared report guidance.');
}

// A skill keeps its outputs (reports, RFCs, saved runs) under <workspace>/.octocode/<skill-name>/ and its scratch
// under <workspace>/.octocode/tmp/<skill-name>/. A scratch path into another folder of .octocode/tmp/ (shared, a run
// or task name, another skill's) is flagged; the skill's own folder, a <skill-name> placeholder and a bare
// `.octocode/tmp/` mention are not. In code, a joined path counts when its next segment is a literal or absent.
// Repo-only skills (the skills-dev collection) are the repository's own tooling: they follow its
// `.octocode/tmp/<task>/` rule, so their task folders are not flagged.
const TMP_PATH = /\.octocode\/tmp\/(<[^>\s]*>|\{[^}\s]*\}|[^\s/`'"()[\]<>{}$*…]+)/g;
const TMP_JOIN = /['"]\.octocode['"],\s*['"]tmp['"]\s*(?:,\s*['"]([^'"]*)['"]|(\)))/g;
const SKILL_PLACEHOLDER = /^[<{]skill(?:[-_]name)?[>}]$/;
// These two hold the rule's own test fixtures.
const SHARED_TMP_FIXTURES = ['scripts/lib/report/self-test.mjs', 'scripts/skill-review.mjs'];

function foreignScratch(source, name) {
  const folders = [...source.matchAll(TMP_PATH)].map((m) => m[1]);
  for (const m of source.matchAll(TMP_JOIN)) folders.push(m[2] ? '' : m[1]);
  return folders.filter((folder) => folder !== name && !SKILL_PLACEHOLDER.test(folder));
}

function checkConventions({ name, files, texts, dir, add }) {
  if (basename(dirname(realpathSync(dir))) === 'skills-dev') return;
  for (const path of files.filter((item) => /\.(md|mjs|js|cjs|sh|py)$/.test(item) && !SHARED_TMP_FIXTURES.includes(item))) {
    const source = texts.get(path) ?? readFileSync(join(dir, path), 'utf8');
    const [folder] = foreignScratch(source, name);
    if (folder !== undefined) add('WARN', 'scratch-outside-skill-folder', `${path} points to <workspace>/.octocode/tmp/${folder ? `${folder}/` : ''}; keep scratch in <workspace>/.octocode/tmp/${name}/ and outputs in <workspace>/.octocode/${name}/.`);
  }
}

const templated = (target) => target.includes('<') || target.includes('{') || target.includes('*') || target.includes('…');

const insideSkill = (dir, full) => full === dir || full.startsWith(`${dir}/`);

function checkDocLinks({ dir, texts, add }, path, text) {
  const prose = withoutFences(text);
  for (const target of localPaths(prose)) {
    if (templated(target)) continue;
    const full = resolve(dirname(join(dir, path)), target);
    if (insideSkill(dir, full) && !existsSync(full)) add('ERROR', 'missing-path', `${path} references missing ${target}.`);
  }
  // A link out of the folder resolves in this checkout but not where the skill is installed alone. Repo-only
  // skills (the skills-dev collection) are never installed alone, so their links into the repository stay.
  if (basename(dirname(realpathSync(dir))) === 'skills-dev') return;
  const escaping = new Map();
  for (const { target, rooted, line } of linkTargets(prose)) {
    if (templated(target) || (!rooted && insideSkill(dir, resolve(dirname(join(dir, path)), target)))) continue;
    escaping.set(target, [...(escaping.get(target) || []), line]);
  }
  for (const [target, lines] of escaping) add('WARN', 'link-escapes-skill', `${path}:${[...new Set(lines)].join(',')} links to ${target}, outside the skill folder; a standalone install does not ship it. Bundle the file, link a public URL, or name the other skill in plain text.`, { file: path, line: lines[0] });
  const inProse = new Set(supportPaths(prose));
  for (const target of new Set(supportPaths(text))) {
    if (existsSync(join(dir, target))) continue;
    // A path that appears only inside a fenced block may be an example; a real command still deserves a look.
    if (inProse.has(target)) add('ERROR', 'missing-support-file', `${path} references missing ${target}.`);
    else add('WARN', 'missing-support-file-in-code', `${path} names ${target} in a code block, and it does not exist; fix it if it is a real command.`);
  }
  for (const { target, anchor } of anchorLinks(prose)) {
    const file = target ? relative(dir, resolve(dir, dirname(path), target)) : path;
    if (!texts.has(file) || !anchor) continue;
    if (!headingAnchors(texts.get(file)).has(decodeURIComponent(anchor))) add('ERROR', 'missing-anchor', `${path} links to #${anchor} in ${file}, which has no such heading.`);
  }
}

// Follow actual document routes from the lobby; two orphan pages linking each other do not count.
function checkRouting({ dir, files, texts, add }) {
  const reachable = new Set(['SKILL.md']);
  const pending = ['SKILL.md'];
  while (pending.length) {
    const path = pending.pop();
    const text = texts.get(path) || '';
    const targets = [...localPaths(withoutFences(text))].map((target) => relative(dir, resolve(dir, dirname(path), target)));
    targets.push(...supportPaths(text));
    for (const target of targets) {
      if (!files.includes(target) || reachable.has(target)) continue;
      reachable.add(target);
      if (texts.has(target)) pending.push(target);
    }
  }
  for (const path of files.filter((item) => item.startsWith('references/') && item.endsWith('.md') || /^scripts\/[^/]+\.(mjs|js|sh|py)$/.test(item))) {
    if (!reachable.has(path)) add('WARN', 'unrouted-resource', `${path} has no route from SKILL.md.`);
  }
  return reachable;
}

// With --sizes: UTF-8 bytes of what a host reads at each stage: the discovery description, the activated entry,
// and each document routed from it (scripts are run, not read, so they are not counted).
function routeSizes({ dir, skill }, reachable) {
  const bytes = (text) => Buffer.byteLength(text, 'utf8');
  const raw = (skill.match(/^description:[ \t]*(.*)$/m) || [])[1] || '';
  let description = raw.trim();
  try { if (description.startsWith('"')) description = JSON.parse(description); } catch { /* reported by the frontmatter checks */ }
  const routed = Object.fromEntries([...reachable].filter((path) => path !== 'SKILL.md' && path.endsWith('.md')).toSorted().map((path) => [path, statSync(join(dir, path)).size]));
  return { description: bytes(description), entry: bytes(skill), routed, routedTotal: Object.values(routed).reduce((sum, size) => sum + size, 0) };
}

// A shipped script is code the agent should run instead of doing the work by hand: the text must
// show its command, and the script must explain its inputs, outputs and exit codes on --help.
// Tests are run by the test runner, and a script another script calls is documented by its caller.
const RUNNERS = { py: 'python3', sh: 'bash' };
function checkScripts({ dir, files, texts, add }) {
  const source = (path) => readFileSync(join(dir, path), 'utf8');
  const code = new Map(files.filter((item) => /^scripts\/.+\.(mjs|js|cjs|sh|py)$/.test(item) && !/\.test\.[a-z]+$/.test(item)).map((path) => [path, source(path)]));
  const docText = [...texts.values()].join('\n');
  const docLines = docText.split('\n');
  const fenced = [...docText.matchAll(/^(`{3,}|~{3,})[^\n]*\n([\s\S]*?)^\1/gm)].map(([, , body]) => body).join('\n');
  for (const path of [...code.keys()].filter((item) => /^scripts\/[^/]+$/.test(item))) {
    const file = basename(path);
    const stem = file.replace(/\.[a-z]+$/, '');
    const imports = new RegExp(`^\\s*(?:import\\s+${stem}\\b|from\\s+${stem}\\s+import\\b)`, 'm');
    if ([...code].some(([other, text]) => other !== path && (text.includes(file) || (file.endsWith('.py') && imports.test(text))))) continue;
    // A command: the path on a line that runs an interpreter, or the path followed by an argument.
    const named = new RegExp(`(?:^|[\\s\`'"(/=])${file.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![\\w.-])`);
    const runs = /\b(?:node|python3?|bash|sh|bun|deno|NODE_OPTIONS)\b|--(?:import|require|loader)\b/;
    const argued = new RegExp(`${file.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s+(?:--?[A-Za-z]|<)`);
    const runner = RUNNERS[file.split('.').pop()] || 'node';
    // A code block naming the script (a command, or a variable a later command runs) also documents it.
    if (!docLines.some((line) => named.test(line) && (runs.test(line) || argued.test(line))) && !named.test(fenced)) add('WARN', 'script-usage-missing', `${path} has no documented command (such as \`${runner} <skill-folder>/${path} --help\`); show it in the step that needs it, with when to run it and how to read its result.`);
    // --help may live in a module the script imports (an argument parser in scripts/lib).
    const own = code.get(path);
    const imported = [...own.matchAll(/from\s+['"](\.{1,2}\/[^'"]+)['"]/g)].map(([, target]) => relative(dir, resolve(dir, dirname(path), target))).filter((target) => code.has(target)).map((target) => code.get(target));
    if (![own, ...imported].some((text) => /--help\b|\bargparse\b|\bgetopts\b/.test(text))) add('WARN', 'script-help-missing', `${path} does not answer --help; document its inputs, outputs, effects and exit codes there.`);
  }
}

// With --commands: every flag a documented command passes to a shipped script must appear in that script's
// --help. Runs each script's --help once (no other arguments); a script that cannot answer is reported by
// script-help-missing instead.
function checkCommandFlags({ dir, files, texts, add }) {
  const scripts = new Set(files.filter((item) => /^scripts\/[^/]+\.(mjs|js|cjs|sh|py)$/.test(item)));
  const help = new Map();
  const helpOf = (path) => {
    if (!help.has(path)) {
      const runner = RUNNERS[path.split('.').pop()] || process.execPath;
      const run = spawnSync(runner, [join(dir, path), '--help'], { cwd: dir, encoding: 'utf8', timeout: 15_000, env: { PATH: process.env.PATH, HOME: process.env.HOME } });
      help.set(path, run.status === 0 ? `${run.stdout}\n${run.stderr}` : null);
    }
    return help.get(path);
  };
  for (const [doc, raw] of texts) {
    // A shell command continued with a trailing backslash is one command.
    const text = raw.replace(/\\\n[ \t]*/g, ' ');
    for (const [, name, rest] of text.matchAll(/scripts\/([\w.-]+\.(?:mjs|js|cjs|sh|py))[`'"]?((?:[ \t]+[^`\n|]*)?)/g)) {
      const path = `scripts/${name}`;
      if (!scripts.has(path)) continue;
      // Arguments after a bare `--` belong to the program the script runs.
      const own = rest.split(/\s--(?:\s|$)/)[0];
      const flags = [...new Set([...own.matchAll(/(?:^|\s)(--[a-z][\w-]*)/g)].map(([, flag]) => flag))].filter((flag) => flag !== '--help');
      if (!flags.length) continue;
      const usage = helpOf(path);
      if (usage === null) continue;
      for (const flag of flags) if (!new RegExp(`${flag}(?![\\w-])`).test(usage)) add('WARN', 'command-flag-unknown', `${doc} passes ${flag} to ${path}, which its --help does not list.`);
    }
  }
}

// With --tells: the writing tells a script can find exactly; references/writing-tells.md judges each one.
// Uppercase MUST/NEVER outside code, counted per file, and sentences that repeat across a skill's documents.
// A slash-joined spelling (MUST/NEVER) names the words instead of using them.
const SHOUTED = /(?<![\w/])(?:MUST|NEVER)(?![\w/])/g;
const withoutFrontmatter = (text) => text.replace(/^---\s*\n[\s\S]*?\n---[^\n]*/, (block) => block.replace(/[^\n]/g, ''));
const withoutCodeSpans = (line) => line.replace(/(`+)[^`\n]+?\1/g, '');
// A cell or item that is only code spans and links is a command or a citation, not a sentence.
const hasProse = (sentence) => /\p{L}/u.test(withoutCodeSpans(sentence).replace(/\[[^\]]*\]\([^)]*\)/g, ''));
// Shorter repeats ("Do not install.") are labels and verdicts, not a rule stated twice.
const MIN_SENTENCE_WORDS = 4;

// Sentences of a document's prose, each with its starting line: paragraphs, list items and table cells are
// split at terminal punctuation; headings, table header rows and rules, fenced code and frontmatter are not prose.
function sentences(text) {
  const found = [];
  let parts = [];
  const flush = () => {
    let joined = '';
    const starts = [];
    for (const part of parts) { starts.push([joined.length, part.line]); joined += `${part.text} `; }
    for (const match of joined.matchAll(/\S[\s\S]*?(?:[.!?]+[)"'\]\u201d\u2019*_]*(?=\s|$)|(?=\s*$))/g)) {
      const sentence = match[0].trim();
      if (sentence.split(/\s+/).length < MIN_SENTENCE_WORDS || !hasProse(sentence)) continue;
      found.push({ sentence, line: starts.findLast(([offset]) => offset <= match.index)[1] });
    }
    parts = [];
  };
  const rule = (line = '') => /^\|?[\s:|-]*-[\s:|-]*$/.test(line.trim());
  const lines = withoutFrontmatter(withoutFences(text)).split('\n');
  lines.forEach((raw, index) => {
    const line = raw.trim();
    if (!line || /^#{1,6}\s/.test(line) || rule(line) || line.startsWith('<!--') || (line.startsWith('|') && rule(lines[index + 1]))) { flush(); return; }
    if (line.startsWith('|')) {
      flush();
      for (const cell of line.split(/(?<!\\)\|/).map((item) => item.trim()).filter(Boolean)) { parts.push({ text: cell, line: index + 1 }); flush(); }
      return;
    }
    const item = line.match(/^(?:[-*+]|\d+[.)]|>)\s+(.*)$/);
    if (item) flush();
    parts.push({ text: item ? item[1] : line, line: index + 1 });
  });
  flush();
  return found;
}

function checkTells({ texts, add }) {
  for (const [path, text] of texts) {
    const lines = withoutFences(text).split('\n').flatMap((line, index) => (withoutCodeSpans(line).match(SHOUTED) || []).map(() => index + 1));
    if (lines.length) add('WARN', 'tell-must-never', `${path}:${[...new Set(lines)].join(',')} has ${lines.length} MUST/NEVER; keep them for safety, authority and hard contracts, and write other rules as plain imperatives.`, { file: path, line: lines[0] });
  }
  const seen = new Map();
  for (const [path, text] of texts) {
    for (const { sentence, line } of sentences(text)) {
      const key = sentence.toLowerCase().replace(/\s+/g, ' ');
      seen.set(key, [...(seen.get(key) || []), { file: path, line, sentence }]);
    }
  }
  for (const places of seen.values()) {
    if (new Set(places.map((place) => place.file)).size < 2) continue;
    add('WARN', 'tell-repeated-sentence', `"${places[0].sentence}" repeats at ${places.map((place) => `${place.file}:${place.line}`).join(', ')}; keep one owner and link to it.`, { file: places[0].file, line: places[0].line });
  }
}

function review(dir, { commands = false, sizes = false, tells = false } = {}) {
  const name = basename(dir);
  const findings = [];
  const add = (level, code, message, at = {}) => findings.push({ level, code, message, ...at });
  if (!existsSync(join(dir, 'SKILL.md'))) {
    add('ERROR', 'skill-missing', 'SKILL.md is missing.');
    return { skill: name, path: dir, findings };
  }
  const files = filesUnder(dir);
  const texts = new Map(files.filter((path) => path.endsWith('.md')).map((path) => [path, readFileSync(join(dir, path), 'utf8')]));
  const ctx = { dir, name, files, texts, skill: texts.get('SKILL.md') || '', add };
  checkFrontmatter(ctx);
  checkLayout(ctx);
  checkDocLength(ctx);
  checkConventions(ctx);
  for (const [path, text] of texts) checkDocLinks(ctx, path, text);
  const reachable = checkRouting(ctx);
  checkScripts(ctx);
  if (commands) checkCommandFlags(ctx);
  if (tells) checkTells(ctx);
  return { skill: name, path: dir, findings, ...(sizes ? { sizes: routeSizes(ctx, reachable) } : {}) };
}

// Self-test, part 1: generic output guides, public descriptions, saved report, missing paths.
function selfTestBasics(dir, skill) {
  writeFileSync(join(skill, 'SKILL.md'), '---\nname: example-skill\ndescription: "Use when an example needs review."\n---\n\n# Example\n\n## Related skills\n\n- `other-skill`: For another job.\n\n## Output\n\nSee [output.md](output.md).\n');
  writeFileSync(join(skill, 'README.md'), '# Example\n');
  writeFileSync(join(skill, 'output.md'), '# Output\n\nAn answer.\n');
  if (review(skill).findings.length) throw new Error('a generic output guide was forced to name an Octocode destination');
  writeFileSync(join(skill, 'output.md'), '# Output\n\nAn answer. Save requested files under `<workspace>/.octocode/example-skill/`.\n');
  if (review(skill).findings.length) throw new Error('valid skill failed');
  const publicRoot = join(dir, 'public', 'skills');
  mkdirSync(publicRoot, { recursive: true });
  const publicSkill = join(publicRoot, 'example-skill');
  mkdirSync(publicSkill);
  for (const file of ['README.md', 'output.md']) writeFileSync(join(publicSkill, file), readFileSync(join(skill, file)));
  const publicSource = readFileSync(join(skill, 'SKILL.md'), 'utf8');
  writeFileSync(join(publicSkill, 'SKILL.md'), publicSource);
  if (review(publicSkill).findings.length) throw new Error('a public skill with a JSON description failed');
  for (const bad of ['description: |-\n  Use when an example needs review.', 'description: Use when an example needs review.', "description: 'Use when an example needs review.'"]) {
    writeFileSync(join(publicSkill, 'SKILL.md'), publicSource.replace('description: "Use when an example needs review."', bad));
    if (!review(publicSkill).findings.some((item) => item.code === 'description-json')) throw new Error(`a public description outside one JSON line passed: ${bad}`);
    writeFileSync(join(skill, 'SKILL.md'), publicSource.replace('description: "Use when an example needs review."', bad));
    if (review(skill).findings.some((item) => item.code === 'description-json')) throw new Error('a non-public skill was held to the JSON description rule');
  }
  writeFileSync(join(skill, 'SKILL.md'), publicSource);
  mkdirSync(join(skill, 'scripts'));
  writeFileSync(join(skill, 'scripts/render-report.mjs'), '// viewer\n');
  if (!review(skill).findings.some((item) => item.code === 'saved-report-missing')) throw new Error('missing saved-report section was not found');
  rmSync(join(skill, 'scripts'), { recursive: true });
  writeFileSync(join(skill, 'README.md'), '# Example\n\nSee [missing.md](missing.md).\n');
  if (!review(skill).findings.some((item) => item.code === 'missing-path')) throw new Error('missing path was not found');
  writeFileSync(join(skill, 'README.md'), '# Example\n');
}

// Part 2: CRLF and comments, reference-style routes, orphan cycles, and heading anchors.
function selfTestLinks(skill) {
  let source = readFileSync(join(skill, 'SKILL.md'), 'utf8').replace('name: example-skill', 'name: example-skill # valid comment').replace('## Related skills', '## Handoffs').replace('[output.md](output.md)', '[Result formats](<output.md> "Formats")');
  writeFileSync(join(skill, 'SKILL.md'), source.replace(/\n/g, '\r\n'));
  if (review(skill).findings.length) throw new Error('valid CRLF, comment, heading, or link rejected');
  mkdirSync(join(skill, 'references'));
  writeFileSync(join(skill, 'references/a.md'), '[B](b.md)\n');
  writeFileSync(join(skill, 'references/b.md'), '[A](a.md)\n');
  if (review(skill).findings.filter((item) => item.code === 'unrouted-resource').length !== 2) throw new Error('orphan reference cycle was missed');
  source += '\n[Resources][guide]\n\n[guide]: references/a.md\n';
  writeFileSync(join(skill, 'SKILL.md'), source);
  if (review(skill).findings.length) throw new Error('reference-style route or linked catalog failed');
  writeFileSync(join(skill, 'references/a.md'), '# Guide\n\n## Search the web\n\n## `--manifest` usage\n\n[B](b.md)\n\n[ok](#search-the-web) [ok](#--manifest-usage) [ok](b.md#a)\n');
  writeFileSync(join(skill, 'references/b.md'), '# A\n\n[A](a.md#search-the-web)\n');
  if (review(skill).findings.length) throw new Error(`valid anchors rejected: ${JSON.stringify(review(skill).findings)}`);
  writeFileSync(join(skill, 'references/b.md'), '# A\n\n[A](a.md#research-with-current-sources)\n');
  if (!review(skill).findings.some((item) => item.code === 'missing-anchor')) throw new Error('renamed-section anchor was missed');
  writeFileSync(join(skill, 'references/a.md'), '[B](b.md)\n');
  writeFileSync(join(skill, 'references/b.md'), '[A](a.md)\n');
  return source;
}

// Part 3: scalar forms, fenced code, scratch paths, and the Resources table.
function selfTestContent(skill, source) {
  source = source.replace('description: "Use when an example needs review."', 'description: |-\n  Review an example.');
  writeFileSync(join(skill, 'SKILL.md'), source);
  if (review(skill).findings.length) throw new Error('block scalar failed');
  writeFileSync(join(skill, 'SKILL.md'), source.replace('description: |-\n  Review an example.', `description: Use when an example\n  ${'needs a long review. '.repeat(60)}`));
  if (!review(skill).findings.some((item) => item.code === 'description-length')) throw new Error('a multi-line plain description escaped the length check');
  for (const count of [1024, 1025]) { writeFileSync(join(skill, 'SKILL.md'), source.replace('description: |-\n  Review an example.', `description: "${'\u{1F600}'.repeat(count)}"`)); if (review(skill).findings.some((item) => item.code === 'description-length') !== count > 1024) throw new Error(`a ${count}-code-point description (twice as many UTF-16 units) was measured wrong`); }
  const trimmed = source.trimEnd(), trimmedLines = trimmed.split('\n').length;
  for (const count of [MAX_SKILL_LINES, MAX_SKILL_LINES + 1]) {
    writeFileSync(join(skill, 'SKILL.md'), `${trimmed}${'\n'.repeat(count - trimmedLines + 1)}`);
    const finding = review(skill).findings.find((item) => item.code === 'skill-length');
    if (Boolean(finding) !== count > MAX_SKILL_LINES || (finding && finding.level !== 'WARN')) throw new Error(`a ${count}-line SKILL.md was judged wrong for skill-length`);
  }
  writeFileSync(join(skill, 'SKILL.md'), source);
  mkdirSync(join(skill, 'references'), { recursive: true });
  for (const count of [MAX_DOC_LINES, MAX_DOC_LINES + 1]) {
    writeFileSync(join(skill, 'references/long.md'), 'line\n'.repeat(count));
    const finding = review(skill).findings.find((item) => item.code === 'doc-length');
    if (Boolean(finding) !== count > MAX_DOC_LINES || (finding && finding.level !== 'WARN')) throw new Error(`a ${count}-line reference was judged wrong for doc-length`);
  }
  rmSync(join(skill, 'references/long.md'));
  writeFileSync(join(skill, 'README.md'), '# Example\n\n```md\n[link](not-here.md) [anchor](#nowhere)\n```\n\n```bash\nnode scripts/example-tool.mjs\n```\n');
  const fenced = review(skill).findings.filter((item) => item.code !== 'description-length');
  if (fenced.some((item) => item.level === 'ERROR') || !fenced.some((item) => item.code === 'missing-support-file-in-code')) throw new Error(`fenced code was linted as links, or a missing command went unreported: ${JSON.stringify(fenced)}`);
  for (const foreign of ['`<workspace>/.octocode/tmp/example/<run>`', '`<workspace>/.octocode/tmp/<run>/notes.md`', "`join(root, '.octocode', 'tmp', 'other-skill')`", "`join(root, '.octocode', 'tmp')`"]) {
    writeFileSync(join(skill, 'README.md'), `# Example\n\nScratch goes to ${foreign}.\n`);
    if (!review(skill).findings.some((item) => item.code === 'scratch-outside-skill-folder')) throw new Error(`scratch outside the skill's own .octocode/tmp/ folder was not flagged: ${foreign}`);
  }
  writeFileSync(join(skill, 'README.md'), "# Example\n\nRuns keep scratch in `<workspace>/.octocode/tmp/example-skill/<run>` (`join(root, '.octocode', 'tmp', 'example-skill', run)`, or `join(root, '.octocode', 'tmp', SKILL)`) and save to `<workspace>/.octocode/example-skill/`; skills keep scratch in `<workspace>/.octocode/tmp/<skill-name>/`; a scratch folder such as `.octocode/tmp/` is not a report location.\n");
  if (review(skill).findings.some((item) => item.code === 'scratch-outside-skill-folder')) throw new Error("the skill's own scratch folder, a placeholder or a passing mention was flagged");
  writeFileSync(join(skill, 'README.md'), '# Example\n');
  writeFileSync(join(skill, 'SKILL.md'), source.replace('description: |-\n  Review an example.', 'description:\nlicense: MIT'));
  if (!review(skill).findings.some((item) => item.code === 'description-missing')) throw new Error('empty description consumed the next field');
  writeFileSync(join(skill, 'SKILL.md'), `${source}\n## Resources\n\n| When | Read |\n|---|---|\n| Saving | [output.md](output.md) |\n`);
  if (review(skill).findings.length) throw new Error('a conditional two-column Resources table was rejected');
  writeFileSync(join(skill, 'SKILL.md'), `${source}\n## Resources\n\n| When | Read or run | Why |\n|---|---|---|\n| Saving | [output.md](output.md) | Format |\n`);
  if (review(skill).findings.length) throw new Error('a three-column Resources table was rejected');
  writeFileSync(join(skill, 'SKILL.md'), source);
}

// Part 4: collections, symlinks, private asset packages, and single-file skills.
function selfTestCollections(dir, skill) {
  const collection = join(dir, 'collection');
  mkdirSync(collection);
  symlinkSync(skill, join(collection, 'example-skill'), 'dir');
  if (skillDirs(collection).length !== 1 || review(skillDirs(collection)[0]).findings.length) throw new Error('symlinked skill failed');
  mkdirSync(join(collection, 'missing-skill'));
  if (skillDirs(collection).length !== 2) throw new Error('incomplete skill folder was silently skipped');
  mkdirSync(join(collection, 'private-assets'));
  writeFileSync(join(collection, 'private-assets', 'package.json'), JSON.stringify({ private: true }));
  if (skillDirs(collection).length !== 2) throw new Error('private asset package was mistaken for a skill');
  const empty = join(dir, 'empty');
  mkdirSync(empty);
  if (!review(skillDirs(empty)[0]).findings.some((item) => item.code === 'skill-missing')) throw new Error('empty collection passed');
  if (skillDirs(empty, true).length) throw new Error('explicit empty collection must have no skills');
  if (skillDirs(collection, true).length !== 2) throw new Error('explicit collection skipped an incomplete skill or included private assets');
  if (skillDirs(join(dir, 'absent'), true)[0] !== join(dir, 'absent')) throw new Error('missing collection root was silently skipped');
  selfTestBrokenPackage(collection);
  const single = join(dir, 'single-skill');
  mkdirSync(single);
  writeFileSync(join(single, 'SKILL.md'), '---\nname: single-skill\ndescription: "Use when one file is enough."\n---\n\n# Single\n\nAnswer inline.\n');
  if (review(single).findings.length) throw new Error(`a single-file skill was rejected: ${JSON.stringify(review(single).findings)}`);
  mkdirSync(join(single, 'scripts'));
  writeFileSync(join(single, 'scripts', 'run.py'), 'import argparse\nargparse.ArgumentParser().parse_args()\nprint("done")\n');
  writeFileSync(join(single, 'SKILL.md'), `${readFileSync(join(single, 'SKILL.md'), 'utf8')}\nRun [the helper](scripts/run.py) with \`python3 scripts/run.py\`; its result format is inline above.\n`);
  if (review(single).findings.length) throw new Error(`a single instruction file with an executable helper was rejected: ${JSON.stringify(review(single).findings)}`);
  writeFileSync(join(single, 'notes.md'), '# Notes\n');
  if (skillDirs(join(single, 'notes.md'), true)[0] !== join(single, 'notes.md')) throw new Error('non-directory collection root was silently skipped');
  if (review(single).findings.length) throw new Error('an inline result contract required a separate output.md because another document exists');
  writeFileSync(join(single, 'output.md'), '# Output\n\nSave under .octocode/single-skill/.\n');
  if (!review(single).findings.some((item) => item.code === 'output-route-missing')) throw new Error('an existing unlinked output guide was missed');
  writeFileSync(join(single, 'SKILL.md'), `${readFileSync(join(single, 'SKILL.md'), 'utf8')}\nSee [output](output.md).\n`);
  if (review(single).findings.length) throw new Error(`a multi-file skill with a linked output guide needs no README: ${JSON.stringify(review(single).findings)}`);
}

// An unreadable package.json is discovered and reviewed as a failing skill, not skipped.
function selfTestBrokenPackage(collection) {
  const broken = join(collection, 'broken-package');
  mkdirSync(broken);
  writeFileSync(join(broken, 'package.json'), '{');
  const discovered = skillDirs(collection, true);
  if (discovered.length !== 3 || !discovered.includes(broken)) throw new Error('a folder with an unreadable package.json was skipped or crashed the run instead of being reported');
  const findings = review(broken).findings;
  if (!findings.some((item) => item.level === 'ERROR' && item.code === 'skill-missing')) throw new Error(`the discovered broken package was not reported as skill-missing: ${JSON.stringify(findings)}`);
}

// Part 5: a routed script needs a documented command and --help.
function selfTestScripts(skill) {
  const source = readFileSync(join(skill, 'SKILL.md'), 'utf8');
  mkdirSync(join(skill, 'scripts'), { recursive: true });
  writeFileSync(join(skill, 'scripts/count-rows.mjs'), 'console.log(1);\n');
  writeFileSync(join(skill, 'SKILL.md'), `${source}\nCount rows with the [counter](scripts/count-rows.mjs).\n`);
  const codes = () => review(skill).findings.map((item) => item.code).toSorted();
  if (codes().join() !== 'script-help-missing,script-usage-missing') throw new Error(`a script without a command or --help passed: ${codes()}`);
  writeFileSync(join(skill, 'scripts/count-rows.mjs'), "if (process.argv.includes('--help')) console.log('Usage: count-rows.mjs --file <path>');\n");
  writeFileSync(join(skill, 'SKILL.md'), `${source}\nCount rows with the [counter](scripts/count-rows.mjs): \`node <skill-folder>/scripts/count-rows.mjs --file rows.jsonl\`.\n`);
  if (codes().length) throw new Error(`a documented script with --help was flagged: ${codes()}`);
  writeFileSync(join(skill, 'SKILL.md'), source);
  rmSync(join(skill, 'scripts'), { recursive: true });
}

// Part 6: --commands compares documented flags with the script's --help.
function selfTestCommandFlags(skill) {
  const source = readFileSync(join(skill, 'SKILL.md'), 'utf8');
  mkdirSync(join(skill, 'scripts'), { recursive: true });
  writeFileSync(join(skill, 'scripts/count-rows.mjs'), "if (process.argv.includes('--help')) console.log('Usage: count-rows.mjs --file <path> [--json]');\n");
  writeFileSync(join(skill, 'SKILL.md'), `${source}\nCount rows ([counter](scripts/count-rows.mjs)):\n\n\`\`\`bash\nnode <skill-folder>/scripts/count-rows.mjs --file rows.jsonl \\\n  --jsonl\n\`\`\`\n`);
  const flagged = review(skill, { commands: true }).findings.filter((item) => item.code === 'command-flag-unknown');
  if (flagged.length !== 1 || !flagged[0].message.includes('--jsonl')) throw new Error(`an unknown documented flag was missed or a known one flagged: ${JSON.stringify(flagged)}`);
  if (review(skill).findings.some((item) => item.code === 'command-flag-unknown')) throw new Error('flags were checked without --commands');
  const { sizes } = review(skill, { sizes: true });
  if (!sizes || sizes.entry !== Buffer.byteLength(readFileSync(join(skill, 'SKILL.md'), 'utf8')) || !(sizes.description > 0)) throw new Error(`--sizes miscounted: ${JSON.stringify(sizes)}`);
  writeFileSync(join(skill, 'SKILL.md'), source);
  rmSync(join(skill, 'scripts'), { recursive: true });
}

// Part 7: links that leave the skill folder are portability warnings; fenced code and URLs are not links.
function selfTestEscapingLinks(skill) {
  const source = readFileSync(join(skill, 'SKILL.md'), 'utf8');
  writeFileSync(join(skill, 'SKILL.md'), `${source}\nSee [the guide](../other-skill/SKILL.md), [again](../other-skill/SKILL.md#use), [docs](/docs/guide.md), [web](https://example.com/a.md), [mail](mailto:a@example.com) and [ours](output.md).\n\n\`\`\`md\n[fenced](../fenced.md)\n\`\`\`\n`);
  const line = readFileSync(join(skill, 'SKILL.md'), 'utf8').split('\n').findIndex((text) => text.startsWith('See [the guide]')) + 1;
  const escaping = review(skill).findings.filter((item) => item.code === 'link-escapes-skill');
  const got = escaping.map((item) => `${item.level} ${item.file}:${item.line} ${item.message.split(' links to ')[1].split(',')[0]}`).toSorted();
  const want = [`WARN SKILL.md:${line} ../other-skill/SKILL.md`, `WARN SKILL.md:${line} /docs/guide.md`];
  if (got.join('|') !== want.join('|') || review(skill).findings.length !== 2) throw new Error(`escaping links were missed, merged wrongly or over-reported: ${JSON.stringify(review(skill).findings)}`);
  const devSkill = join(dirname(skill), 'skills-dev', 'repo-skill');
  mkdirSync(devSkill, { recursive: true });
  writeFileSync(join(devSkill, 'SKILL.md'), readFileSync(join(skill, 'SKILL.md'), 'utf8').replace('name: example-skill', 'name: repo-skill'));
  if (review(devSkill).findings.some((item) => item.code === 'link-escapes-skill')) throw new Error('a repo-only skills-dev skill was held to standalone link portability');
  writeFileSync(join(devSkill, 'README.md'), '# Repo skill\n\nGate logs go to `<repo>/.octocode/tmp/gates/<run>/`.\n');
  if (review(devSkill).findings.some((item) => item.code === 'scratch-outside-skill-folder')) throw new Error("a repo-only skills-dev skill was held to the installed skills' scratch folder");
  rmSync(join(dirname(skill), 'skills-dev'), { recursive: true });
  writeFileSync(join(skill, 'SKILL.md'), source);
}

// Part 8: --tells counts MUST/NEVER outside code and finds sentences repeated across files after folding.
function selfTestTells(skill) {
  const source = readFileSync(join(skill, 'SKILL.md'), 'utf8');
  const tells = (options = { tells: true }) => review(skill, options).findings.filter((item) => item.code.startsWith('tell-'));
  writeFileSync(join(skill, 'SKILL.md'), `${source}\nYou MUST keep the cache in the skill folder.\nNEVER delete it. Reserve MUST/NEVER for safety; \`MUST\` in code is a mention.\n\n\`\`\`text\nMUST NEVER\n\`\`\`\n`);
  const mustLine = readFileSync(join(skill, 'SKILL.md'), 'utf8').split('\n').findIndex((text) => text.startsWith('You MUST')) + 1;
  const shouted = tells().filter((item) => item.code === 'tell-must-never');
  if (shouted.length !== 1 || shouted[0].level !== 'WARN' || shouted[0].line !== mustLine || !shouted[0].message.startsWith(`SKILL.md:${mustLine},${mustLine + 1} has 2 MUST/NEVER`)) throw new Error(`MUST/NEVER was miscounted: ${JSON.stringify(shouted)}`);
  if (tells({}).length) throw new Error('tells were reported without --tells');
  writeFileSync(join(skill, 'SKILL.md'), source);
  writeFileSync(join(skill, 'references/a.md'), '# A\n\n| When this step runs | The rule to follow |\n|---|---|\n| Save | Keep the cache in the skill folder. |\n\n[B](b.md)\n');
  writeFileSync(join(skill, 'references/b.md'), '# B\n\n| When this step runs | The rule to follow |\n|---|---|\n\nFirst read the plan.  keep the cache\nin   the SKILL folder. Then stop.\n\n- `node scripts/run.mjs --all now`\n\n[A](a.md)\n');
  writeFileSync(join(skill, 'references/c.md'), '# C\n\nSay this twice in one file. Say this twice in one file.\n\n- `node scripts/run.mjs --all now`\n');
  const repeated = tells().filter((item) => item.code === 'tell-repeated-sentence');
  if (repeated.length !== 1 || !repeated[0].message.includes('references/a.md:5, references/b.md:6;') || repeated[0].file !== 'references/a.md' || repeated[0].line !== 5) throw new Error(`a folded repeat across files was missed, or a header row, a same-file repeat or a code-only item was reported: ${JSON.stringify(repeated)}`);
  writeFileSync(join(skill, 'references/a.md'), '[B](b.md)\n');
  writeFileSync(join(skill, 'references/b.md'), '[A](a.md)\n');
  rmSync(join(skill, 'references/c.md'));
}

function selfTest() {
  const dir = mkdtempSync(join(tmpdir(), 'skill-review-'));
  try {
    mkdirSync(join(dir, 'example-skill'));
    const skill = join(dir, 'example-skill');
    selfTestBasics(dir, skill);
    selfTestContent(skill, selfTestLinks(skill));
    selfTestCollections(dir, skill);
    selfTestScripts(skill);
    selfTestCommandFlags(skill);
    selfTestEscapingLinks(skill);
    selfTestTells(skill);
    console.log('PASS skill-review-self-test');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

const args = process.argv.slice(2);
if (args.includes('--help') || args.includes('-h')) {
  console.log('Usage: node scripts/skill-review.mjs [skill-dir | collection-dir ...] [--collections] [--commands] [--sizes] [--tells] [--json] [--self-test]');
  console.log('--collections allows existing empty collection directories; missing roots and incomplete child skills still fail.');
  console.log('--commands also runs each shipped script\'s --help and warns on a documented flag it does not list (command-flag-unknown).');
  console.log('--sizes adds UTF-8 bytes per skill: description, entry (SKILL.md) and each routed document (sizes in --json; a line per skill otherwise).');
  console.log(`--tells also warns on writing tells in each skill's Markdown, outside fenced code: tell-must-never (uppercase MUST or NEVER outside code spans, counted per file with its lines; a slash-joined MUST/NEVER names the words and is not counted) and tell-repeated-sentence (a sentence, list item or table cell of ${MIN_SENTENCE_WORDS} or more words found in two or more files after case and whitespace folding, with every file:line; table headers and items that are only code or links are skipped).`);
  console.log('Every run also warns on link-escapes-skill: a Markdown link outside fenced code that resolves outside the skill folder (a ../ path out of it or a root-absolute /path), which a standalone install does not ship; URLs such as https: are not checked, and repo-only skills in a skills-dev collection are exempt.');
  console.log('Output: findings carry level, code and message; link-escapes-skill and the tell findings also carry file and line. Exit 1 when any ERROR is found; WARN alone exits 0.');
  process.exit(0);
}
if (args.includes('--self-test')) { selfTest(); process.exit(0); }
const unknown = args.find((arg) => arg.startsWith('-') && !['--json', '--collections', '--commands', '--sizes', '--tells', '-h'].includes(arg));
if (unknown) { console.error(`Unknown option: ${unknown}`); process.exit(1); }
const json = args.includes('--json');
const targets = args.filter((arg) => !arg.startsWith('--')).map((arg) => resolve(arg));
if (!targets.length) targets.push(resolve('skills'));
function isPrivatePackage(dir) { try { return JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8')).private === true; } catch { return false; } }
function skillDirs(target, collection = false) {
  if (existsSync(join(target, 'SKILL.md'))) return [target];
  if (!existsSync(target) || !statSync(target).isDirectory()) return [target];
  const children = readdirSync(target, { withFileTypes: true })
    .filter((entry) => !entry.name.startsWith('.') && entry.name !== 'node_modules' && (entry.isDirectory() || entry.isSymbolicLink()))
    .map((entry) => join(target, entry.name))
    .filter(dir => existsSync(join(dir, 'SKILL.md')) || !isPrivatePackage(dir)); // asset folders; an unreadable package.json is reported, not skipped
  return collection || children.length ? children : [target];
}
const dirs = [...new Set(targets.flatMap(target => skillDirs(target, args.includes('--collections'))))];
const results = dirs.map((dir) => {
  try { return review(dir, { commands: args.includes('--commands'), sizes: args.includes('--sizes'), tells: args.includes('--tells') }); }
  catch (error) { return { skill: basename(dir), path: dir, findings: [{ level: 'ERROR', code: 'read-failed', message: error.message }] }; }
});
const errorCount = results.flatMap((item) => item.findings).filter((item) => item.level === 'ERROR').length;
const warnCount = results.flatMap((item) => item.findings).filter((item) => item.level === 'WARN').length;
if (json) console.log(JSON.stringify({ errorCount, warnCount, results }, null, 2));
else {
  for (const item of results) {
    console.log(`${item.skill}: ${item.findings.length ? item.findings.map((f) => `${f.level} ${f.code}: ${f.message}`).join('\n  ') : 'OK'}`);
    if (item.sizes) console.log(`  bytes: description ${item.sizes.description}, entry ${item.sizes.entry}, routed ${item.sizes.routedTotal} in ${Object.keys(item.sizes.routed).length} documents`);
  }
  console.log(`${errorCount} errors, ${warnCount} warnings`);
}
process.exitCode = errorCount ? 1 : 0;
