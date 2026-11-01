#!/usr/bin/env node
// Claude Code channel (research preview) for the graph viewer: an MCP server over stdio that opens graphs, pushes
// each viewer chat message into the session as a `notifications/claude/channel` event, and gives Claude tools to
// answer (reply, pointing at graph nodes), query the graph, read and write the wiki, and refresh after edits. Zero dependencies: newline-delimited JSON-RPC 2.0; stdout carries only JSON-RPC,
// every log goes to stderr. Contract: https://code.claude.com/docs/en/channels-reference.md
import { createInterface } from 'node:readline';
import { statSync } from 'node:fs';
import { resolve } from 'node:path';
import { defaults } from './pipeline.mjs';
import { HINT, formatResult } from './query.mjs';
import { freshOut, openBrowser, startServer } from './serve.mjs';

// Claude Code does not register a channel that negotiates MCP revision 2026-07-28 (it cannot carry channel
// messages), so this server answers with an earlier revision: the client's own when it is one of these.
const PROTOCOLS = ['2025-06-18', '2025-03-26', '2024-11-05'];
const SOURCE = 'graph-view';

const INSTRUCTIONS = [
  `Messages from the octocode graph viewer arrive as <channel source="${SOURCE}" chat_id="..." file="..." line="...">text</channel>:`,
  'a person clicked a node in the live code graph and asked about it. file and line name the selected node (repository-relative);',
  'metrics, when present, are the node\'s graph metrics.',
  'Before answering, read the wiki page of the node\'s community (wiki_read with file) and the code.',
  'Prove structural claims with query_graph (impact, deps, dependents, path, cycle, community, callers, callees, top, find) and LSP or source reads; a graph label alone is a lead.',
  'Answer with the reply tool, passing the chat_id from the tag and nodes (file paths) to point at on the graph; mention files as path:line so the viewer links them.',
  'Promote an answer worth keeping with wiki_promote (chat_id), or record notes with wiki_note.',
  'After you edit code, call refresh_graph so the viewer and the wiki show the updated graph.',
  'Call open_graph with a repository path to show it in the viewer (it opens the browser and returns the URL).',
].join(' ');

const TOOLS = [
  {
    name: 'open_graph',
    description: 'Build and show the live code graph of a repository directory in the browser viewer, or reopen a finished result. Reuses the viewer already open for that path. Returns the viewer URL.',
    inputSchema: {
      type: 'object',
      properties: {
        path: { type: 'string', description: 'Repository directory to graph (absolute, or relative to the session working directory).' },
        whole: { type: 'boolean', description: 'Ingest one snapshot instead of chunking at package manifests (cross-package linkage over early feedback).' },
        lsp: { type: 'string', enum: ['on', 'off'], description: 'LSP call cross-check (default on).' },
        result: { type: 'string', description: 'An existing result directory to reopen without rebuilding (replay); path is then optional.' },
      },
    },
  },
  {
    name: 'reply',
    description: 'Send an answer to a graph viewer chat message; it appears in the viewer\'s chat drawer.',
    inputSchema: {
      type: 'object',
      properties: {
        chat_id: { type: 'string', description: 'The chat_id attribute of the <channel> message being answered.' },
        text: { type: 'string', description: 'The answer. `code` spans render as code; path:line mentions of graph nodes become links.' },
        nodes: { type: 'array', items: { type: 'string' }, description: 'Files the answer is about, highlighted and framed on the graph: repository-relative paths, unique suffixes (pipeline.mjs) or node ids. An unknown or ambiguous one refuses the reply and lists candidates.' },
      },
      required: ['chat_id', 'text'],
    },
  },
  {
    name: 'query_graph',
    description: `Answer a structural question over the whole stitched, weighted file graph and highlight the answer in the viewer. Returns every row with its path:line. Query forms: ${HINT} (--tests includes test-only edges). Files are repository-relative paths, unique suffixes or node ids; an ambiguous one returns candidates.`,
    inputSchema: {
      type: 'object',
      properties: {
        query: { type: 'string', description: 'For example: impact src/pipeline.mjs · path a.ts b.ts · cycle x.rs · top pagerank 10 · callers parseArgs' },
        path: { type: 'string', description: 'Which open graph (its repository path or result directory); default: the most recently used viewer.' },
      },
      required: ['query'],
    },
  },
  {
    name: 'wiki_read',
    description: 'Read a wiki page of the open graph: by slug, or the community page that holds a file.',
    inputSchema: { type: 'object', properties: { slug: { type: 'string', description: 'Page slug.' }, file: { type: 'string', description: 'A file (path, suffix or id): reads its community page.' }, path: { type: 'string', description: 'Which open graph; default: the most recent.' } } },
  },
  {
    name: 'wiki_note',
    description: 'Replace the agent-notes section of a wiki page with this text (markdown); the viewer shows the updated page.',
    inputSchema: { type: 'object', properties: { slug: { type: 'string' }, text: { type: 'string', description: 'The notes, markdown.' }, path: { type: 'string', description: 'Which open graph; default: the most recent.' } }, required: ['slug', 'text'] },
  },
  {
    name: 'wiki_promote',
    description: 'Append an answer to a wiki page\'s notes with the date and its source: a viewer chat answer by chat_id (its latest agent reply), or text. Without slug, the page of the asked-about file.',
    inputSchema: { type: 'object', properties: { slug: { type: 'string' }, chat_id: { type: 'string', description: 'The chat_id of the <channel> message whose reply to promote.' }, text: { type: 'string', description: 'Text to promote instead of a chat reply.' }, path: { type: 'string', description: 'Which open graph; default: the most recent.' } } },
  },
  {
    name: 'wiki_lint',
    description: 'Check the wiki of the open graph against the graph and the files: stale, missing or contradicted pages.',
    inputSchema: { type: 'object', properties: { path: { type: 'string', description: 'Which open graph; default: the most recent.' } } },
  },
  {
    name: 'refresh_graph',
    description: 'Rebuild the open graph from the files on disk after code changes, so the viewer updates.',
    inputSchema: {
      type: 'object',
      properties: { path: { type: 'string', description: 'Which open graph to refresh (its repository path); default: the most recently used viewer.' } },
    },
  },
];

const log = (text) => process.stderr.write(`graph-view channel: ${text}\n`);
const write = (message) => process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', ...message })}\n`);
const text = (value) => ({ content: [{ type: 'text', text: typeof value === 'string' ? value : JSON.stringify(value) }] });

// Open viewers by key (repository root or replayed result directory); channel chat_ids map back to their viewer.
const viewers = new Map();
const chats = new Map();
let lastViewer = null;
let nextViewer = 1;

function metricLine(metrics) {
  const parts = Object.entries(metrics ?? {}).map(([k, v]) => `${k}=${typeof v === 'number' && !Number.isInteger(v) ? v.toPrecision(4) : v}`);
  return parts.length ? `\n\n(selected node metrics: ${parts.join(', ')})` : '';
}

// The viewer's transport: only messages from its token-authenticated browser socket reach this function.
function deliverer(viewer) {
  return (m) => {
    const chatId = `${viewer.n}_${m.id}`;
    chats.set(chatId, { viewer, id: m.id });
    lastViewer = viewer;
    const meta = { chat_id: chatId, ...(m.context ? { file: m.context.file, line: String(m.context.line) } : {}) };
    write({ method: 'notifications/claude/channel', params: { content: `${m.text}${metricLine(m.context?.metrics)}`, meta } });
  };
}

function viewerFor(path) {
  if (path) {
    const v = viewers.get(resolve(path)) ?? [...viewers.values()].find((x) => x.server.root() === resolve(path));
    if (!v) throw new Error(`no graph is open for ${resolve(path)}; call open_graph first`);
    return v;
  }
  if (!lastViewer) throw new Error('no graph is open; call open_graph first');
  return lastViewer;
}

function directory(path, what) {
  const p = resolve(path);
  if (!statSync(p, { throwIfNoEntry: false })?.isDirectory()) throw new Error(`${what} ${p} is not a directory`);
  return p;
}

async function openGraph(args) {
  if (!args.path && !args.result) throw new Error('open_graph needs path (a repository directory) or result (a result directory)');
  const replay = Boolean(args.result);
  const root = replay ? null : directory(args.path, 'path');
  const key = replay ? directory(args.result, 'result') : root;
  const open = process.env.GRAPH_VIEW_NO_OPEN !== '1';
  const known = viewers.get(key);
  if (known) {
    if (open) openBrowser(known.server.url);
    lastViewer = known;
    return { url: known.server.url, out: known.server.out, reused: true };
  }
  const viewer = { n: nextViewer++, key, server: null };
  const opts = { ...defaults(), whole: Boolean(args.whole), lsp: args.lsp === 'off' ? 'off' : 'on' };
  viewer.server = await startServer({ root, out: replay ? key : freshOut(root), opts, replay, open, transport: deliverer(viewer) });
  viewers.set(key, viewer);
  lastViewer = viewer;
  void viewer.server.finished.then((r) => log(`${key}: build ${r.ok ? 'done' : `failed: ${r.error}`}`));
  log(`viewer for ${key}: ${viewer.server.url}`);
  return { url: viewer.server.url, out: viewer.server.out, building: !replay };
}

async function reply(args) {
  const chat = chats.get(args.chat_id);
  if (!chat) throw new Error(`unknown chat_id ${args.chat_id}: pass the chat_id attribute of the <channel> message`);
  lastViewer = chat.viewer;
  const r = await chat.viewer.server.reply({ text: args.text, chat_id: chat.id, nodes: args.nodes });
  return r.nodes ? `sent; pointing at ${r.nodes.length} node${r.nodes.length > 1 ? 's' : ''}: ${r.nodes.join(', ')}` : 'sent';
}

async function queryGraph(args) {
  const r = await viewerFor(args.path).server.query(args.query);
  return { ...text(formatResult(r)), ...(r.ok ? {} : { isError: true }) };
}

// A wiki chat_id is the channel's; the server knows the viewer's own message id.
function chatOf(chatId) {
  const chat = chats.get(chatId);
  if (!chat) throw new Error(`unknown chat_id ${chatId}: pass the chat_id attribute of the <channel> message`);
  return chat;
}

const pretty = (r) => (typeof r.markdown === 'string' ? `# ${r.slug}\n\n${r.markdown}` : JSON.stringify(r, null, 2));
const WIKI = {
  wiki_read: (a) => viewerFor(a.path).server.wiki({ action: 'read', slug: a.slug, file: a.file }),
  wiki_note: (a) => viewerFor(a.path).server.wiki({ action: 'note', slug: a.slug, text: a.text }),
  wiki_promote: (a) => {
    const chat = a.chat_id ? chatOf(a.chat_id) : null;
    const server = chat ? chat.viewer.server : viewerFor(a.path).server;
    return server.wiki({ action: 'promote', slug: a.slug, text: a.text, chat_id: chat?.id });
  },
  wiki_lint: (a) => viewerFor(a.path).server.wiki({ action: 'lint' }),
};
const wikiCall = (name) => async (args) => pretty(await WIKI[name](args));

async function refreshGraph(args) {
  const r = await viewerFor(args.path).server.refresh();
  return `graph refreshed in ${(r.ms / 1000).toFixed(1)} s`;
}

const CALLS = {
  open_graph: openGraph, reply, refresh_graph: refreshGraph, query_graph: queryGraph,
  ...Object.fromEntries(Object.keys(WIKI).map((name) => [name, wikiCall(name)])),
};

const METHODS = {
  initialize: (p) => ({
    protocolVersion: PROTOCOLS.includes(p.protocolVersion) ? p.protocolVersion : PROTOCOLS[0],
    capabilities: { experimental: { 'claude/channel': {} }, tools: {} },
    serverInfo: { name: SOURCE, version: '1.0.0' },
    instructions: INSTRUCTIONS,
  }),
  ping: () => ({}),
  'tools/list': () => ({ tools: TOOLS }),
  'tools/call': async (p) => {
    const call = CALLS[p.name];
    if (!call) throw Object.assign(new Error(`unknown tool ${p.name}`), { code: -32602 });
    try {
      const r = await call(p.arguments ?? {});
      return r?.content ? r : text(r);
    } catch (error) {
      return { ...text(error.detail ? `${error.message}\n${JSON.stringify(error.detail)}` : error.message), isError: true };
    }
  },
};

async function request(msg) {
  const handler = METHODS[msg.method];
  if (!handler) {
    write({ id: msg.id, error: { code: -32601, message: `method not found: ${msg.method}` } });
    return;
  }
  try {
    write({ id: msg.id, result: await handler(msg.params ?? {}) });
  } catch (error) {
    write({ id: msg.id, error: { code: error.code ?? -32603, message: error.message } });
  }
}

function onLine(line) {
  if (!line.trim()) return;
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    write({ id: null, error: { code: -32700, message: 'parse error' } });
    return;
  }
  // Requests carry an id; notifications (initialized, cancelled) and responses need no answer.
  if (typeof msg?.method === 'string' && msg.id !== undefined && msg.id !== null) void request(msg);
}

async function shutdown() {
  await Promise.allSettled([...viewers.values()].map((v) => v.server.close()));
  process.exit(0);
}

createInterface({ input: process.stdin, crlfDelay: Infinity }).on('line', onLine).on('close', () => void shutdown());
process.once('SIGTERM', () => void shutdown());
process.once('SIGINT', () => void shutdown());
