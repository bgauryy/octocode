#!/usr/bin/env node
// Answers a viewer chat from the shell (inbox mode) through the viewer serving <result-dir> (lib/served.ts): posts
// the reply, optionally pointing at graph nodes. The same server answers graph queries and reads or writes the wiki,
// so the viewer highlights and shows each answer. `octocode-graph refresh` refreshes a served run the same way.
import { resolve } from 'node:path';
import {
  postServer,
  ServedError,
  serverOf,
  type ServerInfo,
} from '../lib/served.ts';
import { graphCommand } from '../invocation.ts';

const cli = graphCommand();

const USAGE = `usage: ${cli} reply <result-dir> "text" [--chat <chat_id>] [--nodes a,b,c]
       ${cli} reply <result-dir> - [--chat <chat_id>] [--nodes a,b,c]            (text from stdin)
       ${cli} reply query <result-dir> "<query>"                  (saved queries: references/query.md)
       ${cli} reply wiki <result-dir> read <slug> | read --file <file>
       ${cli} reply wiki <result-dir> note <slug> "text"|-
       ${cli} reply wiki <result-dir> promote [<slug>] (--chat <chat_id> | "text"|-) [--file <file>]
       ${cli} reply wiki <result-dir> lint
  A reply goes to --chat, else the latest unanswered message; --nodes (paths, unique suffixes or node ids) light up
  and frame on the graph. After code changes: ${cli} refresh <result-dir>. Prints one JSON line; exit 0 ok,
  1 refused or failed (a query error or ambiguity: the JSON lists candidates), 2 usage, 3 no running server.`;

const fail = (code: number, message: string): never => {
  process.stderr.write(`octocode-graph reply: ${message}\n`);
  process.exit(code);
};

// Removes `--name value` from args and returns the value (undefined when absent).
function option(args: string[], name: string) {
  const at = args.indexOf(name);
  if (at < 0) return undefined;
  const value = args.splice(at, 2).at(1);
  if (value === undefined || value.startsWith('--'))
    return fail(2, `${name} needs a value`);
  return value;
}

function parseReply(argv: string[]) {
  const args = [...argv];
  const chatId = option(args, '--chat');
  const nodes = option(args, '--nodes')
    ?.split(',')
    .map(s => s.trim())
    .filter(Boolean);
  if (args.length !== 2) fail(2, `\n${USAGE}`);
  return { dir: resolve(args[0]), text: args[1], chatId, nodes };
}

function serverInfo(dir: string) {
  return (
    serverOf(dir) ??
    fail(
      3,
      `no viewer serves ${dir} (server.json missing or stale): start ${cli} serve for it first`
    )
  );
}

async function post(info: ServerInfo, path: string, body: unknown) {
  try {
    return await postServer(info, path, body);
  } catch (error) {
    if (error instanceof ServedError)
      return fail(error.exitCode, error.message);
    throw error;
  }
}

async function readStdin() {
  const parts: Buffer[] = [];
  for await (const chunk of process.stdin) parts.push(chunk);
  return Buffer.concat(parts).toString('utf8');
}

const textArg = async (t: string | undefined) => (t === '-' ? readStdin() : t);
const print = (value: unknown) =>
  process.stdout.write(`${JSON.stringify(value)}\n`);

async function replyMode(argv: string[]) {
  const o = parseReply(argv);
  const info = serverInfo(o.dir);
  const text = await textArg(o.text);
  if (!text?.trim()) fail(2, 'the reply text is empty');
  print({
    reply: await post(info, '/reply', {
      text,
      ...(o.chatId ? { chat_id: o.chatId } : {}),
      ...(o.nodes?.length ? { nodes: o.nodes } : {}),
    }),
  });
}

async function queryMode(argv: string[]) {
  const [dir, ...words] = argv;
  if (!dir || !words.length) fail(2, `\n${USAGE}`);
  const r = await post(serverInfo(resolve(dir)), '/query', {
    query: words.join(' '),
  });
  print(r);
  if (!r.ok) fail(1, r.error ?? 'query failed');
}

// wiki <dir> read|note|promote|lint ...: one body for POST /wiki.
async function wikiBody(action: string, args: string[]) {
  const chatId = option(args, '--chat');
  const file = option(args, '--file');
  const [slug, text] = args;
  if (action === 'read' && (slug || file)) return { action, slug, file };
  if (action === 'note' && slug && text)
    return { action, slug, text: await textArg(text) };
  if (action === 'promote' && (chatId || text))
    return {
      action,
      slug,
      file,
      chat_id: chatId,
      text: text === undefined ? undefined : await textArg(text),
    };
  if (action === 'lint' && !args.length) return { action };
  return fail(2, `\n${USAGE}`);
}

async function wikiMode(argv: string[]) {
  const [dir, action, ...args] = argv;
  if (!dir || !action) fail(2, `\n${USAGE}`);
  const body = await wikiBody(action, args);
  print(await post(serverInfo(resolve(dir)), '/wiki', body));
}

const MODES: Partial<Record<string, (argv: string[]) => Promise<void>>> = {
  query: queryMode,
  wiki: wikiMode,
};

async function main(argv: string[]) {
  if (argv.includes('--help')) {
    process.stdout.write(`${USAGE}\n`);
    return;
  }
  const mode = Object.hasOwn(MODES, argv[0]) ? MODES[argv[0]] : undefined;
  await (mode ? mode(argv.slice(1)) : replyMode(argv));
}

void main(process.argv.slice(2));
