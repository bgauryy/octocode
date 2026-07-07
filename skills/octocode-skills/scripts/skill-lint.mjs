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
const LIMITS = { skillMd: 50, reference: 150, refNameChars: 30, descriptionLead: 50, deterministicItems: 4 };
const ROUTE_DESCRIPTION_WORDS = 28;
const DESC_LEAD_BLOAT = /\b(this skill|the skill|the following|in order to|assistant should|the user asks you to)\b/i;
const SCRIPT_EXT = /\.(?:mjs|cjs|js|ts|py|sh|bash|zsh)$/;
const SCRIPT_ARGS = /\b(process\.argv|process\.env|parseArgs|argparse|commander|yargs|getopts|sys\.argv|os\.environ|stdin)\b|readFileSync\(0\)|\$[@1-9]/;
const SCRIPT_HELP = /(?:--help\b|\B-h\b|\bUsage:|\busage:|help text|argparse|commander|yargs|parseArgs)/i;
const SCRIPT_INTERACTIVE = /(?:require\(['"]readline|from ['"]readline|createInterface\s*\(|\binquirer\.|\bprompt\s*\(|read\s+-p|select\s+\w+\s+in)/i;
const INSTALL_COMMAND = /\bnpx\s+octocode\s+skill\b/;
const DETERMINISTIC_VERB = /\b(run|execute|call|parse|validate|check|generate|write|create|copy|sync|install|fetch|download|convert|extract|render|format|sort|dedupe|scan|lint|test)\b/i;
const JUDGMENT_WORD = /\b(judge|judgment|recommend|approval|user|fit|candidate|inspect|evidence|decision|blocker|handoff|worth|safe|risk)\b/i;
const COMMAND_WORD = /\b(?:node|python3?|bash|sh|yarn|npm|pnpm|npx|rg|git)\b/i;
const CRITICAL_LINE = /\b(MUST|NEVER|ALWAYS|FORBIDDEN|REQUIRED|CRITICAL|STOP|HALT)\b/;
const WEAK_WORD = /\b(consider|might|could|may|should|prefer|as needed|if necessary|feel free to|you can)\b/i;
const AMBIGUOUS_ACTION = /\b(do some|handle\s+\w+(?:\s+\w+){0,3}\s+appropriately|process\s+\w+(?:\s+\w+){0,3}\s+accordingly|as needed|if necessary)\b/i;
const DECISION_IF = /\b(?:\*\*)?IF(?:\*\*)?\b/;
const DECISION_THEN = /\b(?:\*\*)?THEN(?:\*\*)?\b/;
const REFERENTIAL_START = /^\s*(?:[-*+]\s+|\d+[.)]\s+)?(?:it|this|that|above|below)\b/i;
const REFERENTIAL_EXPLICIT = /^\s*(?:[-*+]\s+|\d+[.)]\s+)?this\s+(?:skill|file|reference|section|rule|check|script|command|step|flow|mode|guide|document|folder)\b/i;
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

function frontmatterBlock(text) {
  const m = text.match(/^---\n([\s\S]*?)\n---/);
  return m ? m[1] : null;
}

function parseFrontmatter(text) {
  const raw = frontmatterBlock(text);
  if (raw == null) return null;
  const fm = {};
  for (const line of raw.split('\n')) {
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

function scriptMentions(content) {
  return [...stripFencedLines(content).matchAll(/scripts\/([A-Za-z0-9._/-]+)/g)].map((m) =>
    m[1].replace(/[`'"),.;:]+$/g, '')
  );
}

function listRelativeFiles(dir, prefix = '') {
  if (!existsSync(dir)) return [];
  const out = [];
  for (const name of readdirSync(dir)) {
    if (name.startsWith('.')) continue;
    const full = join(dir, name);
    const rel = prefix ? `${prefix}/${name}` : name;
    let st;
    try { st = statSync(full); } catch { continue; }
    if (st.isDirectory()) out.push(...listRelativeFiles(full, rel));
    else out.push(rel);
  }
  return out;
}

function headingLines(content) {
  return stripFencedLines(content)
    .split('\n')
    .map((line, idx) => ({ line, idx }))
    .filter(({ line }) => /^#{1,6}\s+\S/.test(line))
    .map(({ line, idx }) => {
      const m = line.match(/^(#{1,6})\s+(.+?)\s*#*\s*$/);
      return { level: m[1].length, text: m[2].trim(), line: idx + 1 };
    });
}

function sectionBlocks(content) {
  const blocks = [];
  let current = null;
  const lines = content.split('\n');
  lines.forEach((line, idx) => {
    const m = line.match(/^(#{1,6})\s+(.+?)\s*#*\s*$/);
    if (m) {
      if (current) blocks.push(current);
      current = { level: m[1].length, heading: m[2].trim(), line: idx + 1, lines: [] };
      return;
    }
    if (current) current.lines.push(line);
  });
  if (current) blocks.push(current);
  return blocks;
}

function isCodeScript(name) {
  return SCRIPT_EXT.test(name);
}

function isHookScript(name) {
  return name.split('/').includes('hooks');
}

function isGeneratedSupportScript(name) {
  return basename(name) === 'octocode-config.mjs';
}

function isAgentFacingScript(name) {
  return isCodeScript(name) && !isHookScript(name) && !isGeneratedSupportScript(name);
}

function hasScript(scriptFiles, mention) {
  if (!mention || mention.includes('*') || mention.endsWith('/')) return true;
  if (!SCRIPT_EXT.test(mention) && scriptFiles.some((f) => f.startsWith(`${mention}/`))) return true;
  return scriptFiles.includes(mention);
}

function escapeRe(str) {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function routeLines(lines, resourcePath) {
  const re = new RegExp(escapeRe(resourcePath));
  return lines.filter((line) => re.test(line));
}

function routeDescription(line, resourcePath) {
  const idx = line.indexOf(resourcePath);
  if (idx === -1) return '';
  return line
    .slice(idx + resourcePath.length)
    .replace(/^[`'")\]\s]*(?:—|-|:)?\s*/, '')
    .trim();
}

function isConciseRoute(line, resourcePath) {
  const desc = routeDescription(line, resourcePath);
  const words = desc.split(/\s+/).filter(Boolean);
  return words.length >= 3 && words.length <= ROUTE_DESCRIPTION_WORDS;
}

function hasInstallationResource(refFiles, scriptFiles) {
  return refFiles.some((f) => /\binstall|installation/i.test(f)) ||
    scriptFiles.some((f) => /(?:^|\/)install[^/]*\.(?:mjs|cjs|js|ts|py|sh|bash|zsh)$/i.test(f));
}

function hasInstallationHeading(content) {
  return headingLines(content).some((h) => h.level <= 3 && /^install(?:ation)?\b/i.test(h.text));
}

function hasHeading(content, re) {
  return headingLines(content).some((h) => h.level <= 3 && re.test(h.text));
}

function readmeHasOverview(content) {
  const h1 = headingLines(content).some((h) => h.level === 1);
  const early = stripFencedLines(content)
    .split('\n')
    .filter((line) => line.trim())
    .slice(0, 8)
    .join(' ');
  return h1 && early.split(/\s+/).length >= 18 && /\b(skill|helps|use|workflow|agent|capabilit|feature)\b/i.test(early);
}

function readmeHasInstall(content) {
  return hasHeading(content, /^install(?:ation)?\b/i) &&
    INSTALL_COMMAND.test(content) &&
    /(?:^|\s)(?:--name|--add|--install-all)\b/.test(content);
}

function hasFormatCue(sectionText) {
  return /```/.test(sectionText) || /^\s*\|.*\|\s*$/m.test(sectionText) || /\[[A-Z][A-Za-z0-9 _/-]{2,}\]/.test(sectionText);
}

function isRuleDocLine(line) {
  return /^\s*[-*+]\s+`[a-z0-9-]+`\s+—/.test(line);
}

function hasGateCue(content) {
  return /(?:^|\n)#{2,6}\s+Gate Check\b/i.test(content) || /<\w+_gate>/.test(content) || /\bSTOP\. DO NOT proceed\b/i.test(content);
}

function gateMissingParts(content) {
  if (!hasGateCue(content)) return [];
  return ['Pre-Conditions', 'Gate Check', 'FORBIDDEN', 'ALLOWED', 'On Failure'].filter((part) =>
    !new RegExp(`(?:^|\\n)#{2,6}\\s+${escapeRe(part)}\\b`, 'i').test(content)
  );
}

function xmlTagCount(content) {
  const stripped = stripFencedLines(content)
    .replace(/^---[\s\S]*?---\n/, '')
    .replace(/`[^`]*`/g, '');
  const matches = [...stripped.matchAll(/<\/?([a-z][a-z0-9_-]*)(\s[^>]*)?>/gi)];
  const closingNames = new Set(matches.filter((m) => m[0].startsWith('</')).map((m) => m[1].toLowerCase()));
  return matches
    .filter((m) => !/^<https?:/i.test(m[0]))
    .filter((m) => closingNames.has(m[1].toLowerCase()) || Boolean(m[2]?.trim()))
    .length;
}

function lowDensitySections(content) {
  const out = [];
  for (const block of sectionBlocks(content)) {
    const nonblank = block.lines.filter((line) => line.trim());
    if (nonblank.length <= 20) continue;
    const structured = nonblank.filter((line) =>
      /^\s*(?:[-*+]|\d+[.)]|\|)/.test(line) || /```/.test(line) || CRITICAL_LINE.test(line) || DECISION_THEN.test(line)
    ).length;
    if (structured / nonblank.length < 0.2)
      out.push({ heading: block.heading, line: block.line, lines: nonblank.length });
  }
  return out;
}

function commandishCount(str) {
  let count = COMMAND_WORD.test(str) ? 1 : 0;
  for (const m of str.matchAll(/`([^`]+)`/g)) {
    const code = m[1].trim();
    if (/^(?:node|python3?|bash|sh|yarn|npm|pnpm|npx|rg|git|\.\/|~\/|\/)/i.test(code) || /--[a-z0-9-]+/i.test(code))
      count++;
  }
  return count;
}

function orderedListGroups(content) {
  const groups = [];
  let group = [];
  for (const line of stripFencedLines(content).split('\n')) {
    const m = line.match(/^\s*\d+[.)]\s+(.+)/);
    if (m) {
      group.push(m[1]);
      continue;
    }
    if (group.length) groups.push(group);
    group = [];
  }
  if (group.length) groups.push(group);
  return groups;
}

function deterministicProseHit(content) {
  for (const group of orderedListGroups(content)) {
    if (group.length < LIMITS.deterministicItems) continue;
    const blob = group.join(' ');
    if (blob.includes('scripts/')) continue;
    if (JUDGMENT_WORD.test(blob)) continue;
    if (DETERMINISTIC_VERB.test(blob) && commandishCount(blob) >= 2) {
      return { count: group.length, sample: group[0].trim().slice(0, 100) };
    }
  }
  return null;
}

function lintSkill(skillDir) {
  const findings = [];
  const add = (sev, rule, msg) => findings.push({ sev, rule, msg });
  const mdPath = join(skillDir, 'SKILL.md');
  const readmePath = join(skillDir, 'README.md');
  const text = readFileSync(mdPath, 'utf8');
  const fmText = frontmatterBlock(text) ?? '';
  const readmeText = existsSync(readmePath) ? readFileSync(readmePath, 'utf8') : null;
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
    add('WARN', 'skill-too-long', `SKILL.md is ${lines.length} lines > ${LIMITS.skillMd}; separate conditional detail into references/ after 50 lines`);

  // W: must use references
  const refLinks = referenceMentions(text);
  if (refLinks.length === 0)
    add('WARN', 'no-references', 'SKILL.md links no references/*.md; lean skills push conditional detail into references');

  // W: SKILL.md is the always-loaded agent map: summarize how it works, then route detail out.
  const bodyWithoutFm = text.replace(/^---[\s\S]*?---\n/, '');
  const bodyLines = stripFencedLines(bodyWithoutFm).split('\n');
  const earlyMap = bodyLines.slice(0, 45).join('\n');
  if (!/\b(flow|workflow|operating model|how it works|route|routing|steps?|loop|mode|core flow)\b/i.test(earlyMap))
    add('WARN', 'skill-map-summary', 'SKILL.md should summarize how the skill works near the top (flow/steps/modes/routing) so agents get the map before conditional detail');

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
  const scriptFiles = listRelativeFiles(join(skillDir, 'scripts'));
  const hookFiles = listRelativeFiles(join(skillDir, 'hooks'));
  const refContents = refFiles.map((f) => ({
    name: f,
    label: `references/${f}`,
    content: readFileSync(join(refsDir, f), 'utf8'),
  }));

  if (refFiles.length >= 3 && new Set(refLinks).size < Math.min(refFiles.length, 3))
    add('WARN', 'capability-routing', `SKILL.md mentions ${new Set(refLinks).size}/${refFiles.length} reference files directly; it should act as a compact capability map to the refs agents may need`);
  if (scriptFiles.length && scriptMentions(text).length === 0)
    add('WARN', 'script-routing', `scripts/ has ${scriptFiles.length} file(s), but SKILL.md never mentions scripts/; route deterministic capabilities to scripts instead of prose`);

  const directRefLinks = new Set(referenceMentions(bodyWithoutFm));
  for (const f of refFiles) {
    if (NAME_EXEMPT.has(f)) continue;
    const resourcePath = `references/${f}`;
    if (!directRefLinks.has(f)) {
      add('WARN', 'reference-map-complete', `${resourcePath} is not listed in SKILL.md; every bundled reference should be concisely routed from the main skill`);
      continue;
    }
    if (!routeLines(bodyLines, resourcePath).some((line) => isConciseRoute(line, resourcePath)))
      add('WARN', 'route-description', `${resourcePath} is listed in SKILL.md but needs a concise same-line purpose/load condition (3-${ROUTE_DESCRIPTION_WORDS} words)`);
  }

  const directScriptLinks = new Set(scriptMentions(bodyWithoutFm));
  for (const s of scriptFiles.filter(isAgentFacingScript)) {
    const resourcePath = `scripts/${s}`;
    if (!directScriptLinks.has(s)) {
      add('WARN', 'script-map-complete', `${resourcePath} is not listed in SKILL.md; every agent-facing script should be concisely routed from the main skill`);
      continue;
    }
    if (!routeLines(bodyLines, resourcePath).some((line) => isConciseRoute(line, resourcePath)))
      add('WARN', 'route-description', `${resourcePath} is listed in SKILL.md but needs a concise same-line purpose/load condition (3-${ROUTE_DESCRIPTION_WORDS} words)`);
  }

  if (hasInstallationResource(refFiles, scriptFiles) && !hasInstallationHeading(bodyWithoutFm))
    add('WARN', 'installation-section', 'skill has install-related references/scripts but no Installation section in SKILL.md');

  if (!readmeText) {
    add('ERROR', 'missing-readme', 'skill folder has no README.md; every skill needs a human-facing overview, features, how-it-works, and install guide');
  } else {
    if (!readmeHasOverview(readmeText))
      add('WARN', 'readme-overview', 'README.md should start with an H1 plus a high-level explanation of what the skill does and when to use it');
    if (!hasHeading(readmeText, /^(features|capabilities|what you get|what it does|good asks|use cases|commands|tools|user value)\b/i))
      add('WARN', 'readme-features', 'README.md should concisely describe the skill features/capabilities users can expect');
    if (!hasHeading(readmeText, /^(how it works|workflow|architecture|operating model|implementation|developer|internals)\b/i))
      add('WARN', 'readme-how-it-works', 'README.md should explain how the skill works for users and developers');
    if (!/\busers?\b/i.test(readmeText) || !/\b(developers?|maintainers?|contributors?|internals?|implementation|scripts?)\b/i.test(readmeText))
      add('WARN', 'readme-audience', 'README.md should help both users and developers/maintainers understand the skill');
    if (!readmeHasInstall(readmeText))
      add('WARN', 'readme-installation', 'README.md should include an Installation section with an `npx octocode skill ...` command');
  }

  const mentionedScripts = new Set([...scriptMentions(text), ...scriptMentions(fmText)]);
  for (const s of mentionedScripts) {
    if (!hasScript(scriptFiles, s))
      add('ERROR', 'missing-script', `SKILL.md/frontmatter mentions scripts/${s} but that bundled script is missing`);
  }

  const hasHooks = /^\s*hooks\s*:/m.test(fmText) || hookFiles.length > 0 || scriptFiles.some((f) => f.split('/').includes('hooks'));
  if (hasHooks) {
    const hookLines = stripFencedLines(bodyWithoutFm).split('\n').filter((ln) => /\bhooks?\b/i.test(ln));
    const explainsHooks = hookLines.some((ln) => /\b(when|before|after|install|inspect|verify|scope|trigger|run|copy|third-party)\b/i.test(ln));
    if (!explainsHooks)
      add('WARN', 'hooks-handling', 'hooks are bundled/configured but SKILL.md does not explain when they run, how to inspect them, or how to verify/handle them');
  }

  if (/^\s*hooks\s*:/m.test(fmText)) {
    // Matches both block-style (`command:` on its own line) and this repo's
    // compact flow-style (`{ type: command, command: "...", timeout: 20 }`
    // all on one line) — a line-anchored regex misses the latter entirely.
    const commandRe = /\bcommand\s*:\s*(?:"([^"]*)"|'([^']*)'|([^,}\s][^,}]*))/g;
    let m;
    while ((m = commandRe.exec(fmText))) {
      const value = (m[1] ?? m[2] ?? m[3] ?? '').trim();
      const lineNo = fmText.slice(0, m.index).split('\n').length;
      const window = fmText.slice(m.index, Math.min(fmText.length, m.index + 200));
      if (!/\b(scripts\/|hooks\/)/.test(value))
        add('WARN', 'hook-script-routing', `frontmatter hook command near line ${lineNo} should route to a bundled scripts/ or hooks/ helper instead of inline logic`);
      if (!/\btimeout\s*:/.test(window))
        add('WARN', 'hook-timeout', `frontmatter hook command near line ${lineNo} has no nearby timeout; hooks should be bounded`);
      if (/\$SKILL_DIR\b|\$\{SKILL_DIR\}/.test(value))
        add('ERROR', 'hook-invalid-skill-dir-var', `frontmatter hook command near line ${lineNo} references $SKILL_DIR / \${SKILL_DIR}, which Claude Code does not substitute — use \${CLAUDE_SKILL_DIR} instead (requires Claude Code v2.1.196+)`);
    }
  }

  for (const { name: f, label, content: rc } of refContents) {
    const rl = rc.split('\n').length;
    if (rl > LIMITS.reference)
      add('WARN', 'reference-too-long', `${label} is ${rl} lines > ${LIMITS.reference}; split it`);
    const h1s = headingLines(rc).filter((h) => h.level === 1);
    if (h1s.length === 0)
      add('WARN', 'reference-focus', `${label} has no H1; each reference should state its single issue/purpose up front`);
    else {
      if (h1s.length > 1)
        add('WARN', 'reference-focus', `${label} has ${h1s.length} H1 headings; split multi-issue references or demote subtopics`);
      const words = h1s[0].text.split(/\s+/).filter(Boolean).length;
      if (words > 8)
        add('WARN', 'reference-focus', `${label} H1 is ${words} words; use a short single-purpose title`);
    }
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

  // W: agent-facing scripts should communicate well and accept deterministic inputs.
  const agentFacingScripts = new Set(scriptMentions(bodyWithoutFm).filter((s) => !isHookScript(s)));
  for (const s of scriptFiles.filter(isCodeScript)) {
    const content = readFileSync(join(skillDir, 'scripts', s), 'utf8');
    const isAgentFacing = agentFacingScripts.has(s);
    if (SCRIPT_INTERACTIVE.test(content))
      add('WARN', 'script-quality', `scripts/${s} appears interactive; agent-facing helpers should accept flags/env/stdin and run unattended`);
    if (isAgentFacing && !SCRIPT_HELP.test(content))
      add('WARN', 'script-quality', `scripts/${s} is referenced from SKILL.md but has no visible --help/usage path`);
    if (isAgentFacing && !SCRIPT_ARGS.test(content))
      add('WARN', 'script-quality', `scripts/${s} is referenced from SKILL.md but does not appear to parse explicit args/env/stdin`);
  }

  // W: bundled references should be reachable from the always-loaded map or another reference.
  const mentionedRefs = new Set(allMdFiles.flatMap(({ content }) => referenceMentions(content)));
  for (const f of refFiles) {
    if (NAME_EXEMPT.has(f)) continue;
    if (!mentionedRefs.has(f))
      add('WARN', 'orphan-reference', `references/${f} is not mentioned by SKILL.md or another reference; route it from the skill map or remove/split it`);
  }

  // E: linked or mentioned references that do not exist. Ignore fenced examples.
  for (const { label, content } of allMdFiles) {
    if (REFERENCE_MENTION_EXEMPT.has(label)) continue;
    for (const r of new Set(referenceMentions(content))) {
      if (!existsSync(join(refsDir, r)))
        add('ERROR', 'missing-reference', `${label} mentions references/${r} but the file is missing`);
    }
  }

  // E: skill declares protocols/schemes in SKILL.md → scripts/scheme.js must exist and expose them all.
  // Only SKILL.md is scanned: it is the author's stated intent. References are secondary documentation and
  // may mention "protocol" descriptively (e.g. install-protocol notes) without the skill owning a protocol.
  // Match only structural declarations: a heading named "protocol/scheme", an explicit "- protocol:" key,
  // or a direct reference to scripts/scheme.js — not casual prose mentions of the word.
  const PROTOCOL_DECL = /(?:^#{1,6}\s+(?:protocols?|schemes?|scheme\.js)\b|^\s*[-*+]\s+protocols?\s*:|scripts\/scheme\.js)/im;
  const schemeScriptPath = join(skillDir, 'scripts', 'scheme.js');
  if (PROTOCOL_DECL.test(stripFencedLines(text)) && !existsSync(schemeScriptPath))
    add('ERROR', 'missing-scheme-script',
      'SKILL.md declares a protocol/scheme section or references scripts/scheme.js but scripts/scheme.js is missing; ' +
      'every skill that uses protocols MUST declare all its schemes and protocols in scripts/scheme.js');

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
    const deterministic = deterministicProseHit(content);
    if (deterministic)
      add('WARN', 'deterministic-prose', `${label}: ${deterministic.count}-step command-like procedure looks scriptable; move deterministic work into scripts/ and keep prose for judgment (starts: "${deterministic.sample}...")`);

    const missingGateParts = gateMissingParts(content);
    if (missingGateParts.length)
      add('WARN', 'gate-structure', `${label}: gate/checkpoint language is present but missing ${missingGateParts.join(', ')} section(s)`);

    for (const block of sectionBlocks(content)) {
      const sectionText = block.lines.join('\n');
      const nonblank = block.lines.filter((line) => line.trim());
      if (/\b(output|report|deliverable|response format|output format)\b/i.test(block.heading) && nonblank.length > 3 && !hasFormatCue(sectionText))
        add('WARN', 'missing-output-format', `${label} line ${block.line}: "${block.heading}" section should include a concrete table/template/fenced format`);
    }

    for (const block of lowDensitySections(content).slice(0, 2)) {
      add('WARN', 'low-density-section', `${label} line ${block.line}: "${block.heading}" is ${block.lines} lines with little structure; compress into routing, tables, gates, or scripts`);
    }

    const xmlTags = xmlTagCount(content);
    if (xmlTags > 8)
      add('WARN', 'xml-overuse', `${label}: ${xmlTags} XML-like tags found; use Markdown by default and XML only for attention-control needs`);

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

    let weakCriticalCount = 0;
    let vagueActionCount = 0;
    let decisionCount = 0;
    let referentialCount = 0;
    stripFencedLines(content).split('\n').forEach((ln, i) => {
      if (isRuleDocLine(ln)) return;
      if (CRITICAL_LINE.test(ln) && WEAK_WORD.test(ln) && ++weakCriticalCount <= 3)
        add('WARN', 'weak-critical-language', `${label} line ${i + 1}: weak word inside critical rule; use MUST/REQUIRED or mark it explicitly optional`);
      if (AMBIGUOUS_ACTION.test(ln) && !/\b(avoid|vague|example|look for)\b/i.test(ln) && ++vagueActionCount <= 3)
        add('WARN', 'ambiguous-action', `${label} line ${i + 1}: vague action phrase; specify the exact action, command, or IF/THEN condition`);
      if (DECISION_IF.test(ln) && !DECISION_THEN.test(ln) && ++decisionCount <= 3)
        add('WARN', 'decision-clarity', `${label} line ${i + 1}: decision rule uses IF without THEN; use IF/THEN for agent branches`);
      if (REFERENTIAL_START.test(ln) && !REFERENTIAL_EXPLICIT.test(ln) && ++referentialCount <= 3)
        add('WARN', 'referential-ambiguity', `${label} line ${i + 1}: starts with an ambiguous referent; name the object/section explicitly`);
    });

    // W-clarity: can this be described more clearly?
    // Three signals: (1) nominalization — verb buried in a noun phrase, (2) double negative,
    // (3) overlong prose line that strains comprehension. Code spans and links are stripped
    // before analysis so inline examples do not inflate word counts or trigger false matches.
    const CLARITY_NOMINALIZE = /\b(?:make|provide|give|perform|conduct|carry\s+out)\s+(?:a|an|the)\s+\w+(?:tion|sion|ment|ance|ence)\b/i;
    const CLARITY_DOUBLE_NEG = /\bnot\b.{1,60}\bnot\b|\bnever\b.{1,60}\bnot\b|\bnot\b.{1,60}\bwithout\b/i;
    const CLARITY_LINE_WORDS = 35;
    let clarityCount = 0;
    stripFencedLines(content).split('\n').forEach((ln, i) => {
      if (clarityCount >= 3 || isRuleDocLine(ln)) return;
      const clean = ln.replace(/`[^`]*`/g, '').replace(/\[[^\]]*\]\([^)]*\)/g, '').trim();
      if (!clean || /^#{1,6}\s/.test(clean) || /^\|/.test(clean)) return;
      if (CLARITY_NOMINALIZE.test(clean)) {
        const m = clean.match(CLARITY_NOMINALIZE);
        add('WARN', 'clarity', `${label} line ${i + 1}: nominalization ("${m[0]}") — can this be stated more directly? rewrite as a direct verb`);
        clarityCount++;
      } else if (CLARITY_DOUBLE_NEG.test(clean)) {
        add('WARN', 'clarity', `${label} line ${i + 1}: double negative — can this be stated more directly? rewrite as a positive statement`);
        clarityCount++;
      } else {
        const wordCount = clean.split(/\s+/).filter(Boolean).length;
        if (wordCount > CLARITY_LINE_WORDS) {
          add('WARN', 'clarity', `${label} line ${i + 1}: ${wordCount}-word prose line — can this be stated more directly? split into shorter, focused statements`);
          clarityCount++;
        }
      }
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
