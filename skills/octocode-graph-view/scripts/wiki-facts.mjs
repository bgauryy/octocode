// The FACTS section of each wiki page, derived from data.json only: size, chunks and purity, every member ranked
// by PageRank with its location and metrics, links to other pages with summed weights and their strongest edge,
// cycles touching the page, file-check problems and the most-called symbols. Every row cites a location.
const fileOf = (id) => String(id).split('#')[0];
// The most-called symbols a page lists; the page says how many more data.json holds.
const SYMBOLS_SHOWN = 20;

export const num = (x) => (typeof x === 'number' && Number.isFinite(x) ? String(Number(x.toPrecision(3))) : '–');
export const cell = (s) => String(s ?? '').replace(/\|/g, '\\|').replace(/\n/g, ' ');
const at = (path, line) => `\`${path}${line ? `:${line}` : ''}\``;
export const pathOfFile = (f) => f?.loc?.path ?? f?.manifest ?? f?.id;
export const citeFile = (f) => at(pathOfFile(f), f?.loc?.line ?? 1);

function bump(sides, key, other, e) {
  const side = sides.get(key) ?? sides.set(key, new Map()).get(key);
  const cur = side.get(other) ?? { weight: 0, edges: 0, top: null };
  cur.weight += e.weight;
  cur.edges++;
  if (!cur.top || e.weight > cur.top.weight) cur.top = e;
  side.set(other, cur);
}

// Summed production weight between communities, both directions (test-only edges are outside the weight rule).
function linksOf(D, comm) {
  const out = new Map(), into = new Map();
  for (const e of D.fileEdges ?? []) {
    if (e.test) continue;
    const a = comm(e.source), b = comm(e.target);
    if (a === null || b === null || a === b) continue;
    bump(out, a, b, e);
    bump(into, b, a, e);
  }
  return { out, into };
}

// The first witness line of each file pair: imports first, then calls.
function edgeLines(D) {
  const lines = new Map();
  for (const e of [...(D.imports ?? []), ...(D.calls ?? [])]) {
    const key = `${fileOf(e.source)}\0${fileOf(e.target)}`;
    const line = e.loc?.lines?.[0] ?? e.lines?.[0] ?? e.line;
    if (!lines.has(key) && Number.isInteger(line)) lines.set(key, line);
  }
  return lines;
}

// Production call sites per called symbol, grouped by the community of the symbol's file.
function calledOf(D, comm) {
  const by = new Map();
  for (const c of D.calls ?? []) {
    if (c.test || c.source === c.target) continue;
    const k = comm(c.target);
    if (k === null) continue;
    const sites = c.count ?? c.lines?.length ?? 1;
    const rows = by.get(k) ?? by.set(k, new Map()).get(k);
    const cur = rows.get(c.target) ?? { id: c.target, sites: 0, outside: 0, callers: 0 };
    cur.sites += sites;
    cur.callers++;
    if (comm(c.source) !== k) cur.outside += sites;
    rows.set(c.target, cur);
  }
  return by;
}

// Everything the pages need from data.json, in one pass over files, edges and calls.
export function wikiContext(D) {
  const byId = new Map(D.files.map((f) => [f.id, f]));
  const comm = (id) => byId.get(fileOf(id))?.metrics?.community ?? null;
  return { D, byId, comm, links: linksOf(D, comm), lines: edgeLines(D), called: calledOf(D, comm), symbols: new Map((D.symbols ?? []).map((s) => [s.id, s])) };
}

const dash = (x) => x ?? '–';
const blank = (x) => x ?? '';

// One member row; `extra` adds the community column (the isolated listing).
function memberRow(ctx, id, i, extra) {
  const f = ctx.byId.get(id) ?? { id };
  const m = f.metrics ?? {};
  const file = f.kind === 'package' ? `${citeFile(f)} (${cell(id)})` : citeFile(f);
  const cells = [i + 1, file, cell(f.language ?? f.kind), num(m.pagerank), num(m.betweenness), num(m.inWeight), num(m.outWeight), dash(m.depth), dash(m.core), blank(m.cycle), f.check?.status ?? 'unchecked', blank(f.check?.lines)];
  if (extra) cells.push(blank(m.community));
  return `| ${cells.join(' | ')} |`;
}

function memberRows(ctx, members, extra) {
  const head = `| # | file | language | PageRank | betweenness | in | out | depth | core | cycle | check | lines |${extra ? ' community |' : ''}`;
  const rule = `|---|---|---|---|---|---|---|---|---|---|---|---|${extra ? '---|' : ''}`;
  return [head, rule, ...members.map((id, i) => memberRow(ctx, id, i, extra))];
}

function linkRows(ctx, side, { slugOf, labelOf }) {
  if (!side?.size) return ['None.'];
  const rows = [...side].toSorted((a, b) => b[1].weight - a[1].weight || a[0] - b[0]).map(([other, l]) => {
    const { source, target, weight } = l.top;
    const line = ctx.lines.get(`${source}\0${target}`);
    const edge = `${at(pathOfFile(ctx.byId.get(source)), line ?? 1)} → ${citeFile(ctx.byId.get(target))} (${num(weight)})`;
    return `| [[${slugOf(other)}]] | ${cell(labelOf(other))} | ${num(l.weight)} | ${l.edges} | ${edge} |`;
  });
  return ['| page | label | weight | file edges | strongest edge |', '|---|---|---|---|---|', ...rows];
}

function cycleRows(ctx, members) {
  const inside = new Set(members);
  const rows = (ctx.D.metrics.cycles ?? []).map((c) => ({ c, here: c.members.filter((id) => inside.has(id)) })).filter((x) => x.here.length);
  if (!rows.length) return ['None.'];
  return rows.map(({ c, here }) => `- cycle ${c.id}: ${c.size} files, weight ${num(c.weight)}, ${c.languages.join(', ')}; chunks ${c.chunks.map((x) => `\`${x}\``).join(', ')}; ${here.length} of them on this page (cycle column above), the most central ${citeFile(ctx.byId.get(c.members[0]))}`);
}

export function problemRows(ctx, members) {
  const rows = members.map((id) => ctx.byId.get(id)).filter((f) => f && f.check?.status !== 'ok');
  if (!rows.length) return ['None: every member matches its file on disk.'];
  return rows.map((f) => `- ${citeFile(f)} ${f.check?.status ?? 'unchecked'}${f.check?.problems?.length ? `: ${f.check.problems.join('; ')}` : ''}`);
}

function symbolRows(ctx, community) {
  const all = [...(ctx.called.get(community)?.values() ?? [])].toSorted((a, b) => b.sites - a.sites || a.id.localeCompare(b.id));
  if (!all.length) return ['None: no production call reaches a symbol on this page.'];
  const rows = all.slice(0, SYMBOLS_SHOWN).map((r) => {
    const s = ctx.symbols.get(r.id);
    const where = s ? at(s.loc?.path ?? s.file, s.line ?? 1) : `${at(fileOf(r.id))} (no declaration row)`;
    return `| ${cell(s?.name ?? r.id.split('#').pop())} | ${cell(s?.kind ?? '')} | ${where} | ${r.sites} | ${r.outside} | ${r.callers} |`;
  });
  const more = all.length > SYMBOLS_SHOWN ? [`${all.length - SYMBOLS_SHOWN} more called symbols are in data.json \`calls\`.`] : [];
  return [`Production call sites (test calls excluded), ${Math.min(all.length, SYMBOLS_SHOWN)} of ${all.length} called symbols:`, '', '| symbol | kind | declared | call sites | from other pages | callers |', '|---|---|---|---|---|---|', ...rows, ...(more.length ? ['', ...more] : [])];
}

function summaryRows(ctx, c) {
  const chunks = Map.groupBy(c.members, (id) => ctx.byId.get(id)?.chunk ?? '-');
  const list = [...chunks].toSorted((a, b) => b[1].length - a[1].length || a[0].localeCompare(b[0])).map(([k, v]) => `\`${k}\` ${v.length}`).join(' · ');
  const checks = Map.groupBy(c.members, (id) => ctx.byId.get(id)?.check?.status ?? 'unchecked');
  const pr = c.members.reduce((a, id) => a + (ctx.byId.get(id)?.metrics?.pagerank ?? 0), 0);
  return [
    `- **Size:** ${c.size} files · weight inside ${num(c.internal)} · across ${num(c.external)} · summed PageRank ${num(pr)}`,
    `- **Chunks:** ${list} · purity ${num(c.purity)} (share in \`${c.chunk}\`)`,
    `- **File check:** ${[...checks].map(([k, v]) => `${v.length} ${k}`).join(' · ')}`,
  ];
}

// The facts of one community page, as markdown lines.
export function communityFacts(ctx, c, names) {
  return [
    ...summaryRows(ctx, c), '',
    '### Members by PageRank', '', ...memberRows(ctx, c.members, false), '',
    '### Links out', '', ...linkRows(ctx, ctx.links.out.get(c.id), names), '',
    '### Links in', '', ...linkRows(ctx, ctx.links.into.get(c.id), names), '',
    '### Cycles', '', ...cycleRows(ctx, c.members), '',
    '### File-check problems', '', ...problemRows(ctx, c.members), '',
    '### Most-called symbols', '', ...symbolRows(ctx, c.id),
  ];
}

// The isolated listing: every file Louvain left alone (no production edge joins it to a community).
export function isolatedFacts(ctx, members) {
  return [
    `- **Size:** ${members.length} files, each its own community of one`, '',
    '### Files by PageRank', '', ...(members.length ? memberRows(ctx, members, true) : ['None.']), '',
    '### File-check problems', '', ...problemRows(ctx, members),
  ];
}

// One line about a page from its facts alone, for index.md when it has no notes yet.
export function factLine(ctx, c, slugOf) {
  const top = ctx.byId.get(c.members[0]);
  const out = [...(ctx.links.out.get(c.id) ?? [])].toSorted((a, b) => b[1].weight - a[1].weight)[0];
  return `top file ${citeFile(top)} (PageRank ${num(top?.metrics?.pagerank)})${out ? `; strongest link out [[${slugOf(out[0])}]] (${num(out[1].weight)})` : ''}`;
}

export const pagerankOf = (ctx, members) => members.reduce((a, id) => a + (ctx.byId.get(id)?.metrics?.pagerank ?? 0), 0);
