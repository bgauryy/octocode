#!/usr/bin/env node
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const skillRoot = resolve(here, '..');
const defaultRoot = resolve(skillRoot, '..');
const args = process.argv.slice(2);
const json = args.includes('--json');
const targets = args.filter((a) => !a.startsWith('--'));

if (args.includes('--help')) {
  console.log(`skill-review — structure, reference, and navigation gates for Agent Skill folders

  node scripts/skill-review.mjs [skill-or-collection-folders...] [--json]

  no folders   every skill under the nearest skills/ root (or the current folder if it is a skill)
  folder       one skill folder, or a collection whose immediate children are skill folders
  --json       machine-readable findings
  --self-test  run collection, usage-error, and frontmatter-route regressions
  --help       this text

Navigation gates treat the skill as a map: SKILL.md is the lobby that lists every reference and
script with when and how to use it plus the workflow, each route says when to load it, and each
reference points onward. Exit 1 on any ERROR.`);
  process.exit(0);
}

function isSkillDir(dir) {
  return existsSync(join(dir, 'SKILL.md')) && statSync(join(dir, 'SKILL.md')).isFile();
}

function expandTarget(target) {
  const dir = resolve(process.cwd(), target);
  if (!existsSync(dir)) throw new Error(`target does not exist: ${target}`);
  if (!statSync(dir).isDirectory()) throw new Error(`target is not a directory: ${target}`);
  if (isSkillDir(dir)) return [dir];

  const children = readdirSync(dir)
    .map((name) => join(dir, name))
    .filter((child) => statSync(child).isDirectory() && isSkillDir(child));
  if (!children.length) throw new Error(`target contains no skill folders: ${target}`);
  return children;
}

function discoverTargets() {
  if (targets.length) return targets.flatMap(expandTarget);
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

function bodyWithoutFrontmatter(text) {
  return text.replace(/^---\n[\s\S]*?\n---\n/, '');
}

/** Lines of SKILL.md that name a reference or script, so a route can be judged in isolation. */
function routeLines(text) {
  return bodyWithoutFrontmatter(text).split(/\r?\n/)
    .filter((line) => /(?:references|scripts)\//.test(line) && !/^\s*(?:```|#)/.test(line));
}

/** A route earns its place by saying when or why to load the target. */
const ROUTE_CONDITION = /\b(when|whenever|before|after|if|unless|during|while|load|read|use|run|start|then|for)\b/i;

/** A chunk announces its own entry condition in its opening lines. */
const ENTRY_CUE = /\b(load when|use when|read when|apply when|when you|before |after |load for|load to)\b/i;

/** A named directory (`assets/hooks/`) stands in for the files under it. */
function mentionedDirs(text) {
  return [...new Set((text.match(/(?:references|scripts|assets)\/[A-Za-z0-9._-]*\//g) || []))];
}

/** Every runnable file under scripts/, so the lobby can be checked for completeness. */
function scriptFiles(dir, sub = 'scripts') {
  const base = join(dir, sub);
  if (!existsSync(base)) return [];
  return readdirSync(base).flatMap((name) => {
    const full = join(base, name);
    if (statSync(full).isDirectory()) return scriptFiles(dir, `${sub}/${name}`);
    return /\.(mjs|js|sh|py)$/.test(name) ? [`${sub}/${name}`] : [];
  });
}

/** A skill folder installs on its own, so it must not depend on a file outside itself. A bare `../name`
 *  is left alone: it is a directory argument (a skill under review), not a dependency. */
const OUTSIDE_DEP = /\.\.\/(?:[A-Za-z0-9._-]+\/)*[A-Za-z0-9._-]+\.[A-Za-z0-9]{1,5}|~\/[^\s`)]+\.[A-Za-z0-9]{1,5}|file:\/\/[^\s`)]+|(?:^|[\s`("])\/(?:Users|home|etc|opt|var)\/[^\s`)"]+/;

/** Audit trails, templates, and fixtures are carried data, exempt from entry and exit cues. */
const DATA_ARTIFACT = /(?:^|\/)references\.md$|template|appendix|fixture/i;

/** An onward pointer keeps navigation moving instead of dead-ending in a leaf. */
const ONWARD_CUE = /^\s*(?:next|then|return|back|continue|see also)\b/im;

/** Phase tokens from a `Flow:` line — ALL-CAPS steps joined by arrows. */
function flowPhases(text) {
  const line = text.split(/\r?\n/).find((l) => /^\s*(?:\*\*)?Flow:?/i.test(l));
  if (!line) return [];
  return [...new Set((line.match(/\b[A-Z][A-Z0-9 ]{2,}\b/g) || [])
    .map((p) => p.trim())
    .filter((p) => p && p !== 'FLOW' && p !== 'SKILL'))];
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
  const fromLobby = new Set(linkedPaths(skill));
  const refTexts = new Map();
  if (existsSync(refsDir)) {
    for (const file of readdirSync(refsDir).filter((f) => f.endsWith('.md'))) {
      const rel = `references/${file}`;
      const text = readFileSync(join(refsDir, file), 'utf8');
      const refLines = text.trimEnd().split(/\r?\n/).length;
      refTexts.set(rel, text);
      for (const p of linkedPaths(text)) referenced.add(p);
      if (!/^#\s+/m.test(text)) warn('reference-h1', `${rel} should have an H1.`);
      if (refLines > 50) warn('reference-long', `${rel} is ${refLines} lines; the limit is 50 — split it or cut filler.`);
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

  // Navigation gates: the lobby is the map. It lists every reference and script with when/how, plus the workflow.
  const dirs = mentionedDirs(skill);
  const listedInLobby = (rel) => fromLobby.has(rel) || skill.includes(rel) || dirs.some((d) => rel.startsWith(d));

  if (!/^\s*(?:\*\*)?(?:flow|workflow)/im.test(skill) && !/^##+\s+workflow/im.test(skill)) {
    warn('lobby-workflow-missing', 'SKILL.md must show the workflow on its own line so it is scannable — a `Flow:` line or a `## Workflow` heading. A flow trailing mid-sentence does not count.');
  }

  const scripts = scriptFiles(dir).map((rel) => ({ rel, text: readFileSync(join(dir, rel), 'utf8') }));
  const imported = new Set(scripts.flatMap(({ text }) =>
    [...text.matchAll(/(?:from|import)\s+['"]\.\/([A-Za-z0-9._-]+)['"]/g)].map((m) => `scripts/${m[1]}`)));
  // Paths the docs show being executed count as entry points even without a shebang.
  const corpus = [skill, ...refTexts.values()].join('\n');
  const invoked = new Set([...corpus.matchAll(/(?:node|bash|sh|python3?)\s+((?:\.\/)?scripts\/[A-Za-z0-9._\/-]+)/g)]
    .map((m) => m[1].replace(/^\.\//, '')));

  for (const { rel, text } of scripts) {
    // A runnable script is an entry point the lobby must name; a library module is implementation detail
    // that something must import — otherwise it is dead weight in a folder that ships as-is.
    const runnable = /^#!/.test(text) || invoked.has(rel);
    if (runnable && !listedInLobby(rel)) {
      warn('lobby-script-unlisted', `${rel} is not listed in SKILL.md; the lobby names every script with when and how to run it.`);
    }
    if (!runnable && !imported.has(rel) && !listedInLobby(rel)) {
      warn('script-unreferenced', `${rel} is a library nothing imports and the lobby never names; import it or drop it.`);
    }
  }

  for (const [rel, text] of refTexts) {
    if (!referenced.has(rel)) continue; // already reported as orphan-reference
    if (!listedInLobby(rel)) {
      warn('lobby-reference-unlisted', `${rel} is reachable only through another reference; the lobby must list every reference with when to read it.`);
    }
    if (DATA_ARTIFACT.test(rel)) continue; // audit trails and templates are data, not map nodes
    const head = text.split(/\r?\n/).filter(Boolean).slice(0, 5).join(' ');
    if (!ENTRY_CUE.test(head)) {
      warn('reference-entry-cue', `${rel} should open by saying when to load it ("Load when …").`);
    }
    if (refTexts.size >= 3 && !linkedPaths(text).length && !ONWARD_CUE.test(text)) {
      warn('reference-dead-end', `${rel} points nowhere; add the next hop or say the step ends here.`);
    }
  }

  // A table row carries its condition in the left cell, so only prose routes are judged on their own line.
  const topLevelMd = readdirSync(dir).filter((f) => /\.md$/i.test(f) && f !== 'SKILL.md')
    .map((f) => [f, readFileSync(join(dir, f), 'utf8')]);
  for (const [rel, text] of [['SKILL.md', skill], ...topLevelMd, ...refTexts]) {
    for (const [i, line] of text.split(/\r?\n/).entries()) {
      const hit = line.match(OUTSIDE_DEP);
      // A path carrying a placeholder (`<abs>`, `...`, `$HOME`, `{dir}`) is a template the reader fills in,
      // not a file this folder depends on.
      if (hit && !/<[^>]*>|\.\.\.|\$[A-Za-z{]|\{/.test(hit[0])) {
        error('link-outside-skill', `${rel}:${i + 1} depends on ${hit[0].trim()} outside the folder; the skill installs alone — vendor it or drop it.`);
      }
    }
  }

  for (const line of routeLines(skill)) {
    if (/^\s*\|/.test(line)) continue;
    if (!ROUTE_CONDITION.test(line)) {
      warn('route-condition', `route has no when/why cue: "${line.trim().slice(0, 70)}"`);
    }
  }

  // A phase must be routed from the lobby itself — the map is SKILL.md, not the corpus. Stem matching
  // keeps DISCOVER covered by "discovering".
  const lobbyText = skill.toLowerCase();
  for (const phase of flowPhases(skill)) {
    const stem = phase.toLowerCase().replace(/[^a-z ]/g, '').split(' ').pop().replace(/e$/, '');
    if (stem.length < 3) continue;
    if (lobbyText.split(stem).length - 1 < 2) {
      warn('flow-phase-unrouted', `flow phase ${phase} is named only in the flow line; route it from SKILL.md or drop it.`);
    }
  }

  return { skill: name, path: dir, findings };
}

function selfTest() {
  const root = mkdtempSync(join(tmpdir(), 'skill-review-self-test-'));
  const skillDir = join(root, 'hook-skill');
  try {
    mkdirSync(join(skillDir, 'scripts'), { recursive: true });
    writeFileSync(join(skillDir, 'README.md'), '# Hook skill\n');
    writeFileSync(join(skillDir, 'scripts', 'hook.sh'), '#!/bin/sh\nexit 0\n');
    writeFileSync(join(skillDir, 'SKILL.md'), `---
name: hook-skill
description: "Use when testing hook-frontmatter routing."
hooks:
  SessionEnd: [{ hooks: [{ type: command, command: "\${CLAUDE_SKILL_DIR}/scripts/hook.sh" }] }]
---
# Hook skill
Flow: RUN
## Run
Run the hook test and stop.
`);

    const expanded = expandTarget(root);
    if (expanded.length !== 1 || expanded[0] !== skillDir) throw new Error('collection discovery regression');
    const findings = checkSkill(skillDir).findings;
    if (findings.length) throw new Error(`frontmatter route regression: ${JSON.stringify(findings)}`);
    let rejectedMissing = false;
    try { expandTarget(join(root, 'missing')); } catch { rejectedMissing = true; }
    if (!rejectedMissing) throw new Error('missing target must be rejected');
    console.log('PASS skill-review-self-test');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

if (args.includes('--self-test')) {
  selfTest();
  process.exit(0);
}

let results;
try {
  results = discoverTargets().map(checkSkill);
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  if (json) console.error(JSON.stringify({ error: message }, null, 2));
  else console.error(`skill-review: ${message}`);
  process.exit(2);
}
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
