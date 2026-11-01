// The server's answers over the finished graph: graph queries (query.mjs) on the in-memory data.json, node
// resolution for replies that point at the graph, and the wiki (wiki.mjs, optional: a clear 501 until it exists).
// Each answer is an event, so the viewer highlights it and a replay shows it again.
import { existsSync } from 'node:fs';
import { readFile, stat } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolveNodes, runQuery } from './query.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const WIKI_API = ['readPage', 'pageOf', 'writeNotes', 'promote', 'lintWiki'];

export const httpError = (status, message, detail) => Object.assign(new Error(message), { status, ...(detail ? { detail } : {}) });

// data.json, reloaded when the file changes (a run's end, a refresh); one load at a time.
function dataLoader(ctx) {
  let cached = null;
  let loading = null;
  const file = join(ctx.out, 'data.json');
  async function load() {
    const s = await stat(file).catch(() => null);
    if (!s) throw httpError(409, ctx.building ? 'the graph is still building: queries answer once data.json is written' : `no data.json in ${ctx.out}: build or refresh the result first`);
    const key = `${s.mtimeMs}:${s.size}`;
    if (cached?.key !== key) cached = { key, D: JSON.parse(await readFile(file, 'utf8')) };
    return cached.D;
  }
  return () => {
    loading ??= load().finally(() => { loading = null; });
    return loading;
  };
}

async function wikiModule() {
  const unavailable = 'wiki unavailable: this copy of the skill has no scripts/wiki.mjs with readPage, pageOf, writeNotes, promote and lintWiki';
  if (!existsSync(join(here, 'wiki.mjs'))) throw httpError(501, unavailable);
  const m = await import('./wiki.mjs');
  if (!WIKI_API.every((k) => typeof m[k] === 'function')) throw httpError(501, unavailable);
  return m;
}

function needText(t) {
  if (typeof t !== 'string' || !t.trim()) throw httpError(400, 'text must be a non-empty string');
  return t;
}

const runRoot = (ctx) => ctx.root ?? ctx.L.log.find((e) => e.type === 'run')?.root ?? null;

// The agent's answer to a chat message (the user message's id, or the agent reply's own id) and its question, for
// promoting it to the wiki: the latest reply wins.
function chatAnswer(ctx, chatId) {
  const chat = ctx.L.log.filter((e) => e.type === 'chat' && e.role);
  const own = chat.find((e) => e.id === chatId);
  const answer = own?.role === 'agent' ? own : chat.findLast((e) => e.role === 'agent' && e.replyTo === chatId);
  if (!answer) throw httpError(404, `no agent reply to chat message ${chatId} in this viewer`);
  return { text: answer.text, question: chat.find((e) => e.role === 'user' && e.id === answer.replyTo) ?? null };
}

function createWiki(ctx, data) {
  const out = ctx.out;
  // A file argument resolves like a query argument when the graph is loaded; the wiki maps it to its page.
  async function pageSlug({ slug, file }, W) {
    if (slug) return String(slug);
    if (!file) throw httpError(400, 'pass slug or file');
    const D = await data().catch(() => null);
    const r = D ? resolveNodes(D, [file]) : { ids: [String(file)], problems: [] };
    if (r.problems.length) throw httpError(400, r.problems[0].error, { candidates: r.problems[0].candidates });
    const page = await W.pageOf({ out, file: r.ids[0] });
    if (!page) throw httpError(404, `no wiki page holds ${r.ids[0]}`);
    return page;
  }
  async function read(body, W) {
    const slug = await pageSlug(body, W);
    const page = await W.readPage({ out, slug });
    if (!page) throw httpError(404, `no wiki page ${slug}`);
    return page;
  }
  // After a write the viewer gets the page again, so an open wiki drawer shows the new notes.
  async function show(W, slug) {
    const page = await Promise.resolve().then(() => W.readPage({ out, slug })).catch(() => null);
    if (page) ctx.L.emit({ type: 'wikiPage', slug: page.slug ?? slug, markdown: page.markdown }, true);
  }
  const actions = {
    read: (b, W) => read(b, W),
    note: async (b, W) => {
      const slug = await pageSlug(b, W);
      const r = await W.writeNotes({ out, slug, text: needText(b.text) });
      await show(W, slug);
      return r;
    },
    promote: async (b, W) => {
      const from = b.chat_id ? chatAnswer(ctx, b.chat_id) : null;
      const slug = await pageSlug({ slug: b.slug, file: b.file ?? from?.question?.context?.file }, W);
      const source = from ? `viewer chat ${b.chat_id}${from.question ? `: "${from.question.text}"` : ''}` : String(b.source ?? 'agent');
      const r = await W.promote({ out, slug, text: needText(b.text ?? from?.text), source });
      await show(W, slug);
      return r;
    },
    lint: (b, W) => W.lintWiki({ out, root: runRoot(ctx) }),
  };
  return async (body = {}) => {
    const act = actions[body.action];
    if (!act) throw httpError(400, `wiki action must be one of ${Object.keys(actions).join(', ')}`);
    return { ok: true, ...(await act(body, await wikiModule())) };
  };
}

/** ctx: the server's context (out, building, L). Returns query, nodes (resolve for replies), wiki and ops. */
export function createAnswers(ctx) {
  const data = dataLoader(ctx);
  const wiki = createWiki(ctx, data);
  async function query(text, from) {
    if (typeof text !== 'string' || !text.trim()) throw httpError(400, 'query must be a non-empty string');
    const r = runQuery(await data(), text);
    return ctx.L.emit({ type: 'query', from, at: Date.now(), ...r }, true);
  }
  async function nodes(list) {
    if (list == null) return [];
    if (!Array.isArray(list) || list.some((x) => typeof x !== 'string')) throw httpError(400, 'nodes must be a list of file paths, suffixes or node ids');
    if (!list.length) return [];
    const r = resolveNodes(await data(), list);
    if (r.problems.length) throw httpError(400, `cannot point at ${r.problems.map((p) => `${p.arg} (${p.error})`).join('; ')}`, { problems: r.problems });
    return r.ids;
  }
  const ops = {
    // A failed load (still building, no data.json) is a query event too, so the viewer shows why.
    query: (m) => query(m.text, 'viewer').catch((error) => ctx.L.emit({ type: 'query', from: 'viewer', at: Date.now(), ok: false, op: '', text: String(m.text ?? ''), error: error.message, summary: error.message, nodes: [], edges: [], rows: [] }, true)),
    wiki: (m) => wiki({ action: 'read', slug: m.slug, file: m.file })
      .then((page) => ctx.L.emit({ type: 'wikiPage', slug: page.slug, markdown: page.markdown }))
      .catch((error) => ctx.L.emit({ type: 'wikiPage', slug: m.slug ?? null, file: m.file ?? null, markdown: null, error: error.message })),
  };
  return { query, nodes, wiki, ops, prime: () => data().then(() => true, () => false) };
}
