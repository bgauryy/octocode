// The package's MCP server over stdio, in two modes from one implementation:
// - `mcp` (octocode-graph mcp): the saved-graph tools (build_graph, query_graph, refresh_graph, diff_graph,
//   check_graph, link_graph) and the engine's own graphBuild and graphQuery, passed through unchanged from a child
//   `octocode __graph mcp` (its tools/list entries and tools/call results, cursors and errors included);
// - `channel` (octocode-graph channel): the same, plus the live viewer as a Claude Code channel (research preview):
//   open_graph, reply and the wiki tools, each viewer chat message pushed as a `notifications/claude/channel` event.
// Zero dependencies: newline-delimited JSON-RPC 2.0; stdout carries only JSON-RPC, every log goes to stderr.
// Channel contract: https://code.claude.com/docs/en/channels-reference.md
import { spawn, type ChildProcess } from 'node:child_process';
import { createInterface } from 'node:readline';
import { resolveNativeBin } from 'octocode/native';
import { statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { defaults } from './pipeline.ts';
import { lspMode, lspModes } from './lib/run-options.ts';
import {
  HINT,
  formatResult,
  runQuery,
  type QueryData,
  type QueryResult,
} from './query-api.ts';
import {
  dataLoader,
  fieldsOf,
  isRecord,
  type WikiAnswer,
} from './serve-answers.ts';
import {
  openBrowser,
  startServer,
  type ChatSelection,
  type ChatTransport,
  type ViewerServer,
} from './serve.ts';
import { errorMessage } from './lib.ts';
import { packageVersion } from './invocation.ts';
import {
  buildGraph,
  openGraph as openSaved,
  previousGraph,
  refreshGraph as refreshSaved,
} from './api.ts';
import { formatDiff } from './diff.ts';
import {
  FORMATS as CHECK_FORMATS,
  runCheck,
  type CheckFormat,
} from './check.ts';
import {
  LinkRefused,
  proposeLink,
  readProposed,
  removeLink,
  reviewLink,
} from './links-proposed.ts';
import { editedSince, resultDir as savedDir, withStaleness } from './query.ts';
import { nearest } from './query-forgiving.ts';
import { withProvider } from './search-provider.ts';

/** A JSON-RPC 2.0 request: a method and an id (echoed back as the client sent it). */
interface JsonRpcRequest {
  id: unknown;
  method: string;
  params?: unknown;
}

interface JsonRpcError {
  code: number;
  message: string;
}

/** What this server writes: a response (result or error) to a request, or a notification. */
type JsonRpcMessage =
  | { id: unknown; result: unknown }
  | { id: unknown; error: JsonRpcError }
  | { method: string; params: unknown };

/** A JSON-RPC method handler; `params` are the request's (none: an empty object). */
type MethodHandler = (params: Record<string, unknown>) => unknown;

/** The JSON Schema of one tool argument. */
interface ArgumentSchema {
  type: 'string' | 'boolean' | 'array' | 'object';
  description?: string;
  enum?: readonly string[];
  items?: { type: 'string' };
}

/** One MCP tool as tools/list describes it. */
interface ToolDefinition {
  name: string;
  description: string;
  inputSchema: InputSchema;
}

/** A tool's arguments: only the listed properties are accepted. */
interface InputSchema {
  type: 'object';
  properties: Record<string, ArgumentSchema>;
  required?: string[];
  additionalProperties: false;
}

/** A tools/call result: text content, flagged when the tool failed. */
interface ToolResult {
  content: { type: 'text'; text: string }[];
  isError?: boolean;
}

/** A tools/call's arguments (none: an empty object); each tool checks the ones it reads. */
type ToolArgs = Record<string, unknown>;
/** What a tool returns: a finished result, or a value sent as text (a string verbatim, anything else as JSON). */
type ToolReturn = ToolResult | string | Record<string, unknown>;
type ToolCall = (args: ToolArgs) => Promise<ToolReturn>;

/** An open viewer: `n` numbers its chat ids, `key` is the repository root or the replayed result directory. */
interface Viewer {
  n: number;
  key: string;
  server: ViewerServer;
}

// Claude Code does not register a channel that negotiates MCP revision 2026-07-28 (it cannot carry channel
// messages), so this server answers with an earlier revision: the client's own when it is one of these.
const PROTOCOLS = ['2025-06-18', '2025-03-26', '2024-11-05'];
const SOURCE = 'graph-view';

const SAVED_INSTRUCTIONS =
  "Build a graph once with build_graph, then ask query_graph with its result directory: start with search <name> and about <file|symbol>; every row cites path:line and the answer names the next Octocode call. After edits, query_graph with refresh: true answers from a refreshed graph; refresh_graph, then diff_graph, shows what changed. link_graph records a link the graph cannot find, with quoted evidence. graphBuild and graphQuery are the engine's single-scope tools.";
const INSTRUCTIONS = [
  `Viewer messages arrive as <channel source="${SOURCE}" chat_id="...">text</channel>. A selection can include a repository-relative file, line and metrics.`,
  'Send the answer with reply using that chat_id. Include nodes to highlight relevant files, or use path:line for a source link.',
  'Use query_graph for structural questions. Check source or LSP when the answer depends on an uncertain binding.',
  'wiki_read provides community context. Use wiki_promote to save a reply when requested, and refresh_graph when the viewer needs to reflect edits.',
].join(' ');

// Each field is defined once; the tools compose them.
const FIELD = {
  source: {
    type: 'string',
    description:
      'Source file or directory to graph (absolute, or relative to the session working directory).',
  },
  lsp: {
    type: 'string',
    enum: lspModes,
    description:
      'Confirm calls with the language server: smart (default) in hub files and across chunks, all for every function, off for file structure only (fastest).',
  },
  whole: {
    type: 'boolean',
    description: 'One snapshot instead of one per package manifest.',
  },
  result: {
    type: 'string',
    description:
      'A saved result directory, or a project directory: its newest saved graph under .octocode/octocode-graph, else the newest the per-user graph registry lists for it (a build with an output directory elsewhere).',
  },
  viewer: {
    type: 'string',
    description:
      'Which open viewer (its repository path or result directory); default: the most recently used.',
  },
  format: {
    type: 'string',
    enum: ['text', 'json'],
    description: 'text (default): the readable answer; json: every field.',
  },
  slug: { type: 'string', description: 'Wiki page slug.' },
  chat_id: {
    type: 'string',
    description: 'The chat_id attribute of the <channel> message.',
  },
} satisfies Record<string, ArgumentSchema>;

const object = (
  properties: Record<string, ArgumentSchema>,
  required: string[] = []
): InputSchema => ({
  type: 'object',
  properties,
  ...(required.length && { required }),
  additionalProperties: false,
});

const TOOLS_ALL: ToolDefinition[] = [
  {
    name: 'build_graph',
    description:
      'Build the stitched code graph of a file or directory, without a viewer. Returns the result directory with file, import and call counts and the LSP state: the result for query_graph, refresh_graph and diff_graph.',
    inputSchema: object(
      { path: FIELD.source, lsp: FIELD.lsp, whole: FIELD.whole },
      ['path']
    ),
  },
  {
    name: 'query_graph',
    description:
      'Answer a structural question from a saved graph: callers, impact, dependency paths, cycles, rankings, critical code. Start with `search <name>`, then `about <file|symbol>`. Returns every row with path:line, `read as:` when the words were read as another query, coverage warnings, and the next Octocode call: localFetch to read the cited source, lspSearch to confirm callers before claiming a complete list. With path instead of result, the open viewer answers and highlights the rows.',
    inputSchema: object(
      {
        query: {
          type: 'string',
          description: `One query in words; natural phrasing and typos are read as the nearest form. Forms: ${HINT}`,
        },
        result: FIELD.result,
        path: FIELD.viewer,
        format: FIELD.format,
        refresh: {
          type: 'boolean',
          description:
            'true: when graph files changed since the graph was written, refresh the chunks holding them first (as refresh_graph) and answer from the refreshed graph; unchanged, answer directly.',
        },
      },
      ['query']
    ),
  },
  {
    name: 'refresh_graph',
    description:
      'Update a graph after edits: rebuilds only the chunks holding a change, then stitches and measures the whole graph again. Returns the time and the rebuilt chunks; the replaced graph stays for diff_graph. An open viewer updates in place.',
    inputSchema: object({ result: FIELD.result, path: FIELD.viewer }),
  },
  {
    name: 'diff_graph',
    description:
      'Compare two states of a saved graph. Returns review findings first (new or grown cycles, new package dependencies against stability, files with more dependents, new hotspots), then every dependency added or removed and every rename. With result alone: the graph its last refresh replaced, against now.',
    inputSchema: object({
      result: FIELD.result,
      before: {
        ...FIELD.result,
        description: 'The older saved graph, with after.',
      },
      after: {
        ...FIELD.result,
        description: 'The newer saved graph; default: result.',
      },
      format: FIELD.format,
    }),
  },
  {
    name: 'check_graph',
    description:
      "Check dependency rules over a saved graph, as CI does: forbidden, allowed and required rules in dependency-cruiser's vocabulary (paths as regular expressions over file ids and pkg:<name>; reachable, circular with via and viaOnly, dependencyTypes, $1 captures, a load per rule) from .octocode/graph-rules.json at the graph root. Returns each violation with its chain of imports, each hop with its line and kinds. A baseline marks known violations, which never fail; since reports only what a change since that git revision introduced, after refreshing the changed chunks. An error violation the baseline does not list makes the result an error.",
    inputSchema: object({
      result: FIELD.result,
      rules: {
        type: 'string',
        description:
          'The rules file; default: .octocode/graph-rules.json at the graph root.',
      },
      baseline: {
        type: 'string',
        description:
          'Known violations; default: graph-rules.baseline.json beside the rules file, when it exists.',
      },
      write_baseline: {
        type: 'boolean',
        description:
          'true: record every current violation as known in the baseline file.',
      },
      since: {
        type: 'string',
        description:
          'A git revision: refresh the changed chunks, then report only the violations the change introduced (git is only read).',
      },
      format: {
        type: 'string',
        enum: [...CHECK_FORMATS],
        description:
          'text (default); json: every field; sarif: SARIF 2.1.0 with each chain as codeFlows.',
      },
    }),
  },
  {
    name: 'link_graph',
    description:
      "Record a link between files or symbols that static rules cannot settle (a doc's ambiguous or unconfirmed mention, a config a file reads, a concept spread over files), with quoted evidence; the tool verifies every quote before it stores the link, and every build re-checks it. Find candidates with query_graph `links --unresolved`. Returns the stored link with its confidence (candidate, likely, confirmed), or the refusal reason.",
    inputSchema: object(
      {
        result: FIELD.result,
        action: {
          type: 'string',
          enum: ['add', 'remove', 'review', 'list'],
          description:
            'add a link, remove one by id, review one (a second reviewer confirms it), or list them.',
        },
        link: {
          type: 'object',
          description:
            "For add: {relation: documents|configures|relates, source, target (file or file#symbol), claim, anchors: [{path, line, endLine?, quote}] with one in each endpoint's file, each quote at its lines and once in its file, proposedBy}.",
        },
        id: {
          type: 'string',
          description: 'For remove and review: the link id.',
        },
        reviewer: {
          type: 'string',
          description: 'For review: who confirms it (not its proposer).',
        },
      },
      ['result', 'action']
    ),
  },
  {
    name: 'open_graph',
    description:
      'Show the live code graph of a file or directory in the browser viewer (an open viewer for it is reused), or reopen a saved result. Returns the viewer URL; questions typed there arrive as <channel> messages.',
    inputSchema: object({
      path: FIELD.source,
      result: FIELD.result,
      lsp: FIELD.lsp,
      whole: FIELD.whole,
    }),
  },
  {
    name: 'reply',
    description:
      "Answer a viewer chat message; the answer appears in the viewer's chat drawer.",
    inputSchema: object(
      {
        chat_id: FIELD.chat_id,
        text: {
          type: 'string',
          description:
            'The answer: `code` spans render as code; path:line mentions of graph files become links.',
        },
        nodes: {
          type: 'array',
          items: { type: 'string' },
          description:
            'Files to highlight: paths, unique suffixes or node ids. An unknown or ambiguous one refuses the reply with candidates.',
        },
      },
      ['chat_id', 'text']
    ),
  },
  {
    name: 'wiki_read',
    description:
      "Read a page of the open graph's wiki: by slug, or the community page holding a file. Returns its markdown.",
    inputSchema: object({
      slug: FIELD.slug,
      file: {
        type: 'string',
        description:
          'A file (path, suffix or id) whose community page to read.',
      },
      path: FIELD.viewer,
    }),
  },
  {
    name: 'wiki_note',
    description:
      "Replace a wiki page's agent notes with this markdown; the viewer shows the updated page.",
    inputSchema: object(
      {
        slug: FIELD.slug,
        text: { type: 'string', description: 'The notes, markdown.' },
        path: FIELD.viewer,
      },
      ['slug', 'text']
    ),
  },
  {
    name: 'wiki_promote',
    description:
      "Append an answer to a wiki page's notes with its date and source: the latest reply to a chat_id, or text. Without slug: the page of the file asked about.",
    inputSchema: object({
      slug: FIELD.slug,
      chat_id: FIELD.chat_id,
      text: {
        type: 'string',
        description: 'Text to append instead of a chat reply.',
      },
      path: FIELD.viewer,
    }),
  },
  {
    name: 'wiki_lint',
    description:
      'Check the wiki against the graph and the files. Returns stale, missing or contradicted pages and bad citations.',
    inputSchema: object({ path: FIELD.viewer }),
  },
];

/** Which tools a mode serves: `mcp` the saved-graph tools, `channel` those and the viewer's. */
export type Mode = 'mcp' | 'channel';
const VIEWER_TOOLS = new Set([
  'open_graph',
  'reply',
  'wiki_read',
  'wiki_note',
  'wiki_promote',
  'wiki_lint',
]);
const VIEWER_SENTENCES = [
  ' With path instead of result, the open viewer answers and highlights the rows.',
  ' An open viewer updates in place.',
];

// Without a viewer, a saved-graph tool takes its result directory, and its text names no viewer.
function toolsFor(mode: Mode): ToolDefinition[] {
  if (mode === 'channel') return TOOLS_ALL;
  return TOOLS_ALL.filter(tool => !VIEWER_TOOLS.has(tool.name)).map(tool => {
    if (!('path' in tool.inputSchema.properties) || tool.name === 'build_graph')
      return tool;
    const { path: _viewer, ...properties } = tool.inputSchema.properties;
    return {
      ...tool,
      description: VIEWER_SENTENCES.reduce(
        (d, s) => d.replace(s, ''),
        tool.description
      ),
      inputSchema: object(properties, [
        ...new Set([...(tool.inputSchema.required ?? []), 'result']),
      ]),
    };
  });
}
let mode: Mode = 'channel';
let TOOLS: ToolDefinition[] = TOOLS_ALL;

const log = (text: string) =>
  process.stderr.write(`graph-view channel: ${text}\n`);
const write = (message: JsonRpcMessage) =>
  process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', ...message })}\n`);
const text = (value: unknown): ToolResult => ({
  content: [
    {
      type: 'text',
      text: typeof value === 'string' ? value : JSON.stringify(value),
    },
  ],
});

// A string argument where a path or directory is expected.
function pathArg(value: unknown, name: string) {
  if (typeof value !== 'string')
    throw new TypeError(`${name} must be a string`);
  return value;
}

// Open viewers by key (repository root or replayed result directory); channel chat_ids map back to their viewer.
const viewers = new Map<string, Viewer>();
const chats = new Map<string, { viewer: Viewer; id: string }>();
let lastViewer: Viewer | null = null;
let nextViewer = 1;

function metricLine(metrics: ChatSelection['metrics']) {
  const parts = Object.entries(metrics ?? {}).map(
    ([k, v]) =>
      `${k}=${typeof v === 'number' && !Number.isInteger(v) ? v.toPrecision(4) : v}`
  );
  return parts.length ? `\n\n(selected node metrics: ${parts.join(', ')})` : '';
}

// The viewer's transport: only messages from its token-authenticated browser socket reach this function.
function deliverer(viewer: Viewer): ChatTransport {
  return m => {
    const chatId = `${viewer.n}_${m.id}`;
    chats.set(chatId, { viewer, id: m.id });
    lastViewer = viewer;
    const meta = {
      chat_id: chatId,
      ...(m.context
        ? { file: m.context.file, line: String(m.context.line) }
        : {}),
    };
    write({
      method: 'notifications/claude/channel',
      params: { content: `${m.text}${metricLine(m.context?.metrics)}`, meta },
    });
  };
}

function viewerFor(path: unknown) {
  if (path) {
    const target = resolve(pathArg(path, 'path'));
    const available = [...viewers.values()].toReversed();
    const v =
      available.find(x => x.server.out === target) ??
      [lastViewer, ...available].find(
        x => x && (x.key === target || x.server.root() === target)
      );
    if (!v)
      throw new Error(`no graph is open for ${target}; call open_graph first`);
    return v;
  }
  if (!lastViewer) throw new Error('no graph is open; call open_graph first');
  return lastViewer;
}

function directory(path: unknown, what: string) {
  const p = resolve(pathArg(path, what));
  if (!statSync(p, { throwIfNoEntry: false })?.isDirectory())
    throw new Error(`${what} ${p} is not a directory`);
  return p;
}

async function openGraph(args: ToolArgs) {
  if (!args.path && !args.result)
    throw new Error(
      'open_graph needs path (a source file or directory) or result (a result directory)'
    );
  const replay = Boolean(args.result);
  const key = replay
    ? directory(args.result, 'result')
    : resolve(pathArg(args.path, 'path'));
  const root = replay ? null : key;
  const opts = {
    ...defaults(),
    whole: Boolean(args.whole),
    lsp: lspMode(args.lsp),
  };
  const buildKey = JSON.stringify(replay ? [key] : [key, opts.whole, opts.lsp]);
  const open = process.env.GRAPH_VIEW_NO_OPEN !== '1';
  const known = viewers.get(buildKey);
  if (known) {
    if (open) openBrowser(known.server.url);
    lastViewer = known;
    return { url: known.server.url, out: known.server.out, reused: true };
  }
  // `server` is set right below: a chat needs the browser, which connects only after startServer returns.
  const viewer = { n: nextViewer++, key } as Viewer;
  viewer.server = await startServer({
    root,
    out: replay ? key : undefined,
    opts,
    replay,
    open,
    transport: deliverer(viewer),
  });
  viewers.set(buildKey, viewer);
  lastViewer = viewer;
  void viewer.server.finished.then(r =>
    log(`${key}: build ${r.ok ? 'done' : `failed: ${r.error}`}`)
  );
  log(`viewer for ${key}: ${viewer.server.url}`);
  return { url: viewer.server.url, out: viewer.server.out, building: !replay };
}

async function reply(args: ToolArgs) {
  const chat = chatOf(args.chat_id);
  lastViewer = chat.viewer;
  const r = await chat.viewer.server.reply({
    text: args.text,
    chat_id: chat.id,
    nodes: args.nodes,
  });
  return r.nodes
    ? `sent; pointing at ${r.nodes.length} node${r.nodes.length > 1 ? 's' : ''}: ${r.nodes.join(', ')}`
    : 'sent';
}

let saved: { out: string; read: () => Promise<QueryData> } | undefined;
// One output for both modes: the text an agent reads (rows with path:line, `read as:`, warnings and the next
// Octocode call), or with format "json" every field of the answer.
function answer(r: QueryResult, format: unknown): ToolResult {
  return {
    ...text(format === 'json' ? r : formatResult(r)),
    // A broken rule (reach --forbid: `failed`) is an error result like a failed query; the text and json keep it.
    ...(r.ok && !r.failed ? {} : { isError: true }),
  };
}

// A saved graph's result directory: the directory itself, or the newest saved graph of a project directory (the
// default places, then the graph registry).
const resultDir = (arg: unknown) => savedDir(pathArg(arg, 'result'));

// A saved result refreshes through this channel's viewer when one serves it, so its page follows; any other here
// (refreshGraph refuses a run another process's viewer serves).
async function refreshOut(out: string) {
  const mine = [...viewers.values()].find(v => v.server.out === out);
  return mine
    ? { ...(await mine.server.refresh()), viewer: true }
    : await refreshSaved(out);
}

// query_graph refresh: true — graph files edited since the graph at `out` was written are refreshed first.
async function refreshEdited(out: string) {
  const graph = join(out, 'data.json');
  const D = await dataLoader({ out })();
  if (!editedSince(D, graph).length) return;
  await refreshOut(out);
}

async function queryGraph(args: ToolArgs): Promise<ToolResult> {
  if (args.result !== undefined) {
    if (args.path !== undefined)
      throw new Error('query_graph takes result or path');
    const out = resultDir(args.result);
    if (args.refresh === true) await refreshEdited(out);
    if (saved?.out !== out) saved = { out, read: dataLoader({ out }) };
    const D = await saved.read();
    return answer(
      await withProvider(
        withStaleness(
          runQuery(D, args.query),
          editedSince(D, join(out, 'data.json'))
        )
      ),
      args.format
    );
  }
  const viewer = viewerFor(args.path);
  if (args.refresh === true) await refreshEdited(viewer.server.out);
  return answer(await viewer.server.query(args.query), args.format);
}

// A wiki chat_id is the channel's; the server knows the viewer's own message id.
function chatOf(chatId: unknown) {
  const chat = typeof chatId === 'string' ? chats.get(chatId) : undefined;
  if (!chat)
    throw new Error(
      `unknown chat_id ${String(chatId)}: pass the chat_id attribute of the <channel> message`
    );
  return chat;
}

const pretty = (r: WikiAnswer) =>
  typeof r.markdown === 'string'
    ? `# ${r.slug}\n\n${r.markdown}`
    : JSON.stringify(r, null, 2);
const WIKI: Record<string, (a: ToolArgs) => Promise<WikiAnswer>> = {
  wiki_read: a =>
    viewerFor(a.path).server.wiki({
      action: 'read',
      slug: a.slug,
      file: a.file,
    }),
  wiki_note: a =>
    viewerFor(a.path).server.wiki({
      action: 'note',
      slug: a.slug,
      text: a.text,
    }),
  wiki_promote: a => {
    const chat = a.chat_id ? chatOf(a.chat_id) : null;
    const server = chat ? chat.viewer.server : viewerFor(a.path).server;
    return server.wiki({
      action: 'promote',
      slug: a.slug,
      text: a.text,
      chat_id: chat?.id,
    });
  },
  wiki_lint: a => viewerFor(a.path).server.wiki({ action: 'lint' }),
};
const wikiCall =
  (call: (a: ToolArgs) => Promise<WikiAnswer>): ToolCall =>
  async args =>
    pretty(await call(args));

async function refreshGraph(args: ToolArgs) {
  if (args.result !== undefined) {
    const out = resultDir(args.result);
    const r = await refreshOut(out);
    if ('viewer' in r)
      return `graph refreshed in ${(r.ms / 1000).toFixed(1)} s`;
    return `graph refreshed in ${(r.ms / 1000).toFixed(1)} s; rebuilt chunks: ${JSON.stringify(r.chunks ?? [])}; diff_graph({"result":${JSON.stringify(out)}}) shows what changed`;
  }
  const r = await viewerFor(args.path).server.refresh();
  return `graph refreshed in ${(r.ms / 1000).toFixed(1)} s`;
}

async function buildSaved(args: ToolArgs) {
  const r = await buildGraph(pathArg(args.path, 'path'), {
    lsp: lspMode(args.lsp),
    whole: Boolean(args.whole),
  });
  return `built ${r.out} in ${(r.ms / 1000).toFixed(1)} s: ${r.files} files, ${r.imports} imports, ${r.calls} calls, LSP ${r.lsp}. Next: query_graph({"result":${JSON.stringify(r.out)},"query":"search <name>"})`;
}

async function linkGraph(args: ToolArgs) {
  const out = resultDir(args.result);
  if (args.action === 'list') return JSON.stringify(readProposed(out));
  try {
    if (args.action === 'add')
      return JSON.stringify(proposeLink(out, args.link));
    const id = pathArg(args.id, 'id');
    if (args.action === 'remove') {
      removeLink(out, id);
      return `removed ${id}`;
    }
    const reviewer = typeof args.reviewer === 'string' ? args.reviewer : '';
    return JSON.stringify(reviewLink(out, id, reviewer));
  } catch (error) {
    if (error instanceof LinkRefused)
      return { ...text(`refused: ${error.message}`), isError: true };
    throw error;
  }
}

async function diffGraph(args: ToolArgs) {
  const after = args.after ?? args.result;
  if (after === undefined)
    throw new Error(
      'diff_graph needs result (its last refresh against now) or before and after'
    );
  const now = openSaved(pathArg(after, 'after'));
  const then =
    args.before === undefined
      ? previousGraph(resultDir(after))
      : openSaved(pathArg(args.before, 'before'));
  const d = now.diffFrom(then);
  return args.format === 'json' ? JSON.stringify(d) : formatDiff(d);
}

// check_graph: the CLI's `check`; a check that cannot run is an error result, a failing one too (isError).
async function checkGraph(args: ToolArgs): Promise<ToolResult> {
  const r = await runCheck({
    graph: args.result === undefined ? '.' : pathArg(args.result, 'result'),
    ...(args.rules !== undefined && { rules: pathArg(args.rules, 'rules') }),
    ...(args.baseline !== undefined && {
      baseline: pathArg(args.baseline, 'baseline'),
    }),
    ...(args.since !== undefined && { since: pathArg(args.since, 'since') }),
    writeBaseline: args.write_baseline === true,
    format: (args.format ?? 'text') as CheckFormat,
  });
  return { ...text(r.output), ...(r.exitCode ? { isError: true } : {}) };
}

const CALLS: Partial<Record<string, ToolCall>> = {
  open_graph: openGraph,
  build_graph: buildSaved,
  reply,
  refresh_graph: refreshGraph,
  query_graph: queryGraph,
  diff_graph: diffGraph,
  check_graph: checkGraph,
  link_graph: linkGraph,
  ...Object.fromEntries(
    Object.entries(WIKI).map(([name, call]) => [name, wikiCall(call)])
  ),
};

const isResult = (r: ToolReturn): r is ToolResult =>
  typeof r === 'object' && Boolean(r.content);

// The engine's MCP server as a child (`octocode __graph mcp`, the graph tools enabled as `octocode-graph mcp` always
// did): its tools are listed beside ours and its answers returned as they come. Without an engine, only the
// saved-graph tools are served, and the reason is logged.
/** A reply of the engine's MCP server, read only for its id, result and error. */
interface EngineReply {
  id?: number;
  result?: unknown;
  error?: { code?: number; message?: string };
}

const native = (() => {
  let child: ChildProcess | null = null;
  let ready: Promise<{ name: string }[]> | null = null;
  let seq = 0;
  const waiting = new Map<number, (m: EngineReply) => void>();
  const send = (method: string, params?: unknown, id?: number) =>
    child?.stdin?.write(
      `${JSON.stringify({ jsonrpc: '2.0', ...(id !== undefined && { id }), method, ...(params !== undefined && { params }) })}\n`
    );
  const ask = (method: string, params?: unknown) =>
    new Promise<EngineReply>((ok, fail) => {
      if (!child) return fail(new Error('no engine'));
      const id = ++seq;
      waiting.set(id, ok);
      send(method, params, id);
    });
  return {
    start(protocolVersion: string) {
      if (ready) return;
      ready = (async () => {
        const engine = await resolveNativeBin();
        if ('error' in engine) {
          log(`engine tools unavailable: ${engine.error}`);
          return [];
        }
        // Installing the graph server opts into the engine's graph tools; workspace restrictions and the host's
        // enabled and disabled tool lists still apply to every request.
        child = spawn(engine.bin, ['__graph', 'mcp'], {
          stdio: ['pipe', 'pipe', 'inherit'],
          env: { OCTOCODE_BETA: 'true', ...process.env },
        });
        child.once('exit', () => {
          child = null;
        });
        let buf = '';
        child.stdout?.on('data', d => {
          buf += d;
          for (let i; (i = buf.indexOf('\n')) >= 0;) {
            const line = buf.slice(0, i);
            buf = buf.slice(i + 1);
            if (!line.trim()) continue;
            const m = JSON.parse(line) as EngineReply;
            if (typeof m.id === 'number') waiting.get(m.id)?.(m);
          }
        });
        await ask('initialize', {
          protocolVersion,
          capabilities: {},
          clientInfo: { name: 'octocode-graph', version: packageVersion() },
        });
        send('notifications/initialized');
        const list = await ask('tools/list', {});
        return (
          isRecord(list.result) && Array.isArray(list.result.tools)
            ? list.result.tools
            : []
        ) as { name: string }[];
      })().catch((error: unknown) => {
        log(`engine tools unavailable: ${errorMessage(error)}`);
        return [];
      });
    },
    tools: async () => (ready ? await ready : []),
    has: async (name: string) =>
      (ready ? await ready : []).some(tool => tool.name === name),
    async call(params: Record<string, unknown>) {
      const m = await ask('tools/call', params);
      if (m.error)
        throw Object.assign(new Error(String(m.error.message)), m.error);
      return m.result;
    },
    stop: () => child?.kill(),
  };
})();

const METHODS: Partial<Record<string, MethodHandler>> = {
  initialize: p => {
    const protocolVersion =
      typeof p.protocolVersion === 'string' &&
      PROTOCOLS.includes(p.protocolVersion)
        ? p.protocolVersion
        : PROTOCOLS[0];
    native.start(protocolVersion);
    return {
      protocolVersion,
      capabilities:
        mode === 'channel'
          ? { experimental: { 'claude/channel': {} }, tools: {} }
          : { tools: {} },
      serverInfo: {
        name: mode === 'channel' ? SOURCE : 'octocode-graph',
        version: packageVersion(),
      },
      instructions:
        mode === 'channel'
          ? `${SAVED_INSTRUCTIONS} ${INSTRUCTIONS}`
          : SAVED_INSTRUCTIONS,
    };
  },
  ping: () => ({}),
  'tools/list': async () => ({
    tools: [...TOOLS, ...(await native.tools())],
  }),
  'tools/call': async (p): Promise<ToolResult> => {
    const name = String(p.name);
    // The engine's own tools answer unchanged.
    if (await native.has(name)) return native.call(p) as Promise<ToolResult>;
    const call = Object.hasOwn(CALLS, name) ? CALLS[name] : undefined;
    if (!call)
      throw Object.assign(new Error(`unknown tool ${String(p.name)}`), {
        code: -32602,
      });
    try {
      const r = await call(checkArgs(name, fieldsOf(p.arguments)));
      return isResult(r) ? r : text(r);
    } catch (error) {
      const detail = isRecord(error) ? error.detail : undefined;
      return {
        ...text(
          detail
            ? `${errorMessage(error)}\n${JSON.stringify(detail)}`
            : errorMessage(error)
        ),
        isError: true,
      };
    }
  },
};

const isType = (type: ArgumentSchema['type'], value: unknown) =>
  type === 'array'
    ? Array.isArray(value)
    : type === 'object'
      ? typeof value === 'object' && value !== null && !Array.isArray(value)
      : typeof value === type;

// Arguments are checked against the tool's own schema, so a wrong name or value is refused with what is accepted.
function checkArgs(name: string, args: ToolArgs): ToolArgs {
  const schema = TOOLS.find(tool => tool.name === name)?.inputSchema;
  if (!schema) return args;
  const accepted = Object.keys(schema.properties);
  const problems: string[] = [];
  for (const [key, value] of Object.entries(args)) {
    const field = schema.properties[key];
    if (!field) {
      const near = nearest(key, accepted).best;
      problems.push(
        `unknown field ${key}${near.length ? ` (did you mean ${near.join(' or ')}?)` : ''}`
      );
    } else if (!isType(field.type, value))
      problems.push(`${key} must be a ${field.type}`);
    else if (field.enum && !field.enum.includes(String(value)))
      problems.push(`${key} must be one of ${field.enum.join(', ')}`);
  }
  for (const key of schema.required ?? [])
    if (args[key] === undefined) problems.push(`${key} is required`);
  if (problems.length)
    throw new Error(
      `${name}: ${problems.join('; ')}. Fields: ${accepted.join(', ')}`
    );
  return args;
}

// The JSON-RPC error code a handler failure carries (unknown tool: invalid params), else an internal error.
const errorCode = (error: unknown) =>
  isRecord(error) && typeof error.code === 'number' ? error.code : -32603;

async function request(msg: JsonRpcRequest) {
  const handler = Object.hasOwn(METHODS, msg.method)
    ? METHODS[msg.method]
    : undefined;
  if (!handler) {
    write({
      id: msg.id,
      error: { code: -32601, message: `method not found: ${msg.method}` },
    });
    return;
  }
  try {
    write({ id: msg.id, result: await handler(fieldsOf(msg.params)) });
  } catch (error) {
    write({
      id: msg.id,
      error: { code: errorCode(error), message: errorMessage(error) },
    });
  }
}

const isRequest = (msg: unknown): msg is JsonRpcRequest =>
  isRecord(msg) &&
  typeof msg.method === 'string' &&
  msg.id !== undefined &&
  msg.id !== null;

function onLine(line: string) {
  if (!line.trim()) return;
  let msg: unknown;
  try {
    msg = JSON.parse(line);
  } catch {
    write({ id: null, error: { code: -32700, message: 'parse error' } });
    return;
  }
  // Requests carry an id; notifications (initialized, cancelled) and responses need no answer.
  if (isRequest(msg)) void request(msg);
}

async function shutdown() {
  native.stop();
  await Promise.allSettled([...viewers.values()].map(v => v.server.close()));
  process.exit(0);
}

/** Serve MCP on stdin and stdout in `m` mode until stdin closes. */
export function serve(m: Mode) {
  mode = m;
  TOOLS = toolsFor(m);
  createInterface({ input: process.stdin, crlfDelay: Infinity })
    .on('line', onLine)
    .on('close', () => void shutdown());
  process.once('SIGTERM', () => void shutdown());
  process.once('SIGINT', () => void shutdown());
}
