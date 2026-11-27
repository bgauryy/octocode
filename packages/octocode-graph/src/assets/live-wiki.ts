// Wiki drawer: the code-wiki page (src/runtime/wiki.ts) of the clicked community or the selected file's community.
// It asks the server with `wiki` ops ({slug} or {file}); the answer is a `wikiPage` event. Until it arrives the drawer
// shows the facts the metrics stream already holds. Markdown is rendered from escaped text only: headings, lists,
// tables (long ones collapse, every row stays in the page), code, [[slug]] links and path:line links to graph nodes.
import type { FileMetrics } from '../runtime/types.ts';
import { closest, find } from './live-dom.ts';
import type {
  CommunityDetail,
  LiveEvent,
  LiveState,
  MetricsPayload,
  ViewDeps,
  WikiPagePayload,
  WikiPageRow,
  WikiPayload,
} from './live-state.ts';

/** What the drawer shows: a page by slug, a community's page, or the page that holds a file. */
interface WikiTarget {
  slug?: string;
  community?: number;
  file?: string;
}
/** A `wiki` op in flight; `retried` once it fell back from a stale slug to the file. */
interface WikiRequest {
  slug?: string;
  file?: string;
  retried?: boolean;
}
/** The drawer header's facts about a page. */
interface WikiInfo {
  slug?: string | null;
  label: string;
  meta?: string;
  size?: number;
  community?: number | null;
  state: string | null | undefined;
}
interface WikiState {
  open: boolean;
  target: WikiTarget | null;
  slug: string | null;
  /** The page shown is the metrics-stream placeholder, not a wiki page. */
  fallback?: boolean;
  pages: Map<string, WikiPageRow>;
  cache: Map<string, string>;
  learned: Map<number, string>;
  history: string[];
  pending: WikiRequest | null;
  members: string[];
}
interface Wiki extends ViewDeps {
  S: LiveState;
  W: WikiState;
  D: ReturnType<typeof buildDom>;
}

const HTML: Record<string, string> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&#39;',
};
const MENTION = /[\w@+./-]*[\w@+-]\.[A-Za-z][\w]*(?::\d+(?:-\d+)?){0,2}/g;
const INLINE = /(`[^`\n]+`|\[\[[^\]\n]+\]\]|\*\*[^*\n]+\*\*)/g;
const STATE: Record<string, string> = {
  none: 'no notes yet',
  fresh: 'notes fresh',
  stale: 'notes stale',
  orphaned: 'orphaned',
};
// Table rows shown before "Show all": the rest are in the page, revealed on demand.
const ROWS = 25;
const ISO_BUCKET = 'c:iso';

const esc = (s: string | number | null | undefined) =>
  String(s ?? '').replace(/[&<>"']/g, c => HTML[c]);
const isTyping = (el: EventTarget | null) =>
  el instanceof Element &&
  Boolean(
    el.closest(
      'input,textarea,select,[contenteditable=""],[contenteditable="true"]'
    )
  );
const fmt = (x: number | null | undefined) =>
  typeof x !== 'number'
    ? '–'
    : Number.isInteger(x)
      ? String(x)
      : String(Number(x.toPrecision(3)));

function nodeFor(S: LiveState, token: string) {
  let path = token.replace(/(?::\d+(?:-\d+)?){1,2}$/, '').replace(/^\.\//, '');
  if (S.root && path.startsWith(`${S.root}/`))
    path = path.slice(S.root.length + 1);
  if (S.nodes.has(path)) return path;
  const pkg = [...S.nodes.values()].find(
    n => n.kind === 'package' && n.manifest === path
  );
  return pkg?.id ?? null;
}

function linkify(S: LiveState, raw: string) {
  let html = '',
    at = 0;
  for (const m of raw.matchAll(MENTION)) {
    const id = nodeFor(S, m[0]);
    if (!id) continue;
    html += `${esc(raw.slice(at, m.index))}<a href="#" class="wiki-node" data-node="${esc(id)}" title="show in graph">${esc(m[0])}</a>`;
    at = m.index + m[0].length;
  }
  return html + esc(raw.slice(at));
}

function inline(K: Wiki, raw: string) {
  return raw
    .split(INLINE)
    .map(p => {
      if (p.startsWith('`') && p.endsWith('`') && p.length > 2)
        return `<code>${linkify(K.S, p.slice(1, -1))}</code>`;
      if (p.startsWith('[[') && p.endsWith(']]')) {
        const slug = p.slice(2, -2).trim();
        return `<a href="#" class="wiki-link" data-wiki="${esc(slug)}" title="${esc(K.W.pages.get(slug)?.label ?? slug)}">${esc(slug)}</a>`;
      }
      if (p.startsWith('**') && p.endsWith('**') && p.length > 4)
        return `<b>${linkify(K.S, p.slice(2, -2))}</b>`;
      return linkify(K.S, p);
    })
    .join('');
}

// A table row's cells, split at pipes no backslash escapes; `\|` and `\\` read as `|` and `\` (wiki-facts `cell`).
const cellsOf = (line: string) =>
  line
    .trim()
    .replace(/^\||\|$/g, '')
    .split(/(?<!\\)\|/)
    .map(c => c.trim().replace(/\\([\\|])/g, '$1'));

function table(K: Wiki, rows: string[]) {
  const [head, , ...body] = rows;
  const tr = (cells: string[], tag: string, i: number) =>
    `<tr${i >= ROWS ? ' class="wiki-more" hidden' : ''}>${cells.map(c => `<${tag}>${inline(K, c)}</${tag}>`).join('')}</tr>`;
  const more =
    body.length > ROWS
      ? `<button type="button" class="wiki-showall" data-more>Show all ${body.length} rows</button>`
      : '';
  return `<div class="wiki-table"><table><thead>${tr(cellsOf(head), 'th', 0)}</thead><tbody>${body.map((r, i) => tr(cellsOf(r), 'td', i)).join('')}</tbody></table></div>${more}`;
}

const LIST = /^\s*(?:[-*]|\d+\.)\s+/;
const BLOCK_START = /^(#|```|\||>|\s*(?:[-*]|\d+\.)\s)/;
// The index past the run of lines from `i` that satisfy `test`.
function runEnd(lines: string[], i: number, test: (line: string) => unknown) {
  let j = i;
  while (j < lines.length && test(lines[j])) j++;
  return j;
}

type Block = [html: string, next: number];
function fence(K: Wiki, lines: string[], i: number): Block {
  const j = runEnd(lines, i + 1, l => !l.startsWith('```'));
  return [
    `<pre><code>${linkify(K.S, lines.slice(i + 1, j).join('\n'))}</code></pre>`,
    j + 1,
  ];
}

function list(K: Wiki, lines: string[], i: number): Block {
  const j = runEnd(lines, i, l => LIST.test(l));
  const tag = /^\s*\d+\./.test(lines[i]) ? 'ol' : 'ul';
  return [
    `<${tag}>${lines
      .slice(i, j)
      .map(l => `<li>${inline(K, l.replace(LIST, ''))}</li>`)
      .join('')}</${tag}>`,
    j,
  ];
}

// One block starting at line i: returns [html, next index].
function block(K: Wiki, lines: string[], i: number): Block {
  const line = lines[i];
  if (line.startsWith('```')) return fence(K, lines, i);
  if (line.startsWith('|') && /^\|[\s:|-]+\|$/.test(lines[i + 1] ?? '')) {
    const j = runEnd(lines, i, l => l.startsWith('|'));
    return [table(K, lines.slice(i, j)), j];
  }
  if (LIST.test(line)) return list(K, lines, i);
  if (line.startsWith('>'))
    return [
      `<blockquote>${inline(K, line.replace(/^>\s?/, ''))}</blockquote>`,
      i + 1,
    ];
  const j = Math.max(
    runEnd(lines, i, l => l.trim() && !BLOCK_START.test(l)),
    i + 1
  );
  return [`<p>${inline(K, lines.slice(i, j).join(' '))}</p>`, j];
}

// Markdown to HTML; each ### section is collapsible. The page title (# line) is shown in the drawer header.
export function render(K: Wiki, markdown: string) {
  // Removing a comment can join its neighbors into another (`<!<!-- -->-- -->`): remove until none is left.
  let text = markdown;
  let before: string;
  do {
    before = text;
    text = text.replace(/<!--[\s\S]*?-->/g, '');
  } while (text !== before);
  const lines = text.split('\n');
  let html = '',
    open = false,
    i = 0;
  while (i < lines.length) {
    const h = /^(#{1,6})\s+(.*)$/.exec(lines[i]);
    if (!lines[i].trim() || (h && h[1].length === 1)) {
      i++;
      continue;
    }
    if (h) {
      if (open) html += '</details>';
      open = h[1].length >= 3;
      html += open
        ? `<details open><summary>${inline(K, h[2])}</summary>`
        : `<h3 class="wiki-h2">${inline(K, h[2])}</h3>`;
      i++;
      continue;
    }
    const [part, next] = block(K, lines, i);
    html += part;
    i = next;
  }
  return html + (open ? '</details>' : '');
}

const HEAD =
  /^> \*\*Notes:\*\* (none|fresh|stale)[^\n]*?community (\d+|–) · (\d+) files/m;
const SPECIAL: Record<string, string> = {
  index: 'Wiki index',
  log: 'Wiki log',
};

// What a page says about itself: title, notes state, community, size (its header line is regenerated on every write).
function headOf(markdown: string | undefined) {
  const text = markdown ?? '';
  const m = HEAD.exec(text);
  const community = m && m[2] !== '–' ? Number(m[2]) : null;
  return {
    title: /^# (.+)$/m.exec(text)?.[1],
    state: m?.[1],
    community,
    size: m ? Number(m[3]) : 0,
    orphaned: /\*\*orphaned\*\*/.test(text),
  };
}

// Header facts of a page: its own header line first (newest), else the latest wiki event's row.
function infoOf(K: Wiki, slug: string, markdown: string): WikiInfo {
  const h = headOf(markdown);
  if (SPECIAL[slug])
    return { label: SPECIAL[slug], meta: h.title ?? '', state: null };
  const row: Partial<WikiPageRow> = K.W.pages.get(slug) ?? {};
  return {
    slug,
    label: h.title ?? row.label ?? slug,
    size: h.size || row.size,
    community: h.community ?? row.community ?? null,
    state: h.orphaned ? 'orphaned' : (h.state ?? row.notes ?? null),
  };
}

function membersOf(K: Wiki, info: WikiInfo) {
  const m = K.S.metrics;
  if (!m) return [];
  if (info.slug === 'isolated') {
    const big = new Set(m.communities.map(c => c.id));
    return m.ids.filter((_id, i) => !big.has(m.columns.community[i]));
  }
  return m.communities.find(c => c.id === info.community)?.members ?? [];
}

function header(K: Wiki, info: WikiInfo) {
  const { D } = K;
  D.title.textContent = info.label;
  D.meta.textContent = info.size
    ? `${info.size} files${info.community != null ? ` · community ${info.community}` : ''}`
    : (info.meta ?? '');
  D.state.hidden = !info.state;
  D.state.className = `wiki-state ${info.state ?? ''}`;
  D.state.textContent = (info.state ? STATE[info.state] : undefined) ?? '';
  K.W.members = membersOf(K, info);
  D.hl.hidden = !K.W.members.length;
  D.back.disabled = !K.W.history.length;
}

function showPage(
  K: Wiki,
  slug: string | null,
  markdown: string,
  info: WikiInfo
) {
  K.W.slug = slug;
  K.W.fallback = false;
  header(K, info);
  K.D.body.innerHTML = render(K, markdown);
  K.D.body.scrollTop = 0;
}

// Linked communities from the metrics stream's weighted edges, both directions, strongest first.
function linksOf(m: MetricsPayload, id: number) {
  const out = new Map<number, number>(),
    into = new Map<number, number>();
  m.edges.source.forEach((u, k) => {
    if (m.edges.test[k]) return;
    const a = m.columns.community[u],
      b = m.columns.community[m.edges.target[k]];
    if (a === b) return;
    if (a === id) out.set(b, (out.get(b) ?? 0) + m.edges.weight[k]);
    if (b === id) into.set(a, (into.get(a) ?? 0) + m.edges.weight[k]);
  });
  const rows = (map: Map<number, number>) =>
    [...map]
      .toSorted((x, y) => y[1] - x[1])
      .map(
        ([c, w]) =>
          `| ${m.communities.find(x => x.id === c)?.label ?? `isolated (${c})`} | ${fmt(w)} |`
      );
  return { out: rows(out), into: rows(into) };
}

// Facts the viewer already has for a community, as markdown, while the wiki page is on its way.
function fallback(K: Wiki, community: number) {
  const S = K.S,
    m = S.metrics,
    c = m?.communities.find(x => x.id === community);
  if (!m || !c) return null;
  const at = new Map(m.ids.map((id, i) => [id, i]));
  const col = (k: keyof FileMetrics, id: string) => {
    const i = at.get(id);
    return i == null ? undefined : m.columns[k][i];
  };
  const members = c.members.map(
    (id, i) =>
      `| ${i + 1} | \`${id}:1\` | ${fmt(col('pagerank', id))} | ${fmt(col('inWeight', id))} | ${fmt(col('outWeight', id))} | ${col('cycle', id) ?? ''} | ${S.nodes.get(id)?.check?.status ?? ''} |`
  );
  const links = linksOf(m, community);
  const cycles = (m.cycles ?? [])
    .filter(y => y.members.some(id => c.members.includes(id)))
    .map(y => `- cycle ${y.id}: ${y.size} files, ${y.languages.join(', ')}`);
  return [
    '',
    `Waiting for the wiki page: these facts come from the metrics stream. Chunk \`${c.chunk}\`, purity ${fmt(c.purity)}, weight inside ${fmt(c.internal)}, across ${fmt(c.external)}.`,
    '',
    '### Members by PageRank',
    '| # | file | PageRank | in | out | cycle | check |',
    '|---|---|---|---|---|---|---|',
    ...members,
    '',
    '### Links out',
    ...(links.out.length
      ? ['| community | weight |', '|---|---|', ...links.out]
      : ['None.']),
    '',
    '### Links in',
    ...(links.into.length
      ? ['| community | weight |', '|---|---|', ...links.into]
      : ['None.']),
    '',
    '### Cycles',
    ...(cycles.length ? cycles : ['None.']),
  ].join('\n');
}

// The page of a community: learned from an answer to a file request, else the latest `wiki` event.
const slugOfCommunity = (K: Wiki, community: number) =>
  K.W.learned.get(community) ??
  [...K.W.pages.values()].find(p => p.community === community)?.slug ??
  null;

function request(K: Wiki, { retried, ...ask }: WikiRequest) {
  K.W.pending = { ...ask, retried };
  K.send({ op: 'wiki', ...ask });
}

// While the page is on its way: the community's facts from the metrics stream, or a loading line.
function placeholder(K: Wiki, t: WikiTarget, slug: string | null) {
  const { W, D } = K;
  const facts = t.community != null ? fallback(K, t.community) : null;
  const c = facts
    ? K.S.metrics?.communities.find(x => x.id === t.community)
    : null;
  if (c && facts) {
    showPage(K, slug, facts, {
      label: c.label,
      size: c.size,
      community: c.id,
      state: slug ? W.pages.get(slug)?.notes : null,
      slug,
    });
    W.fallback = true;
  } else D.body.innerHTML = '<p class="wiki-empty">Loading the wiki page…</p>';
  D.body.insertAdjacentHTML(
    'afterbegin',
    '<p class="wiki-wait" role="status">Asking the server for the wiki page…</p>'
  );
}

function load(K: Wiki) {
  const { W, D } = K;
  if (!W.target && W.pages.size) W.target = { slug: 'index' };
  if (!W.target) {
    D.body.innerHTML =
      '<p class="wiki-empty">Click a community in the graph or select a file to see its wiki page. <b>Index</b> lists every page.</p>';
    return;
  }
  const t = W.target;
  const slug =
    t.slug ?? (t.community != null ? slugOfCommunity(K, t.community) : null);
  const cached = slug ? W.cache.get(slug) : undefined;
  if (slug && cached !== undefined) {
    showPage(K, slug, cached, infoOf(K, slug, cached));
    return;
  }
  placeholder(K, t, slug);
  const file = targetFile(K, t);
  if (slug) request(K, { slug });
  else if (file) request(K, { file });
}

function go(K: Wiki, target: WikiTarget, remember = true) {
  if (remember && K.W.slug && !K.W.fallback) K.W.history.push(K.W.slug);
  K.W.target = target;
  if (K.W.open) load(K);
}

function setOpen(K: Wiki, open: boolean) {
  K.W.open = open;
  K.D.root.hidden = !open;
  K.D.toggle.classList.toggle('on', open);
  K.D.toggle.setAttribute('aria-expanded', String(open));
  if (open) load(K);
}

// The file a target stands for: its own, else its community's most central member.
const targetFile = (K: Wiki, t: WikiTarget | null) =>
  t?.file ??
  (t?.community != null
    ? K.S.metrics?.communities.find(c => c.id === t.community)?.members[0]
    : null);
const answers = (p: WikiRequest | null, e: WikiPagePayload): p is WikiRequest =>
  p != null && (p.slug ? p.slug === e.slug : !e.file || e.file === p.file);

function onMissing(K: Wiki, p: WikiRequest, e: WikiPagePayload) {
  // A slug from an older wiki event can be gone (renamed or removed): ask for the page that holds the file instead.
  const file = targetFile(K, K.W.target);
  if (p.slug && file && !p.retried) request(K, { file, retried: true });
  else if (K.W.open)
    K.D.body
      .querySelector('.wiki-wait')
      ?.replaceChildren(
        `No wiki page: ${e.error ?? 'the server has none for this yet'}.`
      );
}

function onPage(K: Wiki, e: WikiPagePayload) {
  const { W } = K;
  const found = typeof e.markdown === 'string';
  if (found) W.cache.set(e.slug, e.markdown);
  const p = W.pending;
  if (!answers(p, e)) return;
  W.pending = null;
  if (!found) return onMissing(K, p, e);
  if (W.target?.community != null) W.learned.set(W.target.community, e.slug);
  W.target = { slug: e.slug };
  if (W.open) showPage(K, e.slug, e.markdown, infoOf(K, e.slug, e.markdown));
}

function onWiki(K: Wiki, e: WikiPayload) {
  const { W } = K;
  W.pages = new Map((e.pages ?? []).map(p => [p.slug, p]));
  W.learned.clear();
  for (const slug of [
    ...(e.updated ?? []),
    ...(e.created ?? []),
    ...(e.removed ?? []),
    'index',
    'log',
  ])
    W.cache.delete(slug);
  if (!W.open || !W.slug) return;
  if (
    (e.updated ?? []).includes(W.slug) ||
    W.slug === 'index' ||
    !W.cache.has(W.slug)
  )
    load(K);
}

const TEMPLATE = `
  <div class="wiki-head">
    <div class="wiki-title"><span class="wiki-logo" aria-hidden="true"></span><div class="wiki-name"><b>Wiki</b><small></small></div><span class="wiki-state" hidden></span></div>
    <div class="wiki-actions">
      <button type="button" class="wiki-back" title="back" aria-label="back" disabled>‹</button>
      <button type="button" class="wiki-index" title="all pages (index.md)">Index</button>
      <button type="button" class="wiki-hl" title="highlight this page's files in the graph" hidden>Highlight members</button>
      <button type="button" class="wiki-close" aria-label="close wiki" title="close (w)">×</button>
    </div>
  </div>
  <div class="wiki-body" tabindex="0"></div>`;

function buildDom(root: HTMLElement) {
  root.innerHTML = TEMPLATE;
  const toggle = document.createElement('button');
  toggle.type = 'button';
  toggle.className = 'wiki-toggle';
  toggle.title = 'Code wiki of the selected community (w)';
  toggle.setAttribute('aria-controls', 'wiki');
  toggle.setAttribute('aria-expanded', 'false');
  toggle.innerHTML =
    '<span class="wiki-ic" aria-hidden="true"></span><span class="wiki-label">Wiki</span>';
  const settings = document.querySelector('header .settings');
  if (settings) settings.prepend(toggle);
  else document.querySelector('header')?.append(toggle);
  const $ = (sel: string) => find(root, sel, HTMLElement);
  return {
    root,
    toggle,
    title: $('.wiki-name b'),
    meta: $('.wiki-name small'),
    state: $('.wiki-state'),
    back: find(root, '.wiki-back', HTMLButtonElement),
    index: $('.wiki-index'),
    hl: $('.wiki-hl'),
    close: $('.wiki-close'),
    body: $('.wiki-body'),
  };
}

function onBodyClick(K: Wiki, ev: MouseEvent) {
  const a = closest(ev.target, '[data-wiki],[data-node],[data-more]');
  if (!a) return;
  ev.preventDefault();
  if (a.dataset.wiki) go(K, { slug: a.dataset.wiki });
  else if (a.dataset.node) K.select(a.dataset.node, { focus: true });
  else {
    for (const tr of a.previousElementSibling?.querySelectorAll(
      'tr.wiki-more'
    ) ?? [])
      if (tr instanceof HTMLElement) tr.hidden = false;
    a.remove();
  }
}

function onCommunity(K: Wiki, detail: CommunityDetail | null) {
  const community = detail?.community;
  if (detail?.bucket === ISO_BUCKET) go(K, { slug: 'isolated' });
  else if (typeof community === 'number' && Number.isInteger(community))
    go(K, { community });
}

function onSelect(K: Wiki, id: string | null) {
  if (!id) return;
  const community = K.S.nodes.get(id)?.metrics?.community;
  const big =
    community != null && K.S.metrics?.communities.some(c => c.id === community);
  // A file on the page already shown (a path:line link in it, say) keeps the page and its scroll position.
  if (big && K.W.slug && slugOfCommunity(K, community) === K.W.slug) return;
  go(K, big ? { community, file: id } : { file: id });
}

function bind(K: Wiki) {
  const { D, W } = K;
  D.toggle.addEventListener('click', () => setOpen(K, !W.open));
  D.close.addEventListener('click', () => {
    setOpen(K, false);
    D.toggle.focus();
  });
  D.index.addEventListener('click', () => go(K, { slug: 'index' }));
  D.back.addEventListener('click', () => {
    const slug = W.history.pop();
    if (slug) go(K, { slug }, false);
  });
  D.hl.addEventListener('click', () => K.highlight(W.members));
  D.body.addEventListener('click', ev => onBodyClick(K, ev));
  document.addEventListener('gv:community', ev => onCommunity(K, ev.detail));
  document.addEventListener('gv:select', ev => onSelect(K, ev.detail ?? null));
  document.addEventListener('keydown', ev => {
    if (
      ev.key !== 'w' ||
      ev.metaKey ||
      ev.ctrlKey ||
      ev.altKey ||
      isTyping(ev.target)
    )
      return;
    ev.preventDefault();
    setOpen(K, !W.open);
  });
}

export function createWiki(
  S: LiveState,
  { send, select, highlight }: ViewDeps
): { add(e: LiveEvent<'wiki' | 'wikiPage'>): void } {
  const root = document.getElementById('wiki');
  if (!root) return { add() {} };
  const W: WikiState = {
    open: false,
    target: null,
    slug: null,
    pages: new Map(),
    cache: new Map(),
    learned: new Map(),
    history: [],
    pending: null,
    members: [],
  };
  const K: Wiki = { S, send, select, highlight, W, D: buildDom(root) };
  bind(K);
  return { add: e => (e.type === 'wikiPage' ? onPage(K, e) : onWiki(K, e)) };
}
