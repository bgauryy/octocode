#!/usr/bin/env node
// skill-lint — lint Agent Skill folders against the octocode-skills standard.
// Usage:  node skill-lint.mjs [skill-dir ...] [--json]
//         no args  -> lint every SKILL.md folder under the nearest parent skills/ root
// Exit:   0 = no errors, 1 = at least one ERROR finding (warnings never fail).
// Rules documented in ../references/skill-lint.md.

import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import { join, basename, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const LIMITS = { skillMd: 100, reference: 150, refNameChars: 30, descriptionLead: 50 };
const DESC_LEAD_BLOAT = /\b(this skill|the skill|the following|in order to|assistant should|the user asks you to)\b/i;
const GENERIC = new Set(['reference', 'doc', 'docs', 'notes', 'misc', 'stuff', 'temp', 'tmp', 'readme', 'index', 'file', 'data']);
// references.md is the canonical research-audit-trail filename a created skill must carry — not a generic content ref.
const NAME_EXEMPT = new Set(['references.md', 'references-template.md']);
const REFERENCE_MENTION_EXEMPT = new Set(['references/references.md', 'references/references-template.md']);
const COND = /\b(when|whenever|if|before|after|during|while|for )\b/i;
// Agents read only name+description at discovery; allowed-tools/license affect install; hooks are
// functional (the harness installs them). Anything else in frontmatter is authoring/repo metadata
// that wastes discovery context.
const FM_ALLOWED = new Set(['name', 'description', 'license', 'allowed-tools', 'allowed_tools', 'hooks']);
// Body headings that are authoring/repo metadata, not task instructions the agent acts on.
const META_HEADING = /^#{1,6}\s+(change\s?log|version\s?history|versions?|history|authors?|credits?|acknowledge?ments?|licen[cs]e|metadata|table of contents|contents|toc|todos?|maintainers?|contributing|contributors?)\b/i;

const args = process.argv.slice(2);
const asJson = args.includes('--json');
if (args.includes('--help') || args.includes('-h')) {
  console.log('skill-lint [skill-dir ...] [--json]\n  Lints SKILL.md folders. No dirs => scans the nearest parent skills/ root.\n  ERROR fails (exit 1); WARN is advisory. Rules: references/skill-lint.md');
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

// --- Prompt-hygiene helpers ---

const STOPWORDS = new Set(
  'the and for are but not you all can had her was one our out day get has him his how its may new now old see two way who did any use say she too via per etc only this that with from have been when will they than then them into your also just more other about after before there these their which where what each such some both does even same most well here over used need make very like been set run file path tool load read link list call'.split(' ')
);

function sigTokens(str) {
  return new Set(str.toLowerCase().replace(/[^a-z0-9\s]/g, ' ').split(/\s+/).filter((w) => w.length > 2 && !STOPWORDS.has(w)));
}

function jaccard(a, b) {
  if (!a.size || !b.size) return 0;
  let inter = 0;
  for (const w of a) if (b.has(w)) inter++;
  return inter / (a.size + b.size - inter);
}

function stripNoise(str) {
  // strip frontmatter, code blocks, tables, and HTML tags before prose analysis
  return str
    .replace(/^---[\s\S]*?---\n/, '')
    .replace(/```[\s\S]*?```/g, '')
    .replace(/\|.*\|/g, '')
    .replace(/<[^>]+>/g, '');
}

function splitSentences(str) {
  return str
    .split(/(?<=[.!?])\s+|\n\n+/)
    .map((s) => s.trim())
    .filter((s) => s.split(/\s+/).length >= 10);
}

// Drop blockquotes and list items before the adjacent-similarity (tautology) pass.
// Parallel enumerated/quoted items (e.g. "ADVOCATE rebuttal" / "CRITIC rebuttal") are
// intentionally similar by design, not redundant prose — only narrative sentences should be policed.
function stripStructuralLines(str) {
  return str
    .split('\n')
    .filter((l) => !/^\s*(>|[-*+]\s|\d+[.)]\s)/.test(l))
    .join('\n');
}

function stripFencedLines(str) {
  let inFence = false;
  return str
    .split('\n')
    .map((ln) => {
      if (/^\s*```/.test(ln)) { inFence = !inFence; return ''; }
      return inFence ? '' : ln;
    })
    .join('\n');
}

function referenceMentions(content) {
  return [...stripFencedLines(content).matchAll(/references\/([A-Za-z0-9._-]+\.md)/g)].map((m) => m[1]);
}

function lintSkill(skillDir) {
  const findings = [];
  const add = (sev, rule, msg) => findings.push({ sev, rule, msg });
  const mdPath = join(skillDir, 'SKILL.md');
  const text = readFileSync(mdPath, 'utf8');
  const lines = text.split('\n');

  // E: frontmatter
  const fm = parseFrontmatter(text);
  if (!fm) add('ERROR', 'frontmatter', 'SKILL.md has no `---` frontmatter block');
  else {
    if (!fm.name) add('ERROR', 'frontmatter', 'frontmatter missing `name`');
    if (!fm.description) add('ERROR', 'frontmatter', 'frontmatter missing `description`');
    else {
      const d = fm.description.trim();
      if (!d) add('ERROR', 'frontmatter', 'frontmatter `description` is empty');
      else {
        if (!/^use\b/i.test(d) || !/\bwhen\b/i.test(d))
          add('WARN', 'description-style', 'description should be "Use when ..." style (imperative trigger + when-clause)');
        if (d.length > 1024) add('WARN', 'description-style', `description ${d.length} chars > 1024 limit`);

        const lead = d.slice(0, LIMITS.descriptionLead);
        const whenIdx = d.search(/\bwhen\b/i);
        if (!/^use\s/i.test(d))
          add('WARN', 'description-concise', 'open with "Use when …" — the first ~50 chars are what agents scan');
        if (whenIdx === -1)
          add('WARN', 'description-concise', 'include a when-clause near the start (trigger intent in the first ~50 chars)');
        else if (whenIdx > LIMITS.descriptionLead)
          add('WARN', 'description-concise', `"when" appears at char ${whenIdx + 1}; move the trigger into the first ${LIMITS.descriptionLead} chars`);
        // No total-length penalty here: Anthropic's limit is 1024 (enforced by description-style above),
        // and descriptions should be trigger-rich/"pushy" to avoid undertriggering. Policing the LEAD
        // (hook quality) matters; capping total length would push authors to delete useful triggers.
        if (DESC_LEAD_BLOAT.test(lead))
          add('WARN', 'description-concise', `first ${LIMITS.descriptionLead} chars waste space on meta wording — lead with concrete triggers: "${lead.trim()}…"`);
        const leadWords = lead.trim().split(/\s+/).filter(Boolean).length;
        if (leadWords > 10)
          add('WARN', 'description-concise', `first ${LIMITS.descriptionLead} chars are ${leadWords} words; tighten the opening hook`);
      }
    }
    // W: redundant frontmatter metadata — agents read only name/description at discovery
    const extraKeys = Object.keys(fm).filter((k) => !FM_ALLOWED.has(k.toLowerCase()));
    if (extraKeys.length)
      add('WARN', 'frontmatter-metadata', `frontmatter carries agent-irrelevant keys (${extraKeys.join(', ')}); agents read only name/description at discovery — drop authoring metadata (version/author/tags/dates) from SKILL.md`);
  }

  // W: SKILL.md leanness
  if (lines.length > LIMITS.skillMd)
    add('WARN', 'skill-too-long', `SKILL.md is ${lines.length} lines > ${LIMITS.skillMd}; move conditional detail into references/`);

  // W: must use references
  const refLinks = referenceMentions(text);
  if (refLinks.length === 0)
    add('WARN', 'no-references', 'SKILL.md links no references/*.md; lean skills push conditional detail into references');

  // W: each reference link line must carry a load condition
  lines.forEach((ln, i) => {
    if (/references\/[A-Za-z0-9._-]+\.md/.test(ln) && !COND.test(ln))
      add('WARN', 'link-no-condition', `line ${i + 1}: reference link lacks a load condition (when/if/before ...)`);
  });

  // references/ files — read all once
  const refsDir = join(skillDir, 'references');
  const refFiles = existsSync(refsDir)
    ? readdirSync(refsDir).filter((f) => f.endsWith('.md'))
    : [];
  const refContents = refFiles.map((f) => ({
    name: f,
    label: `references/${f}`,
    content: readFileSync(join(refsDir, f), 'utf8'),
  }));

  for (const { name: f, label, content: rc } of refContents) {
    const rl = rc.split('\n').length;
    if (rl > LIMITS.reference)
      add('WARN', 'reference-too-long', `${label} is ${rl} lines > ${LIMITS.reference}; split it`);
    const stem = basename(f, '.md');
    if (NAME_EXEMPT.has(f)) continue;
    if (!/^[a-z0-9]+(-[a-z0-9]+)*$/.test(stem))
      add('WARN', 'reference-name', `${label} is not short kebab-case`);
    else if (stem.replace(/-/g, '').length > LIMITS.refNameChars)
      add('WARN', 'reference-name', `${label} name too long; use a short indicative name`);
    if (GENERIC.has(stem))
      add('WARN', 'reference-name', `${label} is a generic name; use an indicative one`);
  }

  // --- All-files collection for cross-file checks ---
  const allMdFiles = [
    { label: 'SKILL.md', content: text },
    ...refContents.map((r) => ({ label: r.label, content: r.content })),
  ];

  // E: linked or mentioned references that do not exist. Ignore fenced examples.
  for (const { label, content } of allMdFiles) {
    if (REFERENCE_MENTION_EXEMPT.has(label)) continue;
    for (const r of new Set(referenceMentions(content))) {
      if (!existsSync(join(refsDir, r)))
        add('ERROR', 'missing-reference', `${label} mentions references/${r} but the file is missing`);
    }
  }

  // E: links that escape the skill folder (must use GitHub URLs instead)
  const mdLinkRe = /\[([^\]]*)\]\(([^)]+)\)/g;
  for (const { label, content } of allMdFiles) {
    stripFencedLines(content).split('\n').forEach((ln, i) => {
      for (const m of ln.matchAll(mdLinkRe)) {
        const href = m[2].split(/[#?]/)[0].trim();
        if (href.startsWith('../') || /^[/~]/.test(href) || href.startsWith('file://'))
          add('ERROR', 'link-outside-skill', `${label} line ${i + 1}: "${href}" escapes the skill folder — use a GitHub URL instead`);
      }
    });
  }

  // W: redundant metadata/authoring sections — not instructions the agent acts on
  for (const { label, content } of allMdFiles) {
    let inFence = false;
    content.split('\n').forEach((ln, i) => {
      if (/^\s*```/.test(ln)) { inFence = !inFence; return; }
      if (inFence) return;
      if (META_HEADING.test(ln))
        add('WARN', 'metadata-section', `${label} line ${i + 1}: "${ln.trim()}" is authoring/repo metadata, not agent instruction — keep changelogs/authors/version notes in the repo README, not the skill`);
    });
  }

  // --- Prompt-hygiene checks ---

  for (const { label, content } of allMdFiles) {
    const body = stripNoise(content);
    const bodyLines = body.split('\n').filter((l) => l.trim());

    // W-rigid: high density of imperative modals
    const rigidHits = [...body.matchAll(/\b(MUST|NEVER|ALWAYS|FORBIDDEN|REQUIRED)\b/g)];
    if (bodyLines.length > 0 && rigidHits.length / bodyLines.length > 0.12)
      add('WARN', 'rigid', `${label}: ${rigidHits.length} rigid modals (MUST/NEVER/ALWAYS/FORBIDDEN) in ${bodyLines.length} lines — prefer defaults with escape hatches; reserve these for fragile/destructive steps`);

    // W-verbose: filler phrases
    const FILLER = /\b(in order to|it is important|make sure to|please note|note that|as mentioned|be sure to|ensure that you|take care to)\b/i;
    let fillerCount = 0;
    content.split('\n').forEach((ln, i) => {
      if (FILLER.test(ln) && ++fillerCount <= 3)
        add('WARN', 'verbose', `${label} line ${i + 1}: filler phrase — rewrite concisely`);
    });

    // W-tautology: adjacent sentences with high token overlap (narrative prose only —
    // parallel blockquote/list items are intentionally similar, so exclude them)
    const sents = splitSentences(stripStructuralLines(body));
    for (let i = 0; i < sents.length - 1; i++) {
      const a = sigTokens(sents[i]);
      const b = sigTokens(sents[i + 1]);
      if (a.size >= 5 && b.size >= 5 && jaccard(a, b) > 0.75) {
        const sim = Math.round(jaccard(a, b) * 100);
        add('WARN', 'tautology', `${label}: adjacent sentences are ${sim}% similar — one is likely redundant:\n         · "${sents[i].slice(0, 90)}"\n         · "${sents[i + 1].slice(0, 90)}"`);
      }
    }

    // W-contradiction: verb appears after both MUST/ALWAYS and NEVER/MUST NOT in same file
    const mustVerbs = new Set([...body.matchAll(/\b(?:MUST|ALWAYS)\s+(\w+)/gi)].map((m) => m[1].toLowerCase()));
    const neverVerbs = new Set([...body.matchAll(/\b(?:NEVER|MUST NOT|must not|do not)\s+(\w+)/gi)].map((m) => m[1].toLowerCase()));
    for (const v of mustVerbs)
      if (neverVerbs.has(v))
        add('WARN', 'contradiction', `${label}: "${v}" follows both MUST/ALWAYS and NEVER/MUST NOT — check for conflicting instructions`);
  }

  // W-duplicate: same non-trivial sentence (≥12 words) in 2+ locations across the skill
  const seen = new Map(); // normalised sentence → first label
  for (const { label, content } of allMdFiles) {
    const body = stripNoise(content);
    for (const s of splitSentences(body)) {
      const words = s.split(/\s+/);
      if (words.length < 12) continue;
      const key = s.toLowerCase().replace(/\s+/g, ' ').trim();
      if (!seen.has(key)) { seen.set(key, label); continue; }
      const first = seen.get(key);
      if (first !== label)
        add('WARN', 'duplicate-content', `Sentence duplicated in ${first} and ${label}: "${s.slice(0, 100)}…"`);
    }
  }

  // info: cross-routing between references
  let crossLinks = 0;
  for (const { content } of refContents)
    crossLinks += [...content.matchAll(/references?\/[A-Za-z0-9._-]+\.md|\.\.\/references\//g)].length;

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
