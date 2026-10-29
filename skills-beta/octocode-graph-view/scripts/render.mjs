#!/usr/bin/env node
// data.json (+ verify.json) -> report.html from the fixed template, brief.txt, and optional PNG shots.
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chrome, flags } from './lib.mjs';
import { synthesize } from './insights.mjs';
import { resolveModel } from './model.mjs';

const USAGE = `usage: render.mjs --data <data.json> [--verify <verify.json>] [--model <model.json>] [--shot files,functions]
  --model adds the overlay (layers, runtime edges, flows, findings); problems print to stderr.
  Writes report.html and brief.txt beside data.json. --shot also writes shot-<view>.png (1600x1000)
  with headless Chrome (CHROME_PATH overrides discovery).`;

const t0 = Date.now();
const argv = process.argv.slice(2);
const f = flags(argv, { data: '', verify: '', model: '', shot: '' });
if (!f.data) {
  console.error(USAGE);
  process.exit(2);
}
const dataPath = resolve(f.data);
const out = dirname(dataPath);
const D = JSON.parse(readFileSync(dataPath, 'utf8'));
const verifyPath = f.verify ? resolve(f.verify) : join(out, 'verify.json');
if (existsSync(verifyPath)) D.verify = JSON.parse(readFileSync(verifyPath, 'utf8'));

// Overlay: validate against the graph, assign layers, derive upward imports.
const modelPath = f.model ? resolve(f.model) : join(out, 'model.json');
if (existsSync(modelPath)) {
  D.model = resolveModel(D, JSON.parse(readFileSync(modelPath, 'utf8')));
  for (const w of D.model.warnings) console.error(`model: ${w}`);
}
const warnings = D.model?.warnings ?? [];

D.synth = synthesize(D);
const I = D.insights;
const cycNote = (c) => (c.top?.length && c.top.every((x) => x.nodes.every((n) => n.endsWith('.rs'))) ? ', Rust-only (modules in one crate may legally cycle)' : c.runtime == null ? '' : `, ${c.runtime} flagged module-load`);
const brief = [
  D.mode === 'overview'
    ? `OVERVIEW ${D.section}${D.area ? ` (area ${D.area})` : ''}: ${D.snapshot.counts.nodesByKind.file} files split into ${D.files.length} chunks, ${D.imports.length} links between chunks`
    : `SECTION ${D.section}: ${D.snapshot.counts.nodesByKind.file} files analysed (${D.snapshot.counts.nodes} graph nodes, ${D.snapshot.counts.edges} edges). VIEW: the ${D.files.length} busiest connected files with ${D.imports.length} imports, ${D.symbols.length} functions and ${D.calls.length} calls drawn. Paths below are relative to the section.`,
  ...(D.chunks ?? []).slice(0, 14).map((c) => `CHUNK ${c.path} files=${c.files} ${c.built ? `built findings=${Object.entries(c.findings ?? {}).map(([k, v]) => `${k}:${v}`).join(',')}` : 'not built'}`),
  ...D.synth.map((x) => `INSIGHT [${x.level}] ${x.text} [${x.layers.join('+')}]`),
  ...(D.mode === 'overview' ? [] : [
  `SECTION-WIDE health: ${I.health?.filesInCycles} files sit in import cycles; average reach ${I.health?.averageReach} files (files a change can touch)`,
  `HUBS (importers/imports): ${I.hubs.map((h) => `${h.id} ${h.fanIn}/${h.fanOut}`).join('; ')}`,
  `CYCLES (section-wide) ${I.cycles.total}${cycNote(I.cycles)}; largest: ${I.cycles.top.slice(0, 3).map((c) => `${c.size} files (${c.nodes.slice(0, 2).join(', ')}${c.size > 2 ? `, +${c.size - 2} more in data.json` : ''})`).join('; ')}`,
  `FINDINGS (section-wide, detector=count) ${Object.entries(I.findings).map(([k, v]) => `${k}=${v}`).join(' ')}`,
  `TOP ranked findings: ${I.issues.slice(0, 6).map((x) => `${x.detector} ${x.subject}`).join('; ')}`,
  `TOP-LEVEL FOLDERS ${I.mutualGroups.length ? `import each other: ${I.mutualGroups.map((p) => p.join(' <-> ')).join(', ')}` : 'none import each other in view'}`
  ]),
];
if (D.model) {
  const m = D.model;
  brief.push(
    `MODEL layers=${m.layers.map((l) => l.id).join('>') || 'none'} upward_imports=${m.upward.length}${m.upward.length ? ` (${m.upward.slice(0, 3).map((e) => `${e.source}>${e.target}`).join('; ')})` : ''} flows=${m.flows.map((x) => x.id).join(',') || 'none'}`,
    ...m.findings.map((x) => `FINDING [${x.confidence}] ${x.title}`),
    ...(m.warnings.length ? [`MODEL WARNINGS ${m.warnings.length}`] : []),
  );
}
const V = D.verify;
if (V) {
  const L = V.render.layout;
  const o = V.render.ocr;
  brief.push(
    `LAYERS ${Object.entries(V.layers).map(([k, v]) => `${k}=${v === 'off' ? 'off (opt-in)' : v}`).join(' ')}`,
    ...(o ? [`RENDER CHECK reading the Files-view picture with OCR: ${o.readWhole} of ${o.expected} texts (file names plus 2 legend lines) read at once, ${o.recovered.length} more after cropping${o.stillMissing.length ? `; unread: ${o.stillMissing.join(', ')}` : ''}`] : []),
    ...V.leads.map((l) => `LEAD [${l.verdict}] ${l.detector} ${l.file}${l.target ? ` > ${l.target}:${l.line}` : ''} :: ${l.why}${l.signals.votes ? ` (usage ${l.signals.votes.usage}, clasify ${l.signals.votes.clasify})` : l.signals.clasify ? ` (${Object.entries(l.signals.clasify).map(([k, v]) => `${k} ${v}`).join(', ')})` : ''}${l.signals.lsp ? ` lsp: ${[l.signals.lsp.status, l.signals.lsp.symbol, l.signals.lsp.note].filter(Boolean).join(' ')}` : ''}`),
  );
}
brief.push('TERMS importers = files that import it; cycle = files importing each other in a loop; god-file = hub-like (many importers and imports), not proof of mixed duties; importer counts come from the graph and are not independently verified; flagged module-load = closed by module-load imports (a JS/TS notion; Rust has none); lead = a graph claim under test; usage = whether imported names are used after the import; clasify = a model answering named yes/no questions, probabilities are P(yes); owners and unrelated = clasify questions whether a file mixes duties; (usage X, clasify Y) = the call each layer makes on an import: runtime, type, unused or unsure; corroborated = independent layers agree; unused = imported but never used; conflict = layers disagree; mutual pair = two files importing each other; detector names in FINDINGS are graph rules, see `octocode graph query issues`.');
const fullText = brief.join('\n') + '\n';
writeFileSync(join(out, 'brief-full.txt'), fullText);

// brief.txt is the cheap default: scope, top hubs and cycle, the strongest insights, one line per lead.
const rank = { risk: 0, warn: 1, info: 2 };
const clip = (t, n = 230) => (t.length > n ? `${t.slice(0, n)} ...` : t);
const dirOf = (id) => id.split('/').slice(0, -1).join('/');
const short = [brief[0]];
if (D.mode === 'overview') short.push(...brief.filter((l) => l.startsWith('CHUNK ')).slice(0, 10));
else {
  short.push(`HUBS (importers/imports): ${I.hubs.slice(0, 3).map((h) => `${h.id} ${h.fanIn}/${h.fanOut}`).join('; ')}`);
  if (I.cycles.total) short.push(`CYCLES (section-wide) ${I.cycles.total}${cycNote(I.cycles)}${I.cycles.top[0] ? `; largest ${I.cycles.top[0].size} files in ${dirOf(I.cycles.top[0].nodes[0] ?? '')}` : ''}`);
}
if (V?.history?.top?.length) short.push(`MOST CHANGED (section-wide, commits): ${V.history.top.slice(0, 3).map((t) => `${t.id} ${t.commits}`).join('; ')}`);
short.push(...[...D.synth].sort((a, b) => rank[a.level] - rank[b.level]).slice(0, 5).map((x) => `INSIGHT [${x.level}] ${clip(x.text)}`));
if (V) short.push(...V.leads.map((l) => `LEAD [${l.verdict}] ${l.detector} ${l.file}${l.target ? ` > ${l.target}:${l.line}` : ''}${l.signals.votes?.usage ? ` (${l.signals.votes.usage} use)` : ''}`), `LAYERS ${Object.entries(V.layers).map(([k, v]) => `${k}=${v === 'off' ? 'off (opt-in)' : v}`).join(' ')}`);
short.push('importers = files that import it. Verdicts: corroborated = independent layers agree; unused = imported, never used; not-supported = the graph claim failed a check. Why-text, terms, findings and layer details: brief-full.txt');
const briefText = short.join('\n') + '\n';
writeFileSync(join(out, 'brief.txt'), briefText);

const est = (bytes) => Math.round(bytes / 4);
const dataBytes = readFileSync(dataPath).length;
const srcTokens = est(D.tokens.sourceBytes);
D.tokens.rows = [
  { name: 'Raw source of the section', tokens: srcTokens, note: D.tokens.sourceBytes ? `${D.tokens.sourceFiles} files, read in full` : `${D.tokens.sourceFiles} files, not read (overview)` },
  { name: 'data.json', tokens: est(dataBytes), note: 'full graph data; machine input to the page' },
  { name: 'One screenshot', tokens: Math.round((1600 * 1000) / 750), note: 'image look at 1600x1000' },
  { name: 'brief-full.txt', tokens: est(fullText.length), note: 'everything: terms, findings, why-text, layer details' },
  { name: 'brief.txt', tokens: est(briefText.length), note: 'what the agent reads first: scope, hubs, strongest insights, one line per lead' },
];
D.tokens.ratio = Math.round(srcTokens / est(briefText.length));
D.tokens.clasifyInput = V ? V.usage?.input_tokens ?? 0 : 0;

const tpl = readFileSync(join(dirname(fileURLToPath(import.meta.url)), '../assets/graph-view.html'), 'utf8');
const json = JSON.stringify(D).replace(/</g, '\\u003c');
const htmlPath = join(out, 'report.html');
writeFileSync(htmlPath, tpl.replace('/*__DATA__*/', () => json));

const shots = [];
for (const view of f.shot ? f.shot.split(',') : []) {
  const png = join(out, `shot-${view}.png`);
  const base = ['--headless=new', '--disable-gpu', '--hide-scrollbars', '--window-size=1600,1000', '--virtual-time-budget=8000'];
  execFileSync(chrome(), [...base, `--screenshot=${png}`, `file://${htmlPath}#shot=${view}`], { stdio: 'ignore' });
  shots.push(png);
  // The page measures its own label boxes; --dump-dom hands them back as JSON.
  const dom = execFileSync(chrome(), [...base, '--dump-dom', `file://${htmlPath}#shot=${view}`], { encoding: 'utf8', maxBuffer: 64 << 20, stdio: ['ignore', 'pipe', 'ignore'] });
  const m = dom.match(/<pre id="measure"[^>]*>([\s\S]*?)<\/pre>/);
  if (!m) throw new Error(`no layout measurement for view ${view}`);
  const text = m[1].replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&');
  writeFileSync(join(out, `measure-${view}.json`), text);
}
console.log(JSON.stringify({ ms: Date.now() - t0, html: htmlPath, warnings: warnings.length, brief: join(out, 'brief.txt'), briefTokens: est(briefText.length), fullTokens: est(fullText.length), sourceTokens: srcTokens, ratio: D.tokens.ratio, shots }));
