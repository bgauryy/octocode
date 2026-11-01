#!/usr/bin/env node
// Answers a viewer chat from the shell (inbox mode): reads <result-dir>/server.json written by serve.mjs and posts
// the reply (optionally pointing at graph nodes), then optionally asks the server to refresh the graph. The same
// server answers graph queries and reads or writes the wiki, so the viewer highlights and shows each answer.
import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const USAGE = `usage: reply.mjs <result-dir> "text" [--chat <chat_id>] [--nodes a,b,c] [--refresh]
       reply.mjs <result-dir> - [--chat <chat_id>] [--nodes a,b,c] [--refresh]   (text from stdin)
       reply.mjs <result-dir> --refresh                                          (refresh only)
       reply.mjs query <result-dir> "<query>"                  (impact, deps, dependents, path, cycle, community,
                                                               callers, callees, top, find; references/query.md)
       reply.mjs wiki <result-dir> read <slug> | read --file <file>
       reply.mjs wiki <result-dir> note <slug> "text"|-
       reply.mjs wiki <result-dir> promote [<slug>] (--chat <chat_id> | "text"|-) [--file <file>]
       reply.mjs wiki <result-dir> lint
  A reply goes to --chat, else the latest unanswered message; --nodes (paths, unique suffixes or node ids) light up
  and frame on the graph. --refresh rebuilds the graph after code changes. Prints one JSON line; exit 0 ok,
  1 refused or failed (a query error or ambiguity: the JSON lists candidates), 2 usage, 3 no running server.`;

const fail = (code, message) => {
  process.stderr.write(`reply.mjs: ${message}\n`);
  process.exit(code);
};

// Removes `--name value` from args and returns the value (undefined when absent).
function option(args, name) {
  const at = args.indexOf(name);
  if (at < 0) return undefined;
  const value = args.splice(at, 2)[1];
  if (value === undefined || value.startsWith('--')) fail(2, `${name} needs a value`);
  return value;
}

function parseReply(argv) {
  const args = [...argv];
  const refreshOnly = args.includes('--refresh');
  const chatId = option(args, '--chat');
  const nodes = option(args, '--nodes')?.split(',').map((s) => s.trim()).filter(Boolean);
  const rest = args.filter((a) => a !== '--refresh');
  if (rest.length < 1 || rest.length > 2 || (rest.length === 1 && !refreshOnly)) fail(2, `\n${USAGE}`);
  return { dir: resolve(rest[0]), text: rest[1], chatId, nodes, refresh: refreshOnly };
}

function serverInfo(dir) {
  let info;
  try {
    info = JSON.parse(readFileSync(join(dir, 'server.json'), 'utf8'));
  } catch {
    return fail(3, `no server.json in ${dir}: start serve.mjs for this result first`);
  }
  try {
    process.kill(info.pid, 0);
  } catch (error) {
    if (error.code === 'ESRCH') fail(3, `the server (pid ${info.pid}) for ${dir} is not running: server.json is stale`);
  }
  return info;
}

async function post(info, path, body) {
  const url = new URL(path, info.url);
  url.search = new URLSearchParams({ t: info.token }).toString();
  let res;
  try {
    res = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });
  } catch (error) {
    return fail(3, `cannot reach ${url.origin}: ${error.cause?.code ?? error.message}`);
  }
  const answer = await res.json().catch(() => ({ ok: false, error: `HTTP ${res.status}` }));
  if (!res.ok) {
    const detail = answer.problems ?? answer.candidates;
    fail(1, `${path} refused (${res.status}): ${answer.error ?? 'no detail'}${detail ? `\n${JSON.stringify(detail)}` : ''}`);
  }
  return answer;
}

async function readStdin() {
  const parts = [];
  for await (const chunk of process.stdin) parts.push(chunk);
  return Buffer.concat(parts).toString('utf8');
}

const textArg = async (t) => (t === '-' ? readStdin() : t);
const print = (value) => process.stdout.write(`${JSON.stringify(value)}\n`);

async function replyMode(argv) {
  const o = parseReply(argv);
  const info = serverInfo(o.dir);
  const text = await textArg(o.text);
  const result = {};
  if (text !== undefined) {
    if (!text.trim()) fail(2, 'the reply text is empty');
    result.reply = await post(info, '/reply', { text, ...(o.chatId ? { chat_id: o.chatId } : {}), ...(o.nodes?.length ? { nodes: o.nodes } : {}) });
  }
  if (o.refresh) result.refresh = await post(info, '/refresh', {});
  print(result);
}

async function queryMode(argv) {
  const [dir, ...words] = argv;
  if (!dir || !words.length) fail(2, `\n${USAGE}`);
  const r = await post(serverInfo(resolve(dir)), '/query', { query: words.join(' ') });
  print(r);
  if (!r.ok) fail(1, r.error ?? 'query failed');
}

// wiki <dir> read|note|promote|lint ...: one body for POST /wiki.
async function wikiBody(action, args) {
  const chatId = option(args, '--chat');
  const file = option(args, '--file');
  const [slug, text] = args;
  if (action === 'read' && (slug || file)) return { action, slug, file };
  if (action === 'note' && slug && text) return { action, slug, text: await textArg(text) };
  if (action === 'promote' && (chatId || text)) return { action, slug, file, chat_id: chatId, text: text === undefined ? undefined : await textArg(text) };
  if (action === 'lint' && !args.length) return { action };
  return fail(2, `\n${USAGE}`);
}

async function wikiMode(argv) {
  const [dir, action, ...args] = argv;
  if (!dir || !action) fail(2, `\n${USAGE}`);
  const body = await wikiBody(action, args);
  print(await post(serverInfo(resolve(dir)), '/wiki', body));
}

const MODES = { query: queryMode, wiki: wikiMode };

async function main(argv) {
  if (argv.includes('--help')) {
    process.stdout.write(`${USAGE}\n`);
    return;
  }
  const mode = MODES[argv[0]];
  await (mode ? mode(argv.slice(1)) : replyMode(argv));
}

void main(process.argv.slice(2));
