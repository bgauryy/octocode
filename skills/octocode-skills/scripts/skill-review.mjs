#!/usr/bin/env node
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const skillRoot = resolve(here, '..');
const defaultRoot = resolve(skillRoot, '..');
const args = process.argv.slice(2);
const json = args.includes('--json');
const targets = args.filter((a) => a !== '--json');

function isSkillDir(dir) {
  return existsSync(join(dir, 'SKILL.md')) && statSync(join(dir, 'SKILL.md')).isFile();
}

function discoverTargets() {
  if (targets.length) return targets.map((t) => resolve(process.cwd(), t));
  if (isSkillDir(process.cwd())) return [process.cwd()];
  if (isSkillDir(skillRoot)) {
    return readdirSync(defaultRoot)
      .map((name) => join(defaultRoot, name))
      .filter((dir) => statSync(dir).isDirectory() && isSkillDir(dir));
  }
  return [];
}

function frontmatter(text) {
  const match = text.match(/^---\n([\s\S]*?)\n---\n/);
  if (!match) return null;
  const out = {};
  for (const line of match[1].split(/\r?\n/)) {
    const m = line.match(/^([A-Za-z0-9_-]+):\s*(.*)$/);
    if (m) out[m[1]] = m[2].replace(/^['"]|['"]$/g, '').trim();
  }
  return out;
}

function linkedPaths(text) {
  const hits = [];
  const rx = /`((?:references|scripts|assets)\/[^`]+?)`|\((?:(\.\/)?((?:references|scripts|assets)\/[^)]+))\)/g;
  let m;
  while ((m = rx.exec(text))) {
    const raw = (m[1] || m[3]).split('#')[0].trim();
    const cleaned = raw.split(/\s+/)[0].replace(/[.,;:]$/, '');
    if (!cleaned.includes('*')) hits.push(cleaned);
  }
  return [...new Set(hits.filter(Boolean))];
}

function checkSkill(dir) {
  const findings = [];
  const skillPath = join(dir, 'SKILL.md');
  const skill = readFileSync(skillPath, 'utf8');
  const fm = frontmatter(skill);
  const lines = skill.trimEnd().split(/\r?\n/).length;
  const name = fm?.name || basename(dir);

  const error = (code, message) => findings.push({ level: 'ERROR', code, message });
  const warn = (code, message) => findings.push({ level: 'WARN', code, message });

  if (!fm) error('frontmatter-missing', 'SKILL.md must start with YAML frontmatter.');
  if (fm && fm.name !== basename(dir)) error('name-mismatch', `frontmatter name (${fm.name}) must match folder (${basename(dir)}).`);
  if (!fm?.description) error('description-missing', 'frontmatter description is required.');
  if (fm?.description && !/^Use when\b/i.test(fm.description.replace(/^>-\s*/, '').trim())) {
    warn('description-trigger', 'description should lead with “Use when …”.');
  }
  if (fm?.description && fm.description.length > 1024) error('description-too-long', 'description must be <=1024 chars.');
  if (lines > 220) warn('lobby-long', `SKILL.md is ${lines} lines; keep the lobby lean when possible.`);

  if (!existsSync(join(dir, 'README.md'))) warn('readme-missing', 'README.md is recommended for standalone skills.');

  const refsDir = join(dir, 'references');
  const referenced = new Set(linkedPaths(skill));
  if (existsSync(refsDir)) {
    for (const file of readdirSync(refsDir).filter((f) => f.endsWith('.md'))) {
      const rel = `references/${file}`;
      const text = readFileSync(join(refsDir, file), 'utf8');
      const refLines = text.trimEnd().split(/\r?\n/).length;
      for (const p of linkedPaths(text)) referenced.add(p);
      if (!/^#\s+/m.test(text)) warn('reference-h1', `${rel} should have an H1.`);
      if (refLines > 80) warn('reference-long', `${rel} is ${refLines} lines; prefer one-concept refs.`);
    }
  }

  for (const rel of referenced) {
    if (rel.includes('://')) continue;
    if (!existsSync(join(dir, rel))) error('missing-route', `${rel} is referenced but missing.`);
  }

  if (existsSync(refsDir)) {
    for (const file of readdirSync(refsDir).filter((f) => f.endsWith('.md'))) {
      const rel = `references/${file}`;
      if (file !== 'references.md' && !referenced.has(rel) && !skill.includes(rel)) {
        warn('orphan-reference', `${rel} is not routed from SKILL.md or another reference.`);
      }
    }
  }

  return { skill: name, path: dir, findings };
}

const results = discoverTargets().map(checkSkill);
const errorCount = results.flatMap((r) => r.findings).filter((f) => f.level === 'ERROR').length;
const warnCount = results.flatMap((r) => r.findings).filter((f) => f.level === 'WARN').length;

if (json) {
  console.log(JSON.stringify({ errorCount, warnCount, results }, null, 2));
} else {
  console.log(`skill-review: ${results.length} skill(s), ${errorCount} ERROR, ${warnCount} WARN`);
  for (const r of results) {
    if (!r.findings.length) continue;
    console.log(`\n${r.skill}`);
    for (const f of r.findings) console.log(`  ${f.level} ${f.code}: ${f.message}`);
  }
}
process.exit(errorCount ? 1 : 0);
