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

export function checkReport(full, meta, body, workspace) {
  const support = supportingFile(full, meta, workspace);
  if (support) return support.hasPrimary ? [] : [`${support.folder}/: a report folder needs README.md (or RFC.md) as its entry point`];
  const rel = toPosix(path.relative(path.join(workspace, '.octocode'), full)).split('/');
  // A skill report written into scratch space (.octocode/<skill>/tmp/, or a legacy .octocode/tmp/) never reaches
  // the catalog. tmp/showcase holds the rendered sample set and is exempt.
  const scratch = rel[0] === 'tmp' ? rel[1] !== 'showcase' : rel[1] === 'tmp';
  if (scratch && meta.status && meta.question)
    return [`${rel.join('/')}: this has a report header but sits in a tmp/ folder, which the catalog skips; save it as .octocode/<skill>/<YYYY-MM-DD>-<slug>.md`];
  if (rel.length < 2 || rel[0] === '..' || !KINDS[rel[0]]) return [];
  const skill = rel[0];
  const warnings = [];
  const name = rel.length === 2 ? rel[1] : rel[1] + '/' + rel.slice(2).join('/');
  if (rel.length > 2 && !PRIMARY.includes(rel[rel.length - 1].toLowerCase()) && !fs.readdirSync(path.dirname(full)).some((f) => PRIMARY.includes(f.toLowerCase())))
    warnings.push(`a single report is one file, .octocode/${skill}/<YYYY-MM-DD>-<slug>.md; a folder needs README.md (or RFC.md) as its entry point`);
  const dated = (rel[1].match(/^(\d{4}-\d{2}-\d{2})-/) || [])[1];
  const stat = fs.statSync(full);
  // Editing a report does not change its original date or stable links. Some
  // filesystems provide no creation timestamp; there it cannot be checked.
  const written = meta.date || (stat.birthtimeMs > 0 ? localDate(stat.birthtime) : null);
  if (!dated) warnings.push(`name the report <YYYY-MM-DD>-<slug> using today's date (date +%F)`);
  else if (written && dated !== written) {
    const from = path.join(workspace, '.octocode', skill, rel[1]);
    const to = path.join(workspace, '.octocode', skill, written + rel[1].slice(dated.length));
    warnings.push(`the name says ${dated} but the file was first written ${written}; dates come from \`date +%F\`, not memory. Fix: mv "${from}" "${to}" and render again`);
  }
  if (!meta.status) warnings.push('the header has no status');
  if (!meta.question) warnings.push('the header has no question');
  if (meta.next && /^(none|n\/a|-)\.?$/i.test(String(meta.next).trim())) warnings.push('`next` says none; drop the key when nothing is pending');
  for (const key of ['captures', 'session']) {
    if (meta[key] && !fs.existsSync(path.resolve(workspace, String(meta[key])))) warnings.push(`\`${key}: ${meta[key]}\` does not exist; point it at the folder that holds the files`);
  }
  const contract = contractFor(skill);
  if (contract) {
    if (meta.status && contract.statuses.length && !contract.statuses.some((w) => w.toLowerCase() === String(meta.status).toLowerCase()))
      warnings.push(`status "${meta.status}" is not one of ${contract.statuses.join(', ')}`);
    const heads = new Set(outline(body).headings.filter(h=>h.level===2).map(h=>h.text.toLowerCase()));
    for (const h of contract.required) if (!heads.has(h.toLowerCase())) warnings.push(`missing the required section ## ${h}`);
    for (const g of contract.anyOf) if (!g.some((h) => heads.has(h.toLowerCase()))) warnings.push(`missing a required section: ${g.map((h) => `## ${h}`).join(' or ')}`);
  }
  return warnings.map((w) => `${name}: ${w}`);
}

export function outline(body) {
  const lines = body.replace(/^ {0,3}(`{3,}|~{3,})[\s\S]*?^ {0,3}\1[ \t]*$/gm, '').split('\n');
  const headings = [];
  let title = '', lead = '', para = [], afterTitle = false;
  const strip = (t) => t.replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/[`*]/g, '').replace(/(^|[^\w])_+|_+(?![\w])/g, '$1').trim();
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const h = line.match(/^ {0,3}(#{1,6})\s+(.+?)\s*#*\s*$/);
    const setext = !h && line.trim() && /^ {0,3}(=+|-+)\s*$/.test(lines[i + 1] || '') && !/^\s*[-*+|>]/.test(line);
    if (h || setext) {
      const level = h ? h[1].length : (lines[i + 1].trim()[0] === '=' ? 1 : 2);
      const text = strip(h ? h[2] : line);
      if (level === 1 && !title) { title = text; afterTitle = true; } else headings.push({ level, text });
      if (setext) i++;
      if (lead === '' && para.length) lead = strip(para.join(' '));
      para = [];
      continue;
    }
    if (lead !== '' || !afterTitle) continue;
    if (!line.trim()) { if (para.length) lead = strip(para.join(' ')); continue; }
    if (/^\s*([-*+]\s|\d+[.)]\s|>|\||<)/.test(line)) { if (para.length) lead = strip(para.join(' ')); continue; }
    para.push(line.trim());
  }
  if (lead === '' && para.length) lead = strip(para.join(' '));
  return { title, lead, headings };
}
