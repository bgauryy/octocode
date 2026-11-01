// Render and catalog: build the HTML report, the JSON manifest, and default output paths.
import fs from 'node:fs';
import path from 'node:path';
import { mdKit } from './markdown.mjs';
import { pageHtml } from './page.mjs';
import { KINDS, PRIMARY, SCRIPT_SKILL_DIR, collect, skillFor, toPosix } from './workspace.mjs';
import { checkReport, contractFor, outline, supportingFile } from './checks.mjs';

const IMAGE_TYPES = {'.png':'image/png','.jpg':'image/jpeg','.jpeg':'image/jpeg','.gif':'image/gif','.webp':'image/webp','.svg':'image/svg+xml','.avif':'image/avif'};

// A local image as a data: URL, shared through `embedded` when several documents use it.
function embedImage(f, raw, workspace, embedded) {
  if (!raw || raw.startsWith('#') || /^[a-z][\w+.-]*:/i.test(raw) || raw.startsWith('//')) throw new Error('Use a local image path or HTTPS URL');
  const full = fs.realpathSync(path.resolve(path.dirname(f.source), decodeURIComponent(raw.split(/[?#]/)[0])));
  const relative = path.relative(fs.realpathSync(workspace), full);
  if (relative === '..' || relative.startsWith('..' + path.sep) || path.isAbsolute(relative)) throw new Error('Image must be inside the workspace');
  const mime = IMAGE_TYPES[path.extname(full).toLowerCase()];
  if (!mime) throw new Error('Unsupported image type');
  if (!embedded.has(full)) embedded.set(full, 'data:' + mime + ';base64,' + fs.readFileSync(full).toString('base64'));
  return embedded.get(full);
}

// Sets each file's `images` (source → data: URL); unusable images become warnings.
function embedImages(files, kit, workspace, warnings) {
  const embedded = new Map();
  for (const f of files) {
    f.images = {};
    for (const match of kit.parse(f.body).matchAll(/<img\b[^>]* src="([^"]*)"/g)) {
      const raw = match[1].replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&');
      if (/^https?:\/\//i.test(raw)) continue;
      try { f.images[raw] = embedImage(f, raw, workspace, embedded); } catch (error) { warnings.push(`${f.path}: image ${raw}: ${error.message}`); }
    }
  }
}

const widgetWarnings = (kit, f) => kit.widgetBlocks(f.body).flatMap((w,i) => { try { kit.widget(w.kind,w.source); return []; } catch(e) { return [`${f.path}: ${w.kind} widget ${i+1}: ${e.message}`]; } });

function reportTitle(options, payload, rels) {
  const { files, rootName } = payload;
  if (options.title) return options.title;
  if (files.length > 1 && !payload.primary) return rootName;
  const firstH1 = (files[0].body.match(/^ {0,3}#\s+(.+?)\s*#*\s*$/m) || [])[1];
  return files[0].meta.title || (firstH1 || rels[0]).replace(/[`*_]/g, '');
}

export function buildReport(target, options = {}) {
  const { abs, isDir, workspace, root, rels, files, underOctocode } = collect(target, options);
  const top = rels.find((r) => !r.includes('/') && PRIMARY.includes(r.toLowerCase()));
  const rootName = underOctocode ? `${path.basename(workspace)} – Octocode output` : path.basename(root);
  const payload = {
    title: options.title || '',
    rootName,
    workspace: toPosix(workspace),
    kind: files.length === 1 ? files[0].kind : '',
    primary: isDir && top ? top : null,
    files,
  };
  const title = reportTitle(options, payload, rels);
  const kit = mdKit();
  const warnings = [...new Set(files.flatMap((f) => [
    ...checkReport(path.join(root, f.path), f.meta, f.body, workspace),
    ...widgetWarnings(kit, f),
  ]))];
  embedImages(files, kit, workspace, warnings);
  return { html: pageHtml(payload, title), files: rels, workspace, isDir, abs, warnings };
}

const HEADER_KEYS = ['status', 'confidence', 'question', 'next'];

export function linksIn(body) {
  // Extract the same links the reader sees, not a second interpretation of Markdown.
  const html = mdKit().parse(body);
  const entities = { '&amp;': '&', '&quot;': '"', '&#39;': "'", '&lt;': '<', '&gt;': '>' };
  const decode = value => value.replace(/&(?:amp|quot|lt|gt|#39);/g, e => entities[e]);
  const links = [...html.matchAll(/<a\b[^>]*\bhref="(https?:[^" ]*)"/g)].map(m => decode(m[1]));
  for (const block of mdKit().widgetBlocks(body).filter(w=>w.kind==='graph')) {
    try { links.push(...mdKit().widget('graph',block.source).nodes.map(n=>n.url).filter(Boolean)); } catch { /* Invalid data has no verified graph links. */ }
  }
  return [...new Set(links)];
}

// Shared reading of each skill's status words: done, attention, failed, open, or inactive.
const STATUS_TONE = { green: 'done', amber: 'attention', red: 'failed', blue: 'open', gray: 'inactive' };
let toneKit;
const kitTone = (status) => (toneKit ||= mdKit()).toneOf(status);

function headerOf(meta) {
  const header = {};
  for (const k of HEADER_KEYS) if (meta[k] !== undefined && meta[k] !== '') header[k] = meta[k];
  return header;
}

// A status outside the skill's contract gets no tone, so an invalid word never looks like success.
function statusTone(full, status) {
  const statuses = (skillFor(full) && contractFor(skillFor(full))?.statuses) || [];
  const validStatus = !statuses.length || statuses.some((w) => w.toLowerCase() === String(status).toLowerCase());
  return validStatus ? STATUS_TONE[kitTone(status)] || null : null;
}

function manifestEntry(f, root, workspace) {
  const full = path.join(root, f.path);
  const { title, lead, headings } = outline(f.body);
  const header = headerOf(f.meta);
  const other = Object.fromEntries(Object.entries(f.meta).filter(([k]) => !HEADER_KEYS.includes(k) && k !== 'title' && k !== 'date'));
  const html = pageFor(full, workspace);
  const rel = toPosix(path.relative(workspace, full));
  return {
    path: rel.startsWith('..') ? toPosix(full) : rel,
    skill: skillFor(full) || null,
    kind: f.kind || null,
    title: f.meta.title || title || path.basename(f.path),
    date: f.meta.date || (f.path.match(/(\d{4}-\d{2}-\d{2})/) || [])[1] || null,
    ...header,
    ...(header.status ? { statusTone: statusTone(full, header.status) } : {}),
    ...(Object.keys(other).length ? { meta: other } : {}),
    lead: lead || null,
    headings,
    links: linksIn(f.body),
    codeRefs: Object.entries(f.refs).map(([ref, r]) => ({ ref, path: r.rel, ...(r.line ? { line: r.line } : {}) })),
    html: html ? toPosix(path.relative(workspace, html)) : null,
  };
}

export function buildManifest(target, options = {}) {
  const { workspace, root, files } = collect(target, options);
  const documents = files.filter((f) => !supportingFile(path.join(root, f.path), f.meta, workspace)).map((f) => manifestEntry(f, root, workspace));
  documents.sort((a, b) => (b.date || '').localeCompare(a.date || '') || a.path.localeCompare(b.path));
  return { workspace: toPosix(workspace), root: toPosix(path.relative(workspace, root)) || '.', count: documents.length, documents };
}

// The page that shows a report: its own <name>.html, else the nearest folder
// render (index.html) above it, up to the workspace's .octocode/ folder.
function pageFor(full, workspace) {
  const own = defaultOut(full, false, workspace);
  if (fs.existsSync(own)) return own;
  const octo = path.join(workspace, '.octocode');
  if (!full.startsWith(octo + path.sep)) return null;
  for (let dir = path.dirname(full); dir.startsWith(octo); dir = path.dirname(dir)) {
    const index = path.join(dir, 'index.html');
    if (fs.existsSync(index)) return index;
    if (dir === octo) break;
  }
  return null;
}

export function defaultOut(abs, isDir, workspace) {
  const octo = path.join(workspace, '.octocode');
  const inside = abs === octo || abs.startsWith(octo + path.sep);
  if (inside) return isDir ? path.join(abs, 'index.html') : abs.replace(/\.(md|markdown)$/i, '') + '.html';
  const name = path.basename(abs).replace(/\.(md|markdown)$/i, '') || 'report';
  // Every skill output stays in that skill's own folder, so a page for an outside source goes there too.
  const own = path.basename(SCRIPT_SKILL_DIR);
  return path.join(octo, KINDS[own] ? own : 'octocode-skills', 'view', name, 'index.html');
}

export function manifestPage(target, options = {}) {
  const catalog = buildManifest(target, {workspace:options.workspace});
  const query = String(options.query || '').toLowerCase();
  const matches = catalog.documents.filter(d=>!query || [d.path,d.title,d.question].some(v=>String(v || '').toLowerCase().includes(query)));
  if (options.all) return {...catalog,count:matches.length,documents:matches};
  const offset = options.offset ?? 0, limit = options.limit ?? 20;
  const documents = matches.slice(offset, offset+limit).map((d)=>({path:d.path,title:d.title,status:d.status,question:d.question}));
  const next = offset+documents.length < matches.length ? [target,'--manifest',...(query?['--query',options.query]:[]),'--limit',String(limit),'--offset',String(offset+documents.length),...(options.workspace?['--workspace',options.workspace]:[])] : null;
  return {root:catalog.root,total:matches.length,offset,count:documents.length,documents,next};
}
