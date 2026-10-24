#!/usr/bin/env node
// Structural checks for Agent Skill folders. Trigger quality and prose require human review.
import { existsSync, realpathSync, statSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync, mkdirSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, relative, resolve } from 'node:path';

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

// A folded or literal block scalar: the indented lines after `key:`.
function blockScalar(lines, key) {
  const start = lines.findIndex((item) => item.startsWith(`${key}:`));
  const parts = [];
  for (const item of lines.slice(start + 1)) {
    if (item && !/^\s+/.test(item)) break;
    parts.push(item.trim());
  }
  return parts.join(' ').trim();
}

// A plain scalar may continue on indented lines; YAML folds them with single spaces.
function plainScalar(lines, key, line) {
  const parts = [line];
  for (const item of lines.slice(lines.findIndex((l) => l.startsWith(`${key}:`)) + 1)) {
    if (!/^\s+\S/.test(item)) break;
    parts.push(item.trim());
  }
  return parts.join(' ').replace(/\s+#.*$/, '').trim();
}

// A double- or single-quoted scalar's value (null when malformed); undefined when the line is not quoted.
function quotedScalar(line) {
  if (line.startsWith('"')) {
    const quoted = line.match(/^"(?:\\.|[^"\\])*"/);
    try { return JSON.parse(quoted?.[0] || ''); } catch { return null; }
  }
  if (line.startsWith("'")) return line.match(/^'((?:''|[^'])*)'/)?.[1]?.replace(/''/g, "'") ?? null;
  return undefined;
}

function yamlField(block, key) {
  const line = block.match(new RegExp(`^${key}:[ \\t]*(.*)$`, 'm'))?.[1]?.trim();
  if (!line) return '';
  const lines = block.split('\n');
  if (/^[>|][+-]?(?:\s+#.*)?$/.test(line)) return blockScalar(lines, key);
  const quoted = quotedScalar(line);
  if (quoted !== undefined) return quoted;
  if (/^[[\]{&*!]/.test(line)) return null; // Advanced YAML needs a YAML-aware validator.
  return plainScalar(lines, key, line);
}

function frontmatter(source) {
  source = source.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n');
  const match = source.match(/^---\s*\n([\s\S]*?)\n---(?:\n|$)/);
  if (!match) return null;
  return { name: yamlField(match[1], 'name'), description: yamlField(match[1], 'description') };
}

// Public skills are embedded in the native binary: `build` decodes `description` from one line.
function jsonDescriptionProblem(source) {
  const lines = source.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n').match(/^---\s*\n([\s\S]*?)\n---(?:\n|$)/)?.[1].split('\n').filter((line) => line.startsWith('description:')) ?? [];
  let value;
  try { value = JSON.parse(lines[0]?.slice('description:'.length).trim()); } catch { /* reported below */ }
  return lines.length === 1 && typeof value === 'string' ? null : 'Public skills need one `description: "<JSON string>"` line; the native build embeds it.';
}

function localPaths(text) {
  const paths = new Set();
  const links = /\]\(\s*(?:<([^>]+)>|([^\s)]+))(?:\s+["'][^\n]*?["'])?\s*\)|^\s*\[[^\]]+\]:\s*(?:<([^>]+)>|(\S+))/gm;
  for (const match of text.matchAll(links)) {
    const raw = match[1] || match[2] || match[3] || match[4];
    const target = raw.split('#')[0];
    if (/^(?:[a-z]+:|\/|#)/i.test(target)) continue;
    if (target) { try { paths.add(decodeURIComponent(target)); } catch { paths.add(target); } }
  }
  return paths;
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

// Fenced blocks hold commands and examples, not Markdown links.
function withoutFences(text) {
  return text.replace(/^ {0,3}(```|~~~)[^\n]*\n[\s\S]*?^ {0,3}\1[^\n]*$/gm, '');
}

function supportPaths(text) {
  return [...text.matchAll(/(?:^|[^A-Za-z0-9./])((?:scripts|references|assets|docs|benchmarks)\/[A-Za-z0-9._/-]+\.(?:mjs|md|tsv|json|html|js|sh|py))/gm)].map((match) => match[1]);
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
  if (fm.description?.length > 1024) add('ERROR', 'description-length', 'Description exceeds the Agent Skills limit of 1024 characters.');
}

function checkLayout({ dir, name, files, texts, skill, add }) {
  // A single-file skill (SKILL.md only, the Agent Skills minimum) owns its output inline.
  if (files.length !== 1) {
    if (!files.includes('README.md')) add('ERROR', 'readme-missing', 'README.md is missing.');
    if (!files.includes('output.md')) add('ERROR', 'output-missing', 'output.md is missing.');
    if (![...localPaths(skill)].some((path) => resolve(dir, path) === join(dir, 'output.md'))) add('WARN', 'output-route-missing', 'Link the output format from SKILL.md using any descriptive label.');
  }
  const common = resolve(dir, '..', 'common');
  for (const resource of ['scripts/render-report.mjs', 'references/report-output.md']) {
    const source = join(common, resource), installed = join(dir, resource);
    if (existsSync(installed) && existsSync(source) && !readFileSync(installed).equals(readFileSync(source))) add('ERROR', 'shared-file-drift', `${resource} differs from skills/common; run node skills/common/build.mjs.`);
  }
  if (!existsSync(join(dir, 'scripts', 'render-report.mjs')) && [...texts.values()].some(text => text.includes('scripts/render-report.mjs'))) add('ERROR', 'missing-script', 'Docs reference scripts/render-report.mjs but the skill does not ship it; run node skills/common/build.mjs.');
  const output = texts.get('output.md');
  if (output === undefined) return;
  if (files.includes('scripts/render-report.mjs') && !/^## Saved report\b[\s\S]*(?:--manifest|references\/report-output\.md)/m.test(output)) add('WARN', 'saved-report-missing', 'output.md needs a Saved report section linking shared report guidance.');
  if (!output.includes('.octocode/')) add('WARN', 'output-location-missing', `output.md should name where saved results go: <workspace>/.octocode/${name}/ (scratch under <workspace>/.octocode/${name}/tmp/).`);
}

// Every file a skill writes stays under <workspace>/.octocode/<skill-name>/ (scratch in its tmp/), never a shared .octocode/tmp/.
const SHARED_TMP = /\.octocode\/tmp\/[A-Za-z0-9<{_-]|['"]\.octocode['"],\s*['"]tmp['"]/;
// These two hold the rule's own test fixtures.
const SHARED_TMP_FIXTURES = ['scripts/lib/report/self-test.mjs', 'scripts/skill-review.mjs'];

function checkConventions({ dir, name, files, texts, skill, add }) {
  // The lobby routes resources through one table whose columns say when to load each file and why.
  const resources = skill.match(/^## Resources\b[\s\S]*?^\|([^\n]*)\|\s*$/m);
  if (resources && !/^\s*When\s*\|\s*Read[^|]*\|\s*Why\s*$/.test(resources[1])) add('WARN', 'resources-columns', 'SKILL.md Resources table should have the columns When | Read | Why.');
  for (const path of files.filter((item) => /\.(md|mjs|js|cjs|sh|py)$/.test(item) && !SHARED_TMP_FIXTURES.includes(item))) {
    const source = texts.get(path) ?? readFileSync(join(dir, path), 'utf8');
    if (SHARED_TMP.test(source)) add('WARN', 'output-outside-skill-folder', `${path} writes or points to <workspace>/.octocode/tmp/; use <workspace>/.octocode/${name}/tmp/ so every output stays in the skill's own folder.`);
  }
}

const templated = (target) => target.includes('<') || target.includes('{') || target.includes('*') || target.includes('…');

function checkDocLinks({ dir, texts, add }, path, text) {
  const prose = withoutFences(text);
  for (const target of localPaths(prose)) {
    if (templated(target)) continue;
    const full = resolve(dirname(join(dir, path)), target);
    if (!full.startsWith(dir + '/') && full !== dir) continue; // cross-skill handoffs are allowed
    if (!existsSync(full)) add('ERROR', 'missing-path', `${path} references missing ${target}.`);
  }
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
}

function review(dir) {
  const name = basename(dir);
  const findings = [];
  const add = (level, code, message) => findings.push({ level, code, message });
  if (!existsSync(join(dir, 'SKILL.md'))) {
    add('ERROR', 'skill-missing', 'SKILL.md is missing.');
    return { skill: name, path: dir, findings };
  }
  const files = filesUnder(dir);
  const texts = new Map(files.filter((path) => path.endsWith('.md')).map((path) => [path, readFileSync(join(dir, path), 'utf8')]));
  const ctx = { dir, name, files, texts, skill: texts.get('SKILL.md') || '', add };
  checkFrontmatter(ctx);
  checkLayout(ctx);
  checkConventions(ctx);
  for (const [path, text] of texts) checkDocLinks(ctx, path, text);
  checkRouting(ctx);
  return { skill: name, path: dir, findings };
}

// Self-test, part 1: output location, JSON descriptions in public skills, saved report, missing paths.
function selfTestBasics(dir, skill) {
  writeFileSync(join(skill, 'SKILL.md'), '---\nname: example-skill\ndescription: "Use when an example needs review."\n---\n\n# Example\n\n## Related skills\n\n- `other-skill`: For another job.\n\n## Output\n\nSee [output.md](output.md).\n');
  writeFileSync(join(skill, 'README.md'), '# Example\n');
  writeFileSync(join(skill, 'output.md'), '# Output\n\nAn answer.\n');
  if (!review(skill).findings.some((item) => item.code === 'output-location-missing')) throw new Error('missing output location was not found');
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
  if (review(skill).findings.length) throw new Error('valid anchors rejected: ' + JSON.stringify(review(skill).findings));
  writeFileSync(join(skill, 'references/b.md'), '# A\n\n[A](a.md#research-with-current-sources)\n');
  if (!review(skill).findings.some((item) => item.code === 'missing-anchor')) throw new Error('renamed-section anchor was missed');
  writeFileSync(join(skill, 'references/a.md'), '[B](b.md)\n');
  writeFileSync(join(skill, 'references/b.md'), '[A](a.md)\n');
  return source;
}

// Part 3: scalar forms, fenced code, shared tmp paths, and the Resources table.
function selfTestContent(skill, source) {
  source = source.replace('description: "Use when an example needs review."', 'description: |-\n  Review an example.');
  writeFileSync(join(skill, 'SKILL.md'), source);
  if (review(skill).findings.length) throw new Error('block scalar failed');
  writeFileSync(join(skill, 'SKILL.md'), source.replace('description: |-\n  Review an example.', `description: Use when an example\n  ${'needs a long review. '.repeat(60)}`));
  if (!review(skill).findings.some((item) => item.code === 'description-length')) throw new Error('a multi-line plain description escaped the length check');
  writeFileSync(join(skill, 'README.md'), '# Example\n\n```md\n[link](not-here.md) [anchor](#nowhere)\n```\n\n```bash\nnode scripts/example-tool.mjs\n```\n');
  const fenced = review(skill).findings.filter((item) => item.code !== 'description-length');
  if (fenced.some((item) => item.level === 'ERROR') || !fenced.some((item) => item.code === 'missing-support-file-in-code')) throw new Error('fenced code was linted as links, or a missing command went unreported: ' + JSON.stringify(fenced));
  writeFileSync(join(skill, 'README.md'), '# Example\n\nRuns save to `<workspace>/.octocode/tmp/example/<run>`.\n');
  if (!review(skill).findings.some((item) => item.code === 'output-outside-skill-folder')) throw new Error('a shared .octocode/tmp/ output path was not flagged');
  writeFileSync(join(skill, 'README.md'), '# Example\n\nRuns save to `<workspace>/.octocode/example-skill/tmp/<run>`; a scratch folder such as `.octocode/tmp/` is not a report location.\n');
  if (review(skill).findings.some((item) => item.code === 'output-outside-skill-folder')) throw new Error('a skill tmp path or a passing mention was flagged');
  writeFileSync(join(skill, 'README.md'), '# Example\n');
  writeFileSync(join(skill, 'SKILL.md'), source.replace('description: |-\n  Review an example.', 'description:\nlicense: MIT'));
  if (!review(skill).findings.some((item) => item.code === 'description-missing')) throw new Error('empty description consumed the next field');
  writeFileSync(join(skill, 'SKILL.md'), `${source}\n## Resources\n\n| When | Read |\n|---|---|\n| Saving | [output.md](output.md) |\n`);
  if (!review(skill).findings.some((item) => item.code === 'resources-columns')) throw new Error('a Resources table without Why was not flagged');
  writeFileSync(join(skill, 'SKILL.md'), `${source}\n## Resources\n\n| When | Read or run | Why |\n|---|---|---|\n| Saving | [output.md](output.md) | Format |\n`);
  if (review(skill).findings.some((item) => item.code === 'resources-columns')) throw new Error('a valid Resources table was flagged');
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
  const single = join(dir, 'single-skill');
  mkdirSync(single);
  writeFileSync(join(single, 'SKILL.md'), '---\nname: single-skill\ndescription: "Use when one file is enough."\n---\n\n# Single\n\nAnswer inline.\n');
  if (review(single).findings.length) throw new Error('a single-file skill was rejected: ' + JSON.stringify(review(single).findings));
  writeFileSync(join(single, 'notes.md'), '# Notes\n');
  if (!['readme-missing', 'output-missing'].every((code) => review(single).findings.some((item) => item.code === code))) throw new Error('a multi-file skill without README.md and output.md passed');
}

function selfTest() {
  const dir = mkdtempSync(join(tmpdir(), 'skill-review-'));
  try {
    mkdirSync(join(dir, 'example-skill'));
    const skill = join(dir, 'example-skill');
    selfTestBasics(dir, skill);
    selfTestContent(skill, selfTestLinks(skill));
    selfTestCollections(dir, skill);
    console.log('PASS skill-review-self-test');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

const args = process.argv.slice(2);
if (args.includes('--help')) {
  console.log('Usage: node scripts/skill-review.mjs [skill-dir | collection-dir ...] [--json] [--self-test]');
  process.exit(0);
}
if (args.includes('--self-test')) { selfTest(); process.exit(0); }
const unknown = args.find((arg) => arg.startsWith('-') && arg !== '--json');
if (unknown) { console.error(`Unknown option: ${unknown}`); process.exit(1); }
const json = args.includes('--json');
const targets = args.filter((arg) => !arg.startsWith('--')).map((arg) => resolve(arg));
if (!targets.length) targets.push(resolve('skills'));
function skillDirs(target) {
  if (existsSync(join(target, 'SKILL.md'))) return [target];
  if (!existsSync(target) || !statSync(target).isDirectory()) return [target];
  const children = readdirSync(target, { withFileTypes: true })
    .filter((entry) => !entry.name.startsWith('.') && entry.name !== 'node_modules' && (entry.isDirectory() || entry.isSymbolicLink()))
    .map((entry) => join(target, entry.name))
    .filter(dir => existsSync(join(dir, 'SKILL.md')) || !existsSync(join(dir, 'package.json')) ||
      JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8')).private !== true);
  return children.length ? children : [target];
}
const dirs = [...new Set(targets.flatMap(skillDirs))];
const results = dirs.map((dir) => {
  try { return review(dir); }
  catch (error) { return { skill: basename(dir), path: dir, findings: [{ level: 'ERROR', code: 'read-failed', message: error.message }] }; }
});
const errorCount = results.flatMap((item) => item.findings).filter((item) => item.level === 'ERROR').length;
const warnCount = results.flatMap((item) => item.findings).filter((item) => item.level === 'WARN').length;
if (json) console.log(JSON.stringify({ errorCount, warnCount, results }, null, 2));
else {
  for (const item of results) {
    console.log(`${item.skill}: ${item.findings.length ? item.findings.map((f) => `${f.level} ${f.code}: ${f.message}`).join('\n  ') : 'OK'}`);
  }
  console.log(`${errorCount} errors, ${warnCount} warnings`);
}
process.exitCode = errorCount ? 1 : 0;
