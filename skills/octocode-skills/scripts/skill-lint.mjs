#!/usr/bin/env node
// skill-lint — lint Agent Skill folders against the octocode-skills standard.
// Usage:  node skill-lint.mjs [skill-dir ...] [--json]
//         no args  -> lint every SKILL.md folder under the repo's skills/ root
// Exit:   0 = no errors, 1 = at least one ERROR finding (warnings never fail).
// Rules documented in ../references/skill-lint.md.

import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import { join, basename, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const LIMITS = { skillMd: 100, reference: 150, refNameChars: 30 };
const GENERIC = new Set(['reference', 'doc', 'docs', 'notes', 'misc', 'stuff', 'temp', 'tmp', 'readme', 'index', 'file', 'data']);
// references.md is the canonical research-audit-trail filename a created skill must carry — not a generic content ref.
const NAME_EXEMPT = new Set(['references.md', 'references-template.md']);
const COND = /\b(when|whenever|if|before|after|during|while|for )\b/i;

const args = process.argv.slice(2);
const asJson = args.includes('--json');
if (args.includes('--help') || args.includes('-h')) {
  console.log('skill-lint [skill-dir ...] [--json]\n  Lints SKILL.md folders. No dirs => scans the repo skills/ root.\n  ERROR fails (exit 1); WARN is advisory. Rules: references/skill-lint.md');
  process.exit(0);
}

function findSkillRoots() {
  // default: the skills/ directory that contains this skill folder
  let dir = HERE;
  for (let i = 0; i < 6; i++) {
    dir = dirname(dir);
    if (basename(dir) === 'skills') return [dir];
  }
  return [join(HERE, '..', '..')];
}

function listSkillDirs(root) {
  if (existsSync(join(root, 'SKILL.md'))) return [root];
  return readdirSync(root)
    .map((n) => join(root, n))
    .filter((p) => { try { return statSync(p).isDirectory() && existsSync(join(p, 'SKILL.md')); } catch { return false; } });
}

function parseFrontmatter(text) {
  const m = text.match(/^---\n([\s\S]*?)\n---/);
  if (!m) return null;
  const fm = {};
  for (const line of m[1].split('\n')) {
    const kv = line.match(/^(\w[\w-]*):\s*(.*)$/);
    if (kv) fm[kv[1]] = kv[2].trim().replace(/^["']|["']$/g, '');
  }
  return fm;
}

function lintSkill(skillDir) {
  const findings = [];
  const add = (sev, rule, msg) => findings.push({ sev, rule, msg });
  const mdPath = join(skillDir, 'SKILL.md');
  const text = readFileSync(mdPath, 'utf8');
  const lines = text.split('\n');

  // E2: frontmatter
  const fm = parseFrontmatter(text);
  if (!fm) add('ERROR', 'frontmatter', 'SKILL.md has no `---` frontmatter block');
  else {
    if (!fm.name) add('ERROR', 'frontmatter', 'frontmatter missing `name`');
    if (!fm.description) add('ERROR', 'frontmatter', 'frontmatter missing `description`');
    // E3 / description style
    else {
      const d = fm.description.trim();
      if (!/^use\b/i.test(d) || !/\bwhen\b/i.test(d.slice(0, 80)))
        add('WARN', 'description-style', 'description should be "Use when ..." style (imperative trigger + when-clause)');
      if (d.length > 1024) add('WARN', 'description-style', `description ${d.length} chars > 1024 limit`);
    }
  }

  // W1: SKILL.md leanness
  if (lines.length > LIMITS.skillMd)
    add('WARN', 'skill-too-long', `SKILL.md is ${lines.length} lines > ${LIMITS.skillMd}; move conditional detail into references/`);

  // W2: must use references
  const refLinks = [...text.matchAll(/references\/([A-Za-z0-9._-]+\.md)/g)].map((m) => m[1]);
  if (refLinks.length === 0)
    add('WARN', 'no-references', 'SKILL.md links no references/*.md; lean skills push conditional detail into references');

  // W5: each reference link line must carry a load condition
  lines.forEach((ln, i) => {
    if (/references\/[A-Za-z0-9._-]+\.md/.test(ln) && !COND.test(ln))
      add('WARN', 'link-no-condition', `line ${i + 1}: reference link lacks a load condition (when/if/before ...)`);
  });

  // references/ files
  const refsDir = join(skillDir, 'references');
  const refFiles = existsSync(refsDir)
    ? readdirSync(refsDir).filter((f) => f.endsWith('.md'))
    : [];
  for (const f of refFiles) {
    const rl = readFileSync(join(refsDir, f), 'utf8').split('\n').length;
    if (rl > LIMITS.reference)
      add('WARN', 'reference-too-long', `references/${f} is ${rl} lines > ${LIMITS.reference}; split it`);
    const stem = basename(f, '.md');
    if (NAME_EXEMPT.has(f)) continue;
    if (!/^[a-z0-9]+(-[a-z0-9]+)*$/.test(stem))
      add('WARN', 'reference-name', `references/${f} is not short kebab-case`);
    else if (stem.replace(/-/g, '').length > LIMITS.refNameChars)
      add('WARN', 'reference-name', `references/${f} name too long; use a short indicative name`);
    if (GENERIC.has(stem))
      add('WARN', 'reference-name', `references/${f} is a generic name; use an indicative one`);
  }

  // W6: linked references that do not exist
  for (const r of new Set(refLinks))
    if (!existsSync(join(refsDir, r)))
      add('ERROR', 'missing-reference', `SKILL.md links references/${r} but the file is missing`);

  // info: cross-routing between references (references calling references)
  let crossLinks = 0;
  for (const f of refFiles)
    crossLinks += [...readFileSync(join(refsDir, f), 'utf8').matchAll(/references?\/[A-Za-z0-9._-]+\.md|\.\.\/references\//g)].length;

  return { skillDir, name: fm?.name ?? basename(skillDir), lines: lines.length, refFiles: refFiles.length, crossLinks, findings };
}

const roots = args.filter((a) => !a.startsWith('-'));
const targets = (roots.length ? roots : findSkillRoots()).flatMap(listSkillDirs);

const results = targets.map(lintSkill);
const totalErr = results.reduce((n, r) => n + r.findings.filter((f) => f.sev === 'ERROR').length, 0);

if (asJson) {
  console.log(JSON.stringify({ results, errors: totalErr }, null, 2));
} else {
  const cwd = process.cwd();
  for (const r of results) {
    const errs = r.findings.filter((f) => f.sev === 'ERROR').length;
    const warns = r.findings.filter((f) => f.sev === 'WARN').length;
    const tag = errs ? 'FAIL' : warns ? 'WARN' : 'PASS';
    console.log(`\n[${tag}] ${r.name}  (${relative(cwd, r.skillDir) || '.'})`);
    console.log(`       ${r.lines} md lines · ${r.refFiles} refs · ${r.crossLinks} cross-links`);
    for (const f of r.findings) console.log(`       ${f.sev === 'ERROR' ? 'x' : '!'} ${f.rule}: ${f.msg}`);
  }
  console.log(`\n${results.length} skills · ${totalErr} errors`);
}
process.exit(totalErr ? 1 : 0);
