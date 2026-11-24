// Report contract checks for a saved report under .octocode/<skill>/: the rules come from that
// skill's output.md (installed beside this script's skill folder); prose is not parsed as a schema.
import fs from 'node:fs';
import path from 'node:path';
import { KINDS, PRIMARY, SCRIPT_SKILL_DIR, toPosix } from './workspace.mjs';

const contractCache = new Map();
export function contractFor(skill) {
  if (contractCache.has(skill)) return contractCache.get(skill);
  const candidates = [path.join(path.dirname(SCRIPT_SKILL_DIR), skill, 'output.md')];
  if (path.basename(SCRIPT_SKILL_DIR) === skill) candidates.unshift(path.join(SCRIPT_SKILL_DIR, 'output.md'));
  const file = candidates.find((c) => fs.existsSync(c));
  let contract = null;
  if (file) {
    const text = fs.readFileSync(file, 'utf8');
    const encoded = text.match(/<!-- report-contract ([\s\S]*?) -->/);
    if (!encoded) throw new Error(`Missing explicit report contract in ${file}`);
    contract = JSON.parse(encoded[1]);
    if (!Array.isArray(contract.statuses) || !contract.statuses.length || !contract.statuses.every(x=>typeof x === 'string' && x) || !Array.isArray(contract.required) || !contract.required.every(x=>typeof x === 'string' && x) || !Array.isArray(contract.anyOf) || !contract.anyOf.every(g=>Array.isArray(g) && g.length && g.every(x=>typeof x === 'string' && x))) throw new Error(`Invalid report contract in ${file}`);
  }
  contractCache.set(skill, contract);
  return contract;
}

// The report skill whose scripts these are, or '' when installed elsewhere (skills/common).
export const SCRIPT_SKILL = Object.hasOwn(KINDS, path.basename(SCRIPT_SKILL_DIR)) ? path.basename(SCRIPT_SKILL_DIR) : '';

// The skill whose contract a report answers to: the .octocode/<skill>/ folder it is saved in, else `owner` for a
// report saved outside <workspace>/.octocode/. '' when neither names a report skill.
export function reportSkill(full, workspace, owner = '') {
  const rel = toPosix(path.relative(path.join(workspace, '.octocode'), full)).split('/');
  if (rel[0] === '..') return owner;
  return rel.length >= 2 && Object.hasOwn(KINDS, rel[0]) ? rel[0] : '';
}

export const localDate = (d) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;

// A file inside a report folder that is not the folder's entry point (README.md, RFC.md, index.md) and carries
// no report header is a supporting file: run logs, cases, transcripts. It is neither checked nor cataloged.
export function supportingFile(full, meta, workspace) {
  const rel = toPosix(path.relative(path.join(workspace, '.octocode'), full)).split('/');
  // Only the folder's top-level README.md, RFC.md, or index.md is its entry point; nested ones are supporting files.
  if (rel.length < 3 || rel[0] === '..' || !KINDS[rel[0]] || (rel.length === 3 && PRIMARY.includes(rel[2].toLowerCase()))) return null;
  const top = path.join(workspace, '.octocode', rel[0], rel[1]);
  const hasPrimary = fs.existsSync(top) && fs.readdirSync(top).some((x) => PRIMARY.includes(x.toLowerCase()));
  if (hasPrimary) return { folder: rel[1], hasPrimary };
  return meta.status || meta.question ? null : { folder: rel[1], hasPrimary };
}

// A report saved below a folder must sit beside that folder's entry point.
function layoutWarnings(full, rel) {
  if (rel.length > 2 && !PRIMARY.includes(rel[rel.length - 1].toLowerCase()) && !fs.readdirSync(path.dirname(full)).some((f) => PRIMARY.includes(f.toLowerCase())))
    return [`a single report is one file, .octocode/${rel[0]}/<YYYY-MM-DD>-<slug>.md; a folder needs README.md (or RFC.md) as its entry point`];
  return [];
}

function dateWarnings(full, rel, meta, workspace) {
  const dated = (rel[1].match(/^(\d{4}-\d{2}-\d{2})-/) || [])[1];
  const stat = fs.statSync(full);
  // Editing a report does not change its original date or stable links. Some
  // filesystems provide no creation timestamp; there it cannot be checked.
  const written = meta.date || (stat.birthtimeMs > 0 ? localDate(stat.birthtime) : null);
  if (!dated) return [`name the report <YYYY-MM-DD>-<slug> using today's date (date +%F)`];
  if (!written || dated === written) return [];
  const from = path.join(workspace, '.octocode', rel[0], rel[1]);
  const to = path.join(workspace, '.octocode', rel[0], written + rel[1].slice(dated.length));
  return [`the name says ${dated} but the file was first written ${written}; dates come from \`date +%F\`, not memory. Fix: mv "${from}" "${to}" and render again`];
}

function headerWarnings(meta, workspace) {
  const warnings = [];
  if (!meta.status) warnings.push('the header has no status');
  if (!meta.question) warnings.push('the header has no question');
  if (meta.next && /^(none|n\/a|-)\.?$/i.test(String(meta.next).trim())) warnings.push('`next` says none; drop the key when nothing is pending');
  for (const key of ['captures', 'session']) {
    if (meta[key] && !fs.existsSync(path.resolve(workspace, String(meta[key])))) warnings.push(`\`${key}: ${meta[key]}\` does not exist; point it at the folder that holds the files`);
  }
  return warnings;
}

// Required sections keep the contract's order: the first ## heading of each present one must follow that order.
function orderWarnings(required, h2) {
  const present = required.filter((h) => h2.includes(h.toLowerCase()));
  const found = present.toSorted((a, b) => h2.indexOf(a.toLowerCase()) - h2.indexOf(b.toLowerCase()));
  if (found.every((h, i) => h === present[i])) return [];
  const list = (hs) => hs.map((h) => `## ${h}`).join(', ');
  return [`required sections are out of order: ${list(found)}; the contract order is ${list(present)}`];
}

// `check`: a skill without a reachable contract is a violation, since --check must not pass what it never checked.
function contractWarnings(skill, meta, body, check = false) {
  const contract = contractFor(skill);
  if (!contract) return check ? [`no report contract for ${skill}: its output.md is not installed beside this script; run ${skill}/scripts/render-report.mjs --check`] : [];
  const warnings = [];
  if (meta.status && contract.statuses.length && !contract.statuses.some((w) => w.toLowerCase() === String(meta.status).toLowerCase()))
    warnings.push(`status "${meta.status}" is not one of ${contract.statuses.join(', ')}`);
  const h2 = outline(body).headings.filter(h=>h.level===2).map(h=>h.text.toLowerCase());
  const heads = new Set(h2);
  for (const h of contract.required) if (!heads.has(h.toLowerCase())) warnings.push(`missing the required section ## ${h}`);
  for (const g of contract.anyOf) if (!g.some((h) => heads.has(h.toLowerCase()))) warnings.push(`missing a required section: ${g.map((h) => `## ${h}`).join(' or ')}`);
  return [...warnings, ...orderWarnings(contract.required, h2)];
}

// `check` (render-report --check) treats every file as a report: one no contract can check is a violation, and a
// report saved outside <workspace>/.octocode/ is checked against `owner`, the skill whose script runs.
export function checkReport(full, meta, body, workspace, { check = false, owner = '' } = {}) {
  const support = supportingFile(full, meta, workspace);
  if (support) return support.hasPrimary ? [] : [`${support.folder}/: a report folder needs README.md (or RFC.md) as its entry point`];
  const rel = toPosix(path.relative(path.join(workspace, '.octocode'), full)).split('/');
  // A skill report written into scratch space (.octocode/<skill>/tmp/, or a legacy .octocode/tmp/) never reaches
  // the catalog. tmp/showcase holds the rendered sample set and is exempt.
  const scratch = rel[0] === 'tmp' ? rel[1] !== 'showcase' : rel[1] === 'tmp';
  if (scratch && meta.status && meta.question)
    return [`${rel.join('/')}: this has a report header but sits in a tmp/ folder, which the catalog skips; save it as .octocode/<skill>/<YYYY-MM-DD>-<slug>.md`];
  const skill = reportSkill(full, workspace, check ? owner : '');
  const fromWorkspace = toPosix(path.relative(workspace, full));
  const shown = fromWorkspace.startsWith('../') ? toPosix(full) : fromWorkspace;
  if (!skill) return check ? [`${shown}: no report skill owns this file; save it as .octocode/<skill>/<YYYY-MM-DD>-<slug>.md, or check it with the owning skill's scripts/render-report.mjs`] : [];
  if (rel[0] === '..') return [...headerWarnings(meta, workspace), ...contractWarnings(skill, meta, body, check)].map((w) => `${shown}: ${w}`);
  const name = rel.length === 2 ? rel[1] : `${rel[1]}/${rel.slice(2).join('/')}`;
  const warnings = [...layoutWarnings(full, rel), ...dateWarnings(full, rel, meta, workspace), ...headerWarnings(meta, workspace), ...contractWarnings(skill, meta, body, check)];
  return warnings.map((w) => `${name}: ${w}`);
}

const strip = (t) => t.replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/[`*]/g, '').replace(/(^|[^\w])_+|_+(?![\w])/g, '$1').trim();

// The ATX or setext heading at line i, if any.
function headingAt(lines, i) {
  const line = lines[i];
  const h = line.match(/^ {0,3}(#{1,6})\s+(.+?)\s*#*\s*$/);
  if (h) return { level: h[1].length, text: strip(h[2]), setext: false };
  const setext = line.trim() && /^ {0,3}(=+|-+)\s*$/.test(lines[i + 1] || '') && !/^\s*[-*+|>]/.test(line);
  return setext ? { level: lines[i + 1].trim()[0] === '=' ? 1 : 2, text: strip(line), setext: true } : null;
}

export function outline(body) {
  const lines = body.replace(/^ {0,3}(`{3,}|~{3,})[\s\S]*?^ {0,3}\1[ \t]*$/gm, '').split('\n');
  const headings = [];
  let title = '', lead = '', para = [], afterTitle = false;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const heading = headingAt(lines, i);
    if (heading) {
      const { level, text } = heading;
      if (level === 1 && !title) { title = text; afterTitle = true; } else headings.push({ level, text });
      if (heading.setext) i++;
      if (lead === '' && para.length) lead = strip(para.join(' '));
      para = [];
      continue;
    }
    if (lead !== '' || !afterTitle) continue;
    // A blank line, list, quote, table, or HTML line ends the lead paragraph.
    if (!line.trim() || /^\s*([-*+]\s|\d+[.)]\s|>|\||<)/.test(line)) { if (para.length) lead = strip(para.join(' ')); continue; }
    para.push(line.trim());
  }
  if (lead === '' && para.length) lead = strip(para.join(' '));
  return { title, lead, headings };
}
