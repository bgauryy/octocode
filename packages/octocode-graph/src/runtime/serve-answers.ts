// The server's answers over the finished graph: graph queries (query.ts) on the in-memory data.json, node
// resolution for replies that point at the graph, and the wiki (wiki.ts).
// Each answer is an event, so the viewer highlights it and a replay shows it again.
import { readFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
import * as wikiApi from './wiki.ts';
import {
  resolveNodes,
  runQuery,
  type NodeProblem,
  type QueryData,
  editedSince,
  withStaleness,
} from './query.ts';
import { textOf } from './query-index.ts';
import { withSearchIndex } from './search-sidecar.ts';
import { withProvider } from './search-provider.ts';
import type { RunEvent } from './types.ts';
import type { LintIssue, WikiPage } from './wiki.ts';
import { errorMessage } from './lib.ts';

/** The graph the answers read: the run's live snapshot while it finishes, else <out>/data.json. */
export type ReadLive = () => QueryData;

/** The event log's emit (serve.ts): numbers and times the event, saves it when asked, and returns it. */
export type LogEmit = <E extends RunEvent>(
  event: E,
  save?: boolean
) => E & RunEvent & { seq: number; t: number };

/** What dataLoader reads from the server's context; the channel passes only `out`. */
export interface LoaderContext {
  out: string;
  building?: boolean;
  readLive?: ReadLive | null;
}

/** What the answers read from the server's context (serve.ts). */
export interface AnswerContext extends LoaderContext {
  root: string | null;
  L: { readonly log: readonly RunEvent[]; emit: LogEmit };
}

/** A wiki request (POST /wiki, the channel's wiki tools, the viewer); each field is checked where it is read. */
export interface WikiBody {
  action?: unknown;
  slug?: unknown;
  file?: unknown;
  text?: unknown;
  chat_id?: unknown;
  source?: unknown;
}

/** A wiki answer: `ok` and the action's result (a page, a notes write, a promotion or the lint issues). */
export interface WikiAnswer {
  ok: true;
  slug?: string;
  markdown?: string;
  updated?: boolean;
  appended?: boolean;
  issues?: LintIssue[];
}

type WikiApi = typeof wikiApi;
type WikiResult =
  | WikiPage
  | { slug: string; updated: boolean }
  | { slug: string; appended: boolean }
  | { issues: LintIssue[] };
type WikiAction = (
  body: WikiBody,
  W: WikiApi
) => WikiResult | Promise<WikiResult>;

/** A message from the viewer's socket: a parsed JSON object; each op checks the fields it reads. */
export type ViewerMessage = Record<string, unknown>;

/** A refused request: its HTTP status and, for the JSON answer, the candidates or problems behind it. */
export class HttpError extends Error {
  readonly status: number;
  readonly detail: Record<string, unknown> | undefined;
  constructor(
    status: number,
    message: string,
    detail?: Record<string, unknown>
  ) {
    super(message);
    this.status = status;
    this.detail = detail;
  }
}

/** A parsed JSON value that is an object (arrays included): its fields can be read. */
export const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null;

/** The fields of a parsed JSON value: an object's own, none for null or a scalar. */
export const fieldsOf = (value: unknown): Record<string, unknown> =>
  isRecord(value) ? value : {};

const isStringList = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every((x: unknown) => typeof x === 'string');

/** The root the run event of a log names (a replay has no other). */
export function logRoot(log: readonly RunEvent[]) {
  const root = log.find(e => e.type === 'run')?.root;
  return typeof root === 'string' ? root : undefined;
}

// data.json, reloaded when the file changes (a run's end, a refresh); one load at a time.
export function dataLoader(ctx: LoaderContext): () => Promise<QueryData> {
  let cached: { key: string; D: QueryData } | null = null;
  let loading: Promise<QueryData> | null = null;
  const file = join(ctx.out, 'data.json');
  async function load() {
    const s = await stat(file, { bigint: true }).catch(() => null);
    if (!s)
      throw new HttpError(
        409,
        ctx.building
          ? 'the graph is still building: queries open as soon as the chunks are stitched'
          : `no data.json in ${ctx.out}: build or refresh the result first`
      );
    const key = `${s.dev}:${s.ino}:${s.mtimeNs}:${s.ctimeNs}:${s.size}`;
    // data.json is this package's own output: the query engine tolerates any missing field.
    if (cached?.key !== key)
      cached = {
        key,
        D: withSearchIndex(
          JSON.parse(await readFile(file, 'utf8')) as QueryData,
          file
        ),
      };
    return cached.D;
  }
  return async () => {
    if (ctx.readLive) return ctx.readLive();
    loading ??= load().finally(() => {
      loading = null;
    });
    return loading;
  };
}

function needText(t: unknown) {
  if (typeof t !== 'string' || !t.trim())
    throw new HttpError(400, 'text must be a non-empty string');
  return t;
}

const runRoot = (ctx: AnswerContext) => ctx.root ?? logRoot(ctx.L.log) ?? null;

// The agent's answer to a chat message (the user message's id, or the agent reply's own id) and its question, for
// promoting it to the wiki: the latest reply wins.
function chatAnswer(ctx: AnswerContext, chatId: unknown) {
  const chat = ctx.L.log.filter(e => e.type === 'chat' && e.role);
  const own = chat.find(e => e.id === chatId);
  const answer =
    own?.role === 'agent'
      ? own
      : chat.findLast(e => e.role === 'agent' && e.replyTo === chatId);
  if (!answer)
    throw new HttpError(
      404,
      `no agent reply to chat message ${String(chatId)} in this viewer`
    );
  return {
    text: answer.text,
    question:
      chat.find(e => e.role === 'user' && e.id === answer.replyTo) ?? null,
  };
}

function createWiki(ctx: AnswerContext, data: () => Promise<QueryData>) {
  const out = ctx.out;
  // A file argument resolves like a query argument when the graph is loaded; the wiki maps it to its page.
  async function pageSlug({ slug, file }: WikiBody, W: WikiApi) {
    if (slug) return textOf(slug);
    if (!file) throw new HttpError(400, 'pass slug or file');
    const D = await data().catch(() => null);
    const r: { ids: string[]; problems: NodeProblem[] } = D
      ? resolveNodes(D, [file])
      : { ids: [textOf(file)], problems: [] };
    if (r.problems.length)
      throw new HttpError(400, r.problems[0].error, {
        candidates: r.problems[0].candidates,
      });
    const page = W.pageOf({ out, file: r.ids[0] });
    if (!page) throw new HttpError(404, `no wiki page holds ${r.ids[0]}`);
    return page;
  }
  async function read(body: WikiBody, W: WikiApi) {
    const slug = await pageSlug(body, W);
    const page = W.readPage({ out, slug });
    if (!page) throw new HttpError(404, `no wiki page ${slug}`);
    return page;
  }
  // After a write the viewer gets the page again, so an open wiki drawer shows the new notes.
  async function show(W: WikiApi, slug: string) {
    const page = await Promise.resolve()
      .then(() => W.readPage({ out, slug }))
      .catch(() => null);
    if (page)
      ctx.L.emit(
        { type: 'wikiPage', slug: page.slug ?? slug, markdown: page.markdown },
        true
      );
  }
  const actions: Partial<Record<string, WikiAction>> = {
    read: (b, W) => read(b, W),
    note: async (b, W) => {
      const slug = await pageSlug(b, W);
      const r = W.writeNotes({ out, slug, text: needText(b.text) });
      await show(W, slug);
      return r;
    },
    promote: async (b, W) => {
      const from = b.chat_id ? chatAnswer(ctx, b.chat_id) : null;
      const context = from?.question?.context;
      const slug = await pageSlug(
        {
          slug: b.slug,
          file: b.file ?? (isRecord(context) ? context.file : undefined),
        },
        W
      );
      const source = from
        ? `viewer chat ${String(b.chat_id)}${from.question ? `: "${String(from.question.text)}"` : ''}`
        : textOf(b.source ?? 'agent');
      const r = W.promote({
        out,
        slug,
        text: needText(b.text ?? from?.text),
        source,
      });
      await show(W, slug);
      return r;
    },
    lint: (_body, W) => W.lintWiki({ out, root: runRoot(ctx) }),
  };
  return async (body: WikiBody = {}): Promise<WikiAnswer> => {
    const name = String(body.action);
    const act = Object.hasOwn(actions, name) ? actions[name] : undefined;
    if (!act)
      throw new HttpError(
        400,
        `wiki action must be one of ${Object.keys(actions).join(', ')}`
      );
    return { ok: true, ...(await act(body, wikiApi)) };
  };
}

/** ctx: the server's context (out, building, L). Returns query, nodes (resolve for replies), wiki and ops. */
export function createAnswers(ctx: AnswerContext) {
  const data = dataLoader(ctx);
  const wiki = createWiki(ctx, data);
  async function query(text: unknown, from: string) {
    if (typeof text !== 'string' || !text.trim())
      throw new HttpError(400, 'query must be a non-empty string');
    const D = await data();
    const r = await withProvider(
      withStaleness(
        runQuery(D, text),
        editedSince(D, join(ctx.out, 'data.json'))
      )
    );
    return ctx.L.emit({ type: 'query', from, at: Date.now(), ...r }, true);
  }
  async function nodes(list: unknown) {
    if (list == null) return [];
    if (!isStringList(list))
      throw new HttpError(
        400,
        'nodes must be a list of file paths, suffixes or node ids'
      );
    if (!list.length) return [];
    const r = resolveNodes(await data(), list);
    if (r.problems.length)
      throw new HttpError(
        400,
        `cannot point at ${r.problems.map(p => `${p.arg} (${p.error})`).join('; ')}`,
        { problems: r.problems }
      );
    return r.ids;
  }
  const ops = {
    // A failed load (still building, no data.json) is a query event too, so the viewer shows why.
    query: (m: ViewerMessage) =>
      query(m.text, 'viewer').catch((error: unknown) =>
        ctx.L.emit(
          {
            type: 'query',
            from: 'viewer',
            at: Date.now(),
            ok: false,
            op: '',
            text: textOf(m.text),
            error: errorMessage(error),
            summary: errorMessage(error),
            nodes: [],
            edges: [],
            rows: [],
          },
          true
        )
      ),
    wiki: (m: ViewerMessage) =>
      wiki({ action: 'read', slug: m.slug, file: m.file })
        .then(page =>
          ctx.L.emit({
            type: 'wikiPage',
            slug: page.slug,
            markdown: page.markdown,
          })
        )
        .catch((error: unknown) =>
          ctx.L.emit({
            type: 'wikiPage',
            slug: m.slug ?? null,
            file: m.file ?? null,
            markdown: null,
            error: errorMessage(error),
          })
        ),
  };
  return {
    query,
    nodes,
    wiki,
    ops,
    prime: () =>
      data().then(
        () => true,
        () => false
      ),
  };
}

export type Answers = ReturnType<typeof createAnswers>;
