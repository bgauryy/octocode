#!/usr/bin/env node
// Structural checks for Agent Skill folders. Trigger quality and prose require human review.
import { existsSync, statSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync, mkdirSync, symlinkSync } from 'node:fs';
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

function frontmatter(source) {
  source = source.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n');
  const match = source.match(/^---\s*\n([\s\S]*?)\n---(?:\n|$)/);
  if (!match) return null;
  const field = (key) => {
    const line = match[1].match(new RegExp(`^${key}:[ \\t]*(.*)$`, 'm'))?.[1]?.trim();
    if (!line) return '';
    if (/^[>|][+-]?(?:\s+#.*)?$/.test(line)) {
      const rest = match[1].split('\n');
      const start = rest.findIndex((item) => item.startsWith(`${key}:`));
      const parts = [];
      for (const item of rest.slice(start + 1)) {
        if (item && !/^\s+/.test(item)) break;
        parts.push(item.trim());
      }
      return parts.join(' ').trim();
    }
    if (line.startsWith('"')) {
      const quoted = line.match(/^"(?:\\.|[^"\\])*"/);
      try { return JSON.parse(quoted?.[0] || ''); } catch { return null; }
    }
    if (line.startsWith("'")) return line.match(/^'((?:''|[^'])*)'/)?.[1]?.replace(/''/g, "'") ?? null;
    if (/^[\[\]{&*!]/.test(line)) return null; // Advanced YAML needs a YAML-aware validator.
    return line.replace(/\s+#.*$/, '').trim();
  };
  return { name: field('name'), description: field('description') };
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

function supportPaths(text) {
  return [...text.matchAll(/(?:^|[^A-Za-z0-9./])((?:scripts|references|assets|docs|benchmarks)\/[A-Za-z0-9._/-]+\.(?:mjs|md|tsv|json|html|js|sh|py))/gm)].map((match) => match[1]);
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
  const skill = texts.get('SKILL.md') || '';
  const fm = frontmatter(skill);
  if (!fm) add('ERROR', 'frontmatter-missing', 'SKILL.md needs YAML frontmatter.');
  if (fm && (fm.name === null || fm.description === null)) add('WARN', 'yaml-unchecked', 'Advanced or invalid YAML: validate with a YAML-aware Agent Skills validator.');
  if (fm && fm.name !== null && fm.name !== name) add('ERROR', 'name-mismatch', `Frontmatter name must match folder ${name}.`);
  if (fm?.name && (fm.name.length > 64 || !/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(fm.name))) {
    add('ERROR', 'name-format', 'Name must follow the Agent Skills identifier syntax.');
  }
  if (fm?.description !== null && !fm?.description) add('ERROR', 'description-missing', 'Description is required.');
  if (fm?.description?.length > 1024) add('ERROR', 'description-length', 'Description exceeds the Agent Skills limit of 1024 characters.');
  if (!files.includes('README.md')) add('ERROR', 'readme-missing', 'README.md is missing.');
  if (!files.includes('output.md')) add('ERROR', 'output-missing', 'output.md is missing.');
  if (![...localPaths(skill)].some((path) => resolve(dir, path) === join(dir, 'output.md'))) add('WARN', 'output-route-missing', 'Link the output format from SKILL.md using any descriptive label.');

  for (const [path, text] of texts) {
    for (const target of localPaths(text)) {
      if (target.includes('<') || target.includes('{') || target.includes('*') || target.includes('…')) continue;
      const full = resolve(dirname(join(dir, path)), target);
      if (!full.startsWith(dir + '/') && full !== dir) continue; // cross-skill handoffs are allowed
      if (!existsSync(full)) add('ERROR', 'missing-path', `${path} references missing ${target}.`);
    }
    for (const target of supportPaths(text)) {
      if (!existsSync(join(dir, target))) add('ERROR', 'missing-support-file', `${path} references missing ${target}.`);
    }
  }
  // Follow actual document routes from the lobby; two orphan pages linking each other do not count.
  const reachable = new Set(['SKILL.md']);
  const pending = ['SKILL.md'];
  while (pending.length) {
    const path = pending.pop();
    const text = texts.get(path) || '';
    const targets = [...localPaths(text)].map((target) => relative(dir, resolve(dir, dirname(path), target)));
    targets.push(...supportPaths(text));
    for (const target of targets) {
      if (files.includes(target) && !reachable.has(target)) {
        reachable.add(target);
        if (texts.has(target)) pending.push(target);
      }
    }
  }
  for (const path of files.filter((item) => item.startsWith('references/') && item.endsWith('.md') || /^scripts\/[^/]+\.(mjs|js|sh|py)$/.test(item))) {
    if (!reachable.has(path)) add('WARN', 'unrouted-resource', `${path} has no route from SKILL.md.`);
  }
  return { skill: name, path: dir, findings };
}

function selfTest() {
  const dir = mkdtempSync(join(tmpdir(), 'skill-review-'));
  try {
    mkdirSync(join(dir, 'example-skill'));
    const skill = join(dir, 'example-skill');
    writeFileSync(join(skill, 'SKILL.md'), '---\nname: example-skill\ndescription: "Use when an example needs review."\n---\n\n# Example\n\n## Related skills\n\n- `other-skill`: For another job.\n\n## Output\n\nSee [output.md](output.md).\n');
    writeFileSync(join(skill, 'README.md'), '# Example\n');
    writeFileSync(join(skill, 'output.md'), '# Output\n\nAn answer.\n');
    if (review(skill).findings.length) throw new Error('valid skill failed');
    writeFileSync(join(skill, 'README.md'), '# Example\n\nSee [missing.md](missing.md).\n');
    if (!review(skill).findings.some((item) => item.code === 'missing-path')) throw new Error('missing path was not found');
    writeFileSync(join(skill, 'README.md'), '# Example\n');
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
    source = source.replace('description: "Use when an example needs review."', 'description: |-\n  Review an example.');
    writeFileSync(join(skill, 'SKILL.md'), source);
    if (review(skill).findings.length) throw new Error('block scalar failed');
    writeFileSync(join(skill, 'SKILL.md'), source.replace('description: |-\n  Review an example.', 'description:\nlicense: MIT'));
    if (!review(skill).findings.some((item) => item.code === 'description-missing')) throw new Error('empty description consumed the next field');
    writeFileSync(join(skill, 'SKILL.md'), source);
    const collection = join(dir, 'collection');
    mkdirSync(collection);
    symlinkSync(skill, join(collection, 'example-skill'), 'dir');
    if (skillDirs(collection).length !== 1 || review(skillDirs(collection)[0]).findings.length) throw new Error('symlinked skill failed');
    mkdirSync(join(collection, 'missing-skill'));
    if (skillDirs(collection).length !== 2) throw new Error('incomplete skill folder was silently skipped');
    const empty = join(dir, 'empty');
    mkdirSync(empty);
    if (!review(skillDirs(empty)[0]).findings.some((item) => item.code === 'skill-missing')) throw new Error('empty collection passed');
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
    .map((entry) => join(target, entry.name));
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
