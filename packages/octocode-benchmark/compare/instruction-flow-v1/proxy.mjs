import { appendFileSync, readFileSync, realpathSync } from 'node:fs';
import { isAbsolute, relative, resolve } from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, ListToolsRequestSchema } from '@modelcontextprotocol/sdk/types.js';

// The proxy restricts access; canonical schemas and execution stay downstream.
export function admit(name, args, allowed, root) {
  if (!allowed.includes(name)) throw new Error('Tool outside trial scope');
  const rows = args.queries ?? [args];
  if (!Array.isArray(rows) || !rows.length) throw new Error('Invalid rows');
  let cells = 0;
  for (const row of rows) {
    if (name === 'clasify') {
      if (!Array.isArray(row.resources) || !Array.isArray(row.questions)) throw new Error('Matrix required');
      cells += row.resources.length * row.questions.length;
      for (const resource of row.resources) {
        const context = resource.context;
        if (Object.hasOwn(context ?? {}, 'value')) continue;
        if (context?.tool === 'clasify' || context?.query?.queries) throw new Error('Nested matrix prohibited');
        admit(context?.tool, context?.query ?? {}, allowed.filter(tool => tool !== 'clasify'), root);
      }
    } else {
      if (!isAbsolute(row.path ?? '')) throw new Error('Absolute fixture path required');
      const rel = relative(realpathSync(root), realpathSync(row.path));
      if (rel === '..' || rel.startsWith('../') || isAbsolute(rel)) throw new Error('Path outside fixture');
      if (row.materialize) throw new Error('Materialization prohibited');
    }
  }
  return { rows: rows.length, cells };
}

async function main() {
  const config = JSON.parse(readFileSync(process.env.FLOW_BENCH_CONFIG, 'utf8'));
  const subject = JSON.parse(readFileSync(config.subject, 'utf8'));
  const allowed = subject.tools;
  const log = value => appendFileSync(resolve(config.runDir, 'calls.jsonl'), JSON.stringify(value) + '\n', { mode: 0o600 });
  const downstream = new Client({ name: 'instruction-flow', version: '1' });
  const env = { ...process.env, TOOLS_TO_RUN: allowed.join(','), ENABLE_LOCAL: 'true',
    ENABLE_CLONE: 'false', OCTOCODE_BETA: 'false', ALLOWED_PATHS: config.fixture,
    WORKSPACE_ROOT: config.fixture, OCTOCODE_STORAGE_MODE: 'memory', OCTOCODE_ENABLE_STATS: 'false' };
  delete env.FLOW_BENCH_CONFIG;
  const transport = new StdioClientTransport({ command: process.execPath, args: [config.entrypoint], cwd: config.fixture, env, stderr: 'pipe' });
  transport.stderr?.resume();
  let server;
  let closing = false;
  const close = async () => { if (closing) return; closing = true; await Promise.allSettled([downstream.close(), server?.close()]); };
  for (const signal of ['SIGINT', 'SIGTERM']) process.once(signal, () => void close());
  process.stdin.once('end', () => void close());
  try {
    await downstream.connect(transport);
    const catalog = await downstream.listTools();
    if (catalog.nextCursor || catalog.tools.length !== allowed.length ||
        allowed.some(name => !catalog.tools.some(tool => tool.name === name)) ||
        catalog.tools.some(tool => tool.outputSchema)) throw new Error('Unexpected catalog');
    log({ event: 'catalog', tools: catalog.tools, instructions: subject.instructions });
    server = new Server({ name: 'instruction-flow', version: '1' }, { capabilities: { tools: {} }, instructions: subject.instructions });
    server.setRequestHandler(ListToolsRequestSchema, async () => catalog);
    let calls = 0, cells = 0;
    server.setRequestHandler(CallToolRequestSchema, async (request, extra) => {
      const start = performance.now();
      const { name, arguments: args = {} } = request.params;
      let result, admitted = false;
      try {
        const cost = admit(name, args, allowed, config.fixture);
        if (++calls > 12 || cells + cost.cells > 50) throw new Error('Trial budget exceeded');
        cells += cost.cells;
        admitted = true;
        result = await downstream.callTool({ name, arguments: args }, undefined, { timeout: 90000, signal: extra.signal });
      } catch {
        result = { isError: true, content: [{ type: 'text', text: 'Trial scope, budget, or downstream request failed.' }] };
      }
      log({ event: 'call', name, args, admitted, durationMs: performance.now() - start, result });
      return result;
    });
    await server.connect(new StdioServerTransport());
  } catch { await close(); throw new Error('Instruction-flow proxy startup failed'); }
}

if (process.env.FLOW_BENCH_CONFIG) main().catch(error => { console.error(error.message); process.exitCode = 1; });
