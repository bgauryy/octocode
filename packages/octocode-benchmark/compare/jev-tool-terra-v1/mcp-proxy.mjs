import { appendFileSync, mkdirSync, readFileSync } from 'node:fs';
import { isAbsolute, join } from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, CallToolResultSchema, ListToolsRequestSchema } from '@modelcontextprotocol/sdk/types.js';

const READ_TOOLS = ['ghSearch', 'ghGetFileContent', 'ghSearchHistory', 'ghGetHistoryItem', 'artifactSearch'];
const LIMITS = { ordinary: 40, jev: 20 };
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const own = (value, key) => Object.hasOwn(value, key);

function loadConfig() {
  const path = process.env.JEV_BENCH_CONFIG;
  if (!path || !isAbsolute(path)) throw new Error('JEV_BENCH_CONFIG must be an absolute JSON file path');
  const config = JSON.parse(readFileSync(path, 'utf8'));
  const fields = ['version', 'arm', 'entrypoint', 'runDir', 'cwd', 'requestTimeoutMs'];
  if (!object(config) || Object.keys(config).some(key => !fields.includes(key)) || config.version !== 1 ||
      !['baseline', 'candidate'].includes(config.arm)) throw new Error('Invalid benchmark config');
  for (const key of ['entrypoint', 'runDir', 'cwd']) {
    if (typeof config[key] !== 'string' || !isAbsolute(config[key])) throw new Error(`Config ${key} must be absolute`);
  }
  if (!Number.isSafeInteger(config.requestTimeoutMs) || config.requestTimeoutMs <= 0) throw new Error('Invalid requestTimeoutMs');
  if (!process.env.OCTOCODE_HOME || !isAbsolute(process.env.OCTOCODE_HOME)) throw new Error('An isolated absolute OCTOCODE_HOME is required');
  return config;
}

// Only admission policy belongs here. The downstream owns every tool's schema and execution.
function costOf(name, args, allowed) {
  if (!allowed.includes(name)) throw new Error('Tool is outside the benchmark read allowlist');
  if (!object(args)) throw new Error('Tool arguments must be an object');
  const rows = own(args, 'queries') ? args.queries : [args];
  if (!Array.isArray(rows) || rows.length === 0 || rows.some(row => !object(row))) throw new Error('Queries must be a nonempty array of objects');
  if (name !== 'jev') {
    if (rows.some(row => row.materialize === true)) throw new Error('Local materialization is outside benchmark read scope');
    return { ordinary: rows.length, jev: 0, nestedTools: [] };
  }
  const nestedTools = [];
  for (const row of rows) {
    const context = row.context;
    if (!object(context)) throw new Error('Jev context must be an inline value or allowed read query');
    if (own(context, 'value')) {
      if (Object.keys(context).some(key => key !== 'value')) throw new Error('Inline Jev context cannot include a tool');
    } else {
      if (!READ_TOOLS.includes(context.tool) || !object(context.query) ||
          own(context.query, 'queries') || context.query.materialize === true || Object.keys(context).some(key => !['tool', 'query'].includes(key))) {
        throw new Error('Jev nested context must be one allowed remote read query');
      }
      nestedTools.push(context.tool);
    }
  }
  return { ordinary: nestedTools.length, jev: rows.length, nestedTools };
}

function providerUsage(result, expectedRows) {
  const rows = result?.structuredContent?.results;
  const covered = new Set();
  let input = 0;
  let output = 0;
  const allocations = [];
  if (Array.isArray(rows)) for (const row of rows) {
    const index = row.index;
    const data = row.data;
    if (row.status !== 'success' || !Number.isInteger(index) || index < 0 || index >= expectedRows || covered.has(index)) continue;
    const usage = data?.usage;
    if (![usage?.input_tokens, usage?.output_tokens].every(value => Number.isSafeInteger(value) && value >= 0)) continue;
    const attribution = data.usageAttribution;
    if (attribution) {
      const members = attribution.sharedWith;
      if (attribution.ownerIndex !== index || !Array.isArray(members) || members.length < 2 ||
          new Set(members).size !== members.length || !members.includes(index) ||
          members.some(member => !Number.isInteger(member) || member < 0 || member >= expectedRows || covered.has(member))) continue;
      for (const member of members) covered.add(member);
      allocations.push({ ownerIndex: index, sharedWith: members, usage });
    } else {
      covered.add(index);
      allocations.push({ ownerIndex: index, sharedWith: [index], usage });
    }
    input += usage.input_tokens;
    output += usage.output_tokens;
  }
  const unknownRows = Array.from({ length: expectedRows }, (_, index) => index).filter(index => !covered.has(index));
  const complete = unknownRows.length === 0;
  return { complete, inputTokens: complete ? input : null, outputTokens: complete ? output : null,
    knownInputTokens: input, knownOutputTokens: output, unknownRows, allocations };
}

async function main() {
  const config = loadConfig();
  mkdirSync(config.runDir, { recursive: true });
  const logPath = join(config.runDir, 'calls.jsonl');
  // Redact credentials only in disk receipts. The MCP response remains unchanged.
  const secrets = Object.entries(process.env).filter(([key, value]) => /(?:TOKEN|SECRET|PASSWORD|(?:^|_)KEY)$/.test(key) && value)
    .map(([, value]) => value).sort((a, b) => b.length - a.length);
  const redact = value => {
    if (typeof value === 'string') {
      for (const secret of secrets) value = value.split(secret).join('[REDACTED]');
      return value;
    }
    if (Array.isArray(value)) return value.map(redact);
    if (object(value)) return Object.fromEntries(Object.entries(value).map(([key, child]) => [redact(key), redact(child)]));
    return value;
  };
  const log = value => {
    appendFileSync(logPath, `${JSON.stringify(redact(value))}\n`, { mode: 0o600 });
  };
  const allowed = [...READ_TOOLS, ...(config.arm === 'candidate' ? ['jev'] : [])];
  const env = { ...process.env, TOOLS_TO_RUN: allowed.join(','), ENABLE_LOCAL: 'false', ENABLE_CLONE: 'false', ENABLE_AST_REWRITE_APPLY: 'false' };
  delete env.JEV_BENCH_CONFIG;
  if (config.arm === 'baseline') env.OCTOCODE_JEV_KEY = '';
  const downstream = new Client({ name: 'jev-tool-terra-proxy', version: '1.0.0' }, { capabilities: {} });
  const transport = new StdioClientTransport({ command: process.execPath, args: [config.entrypoint], cwd: config.cwd, env, stderr: 'pipe' });
  // Downstream stderr is not a credential-safe receipt surface. Drain without recording.
  transport.stderr?.resume();
  let server;
  let closing = false;
  const close = async () => {
    if (closing) return;
    closing = true;
    await Promise.allSettled([downstream.close(), server?.close()]);
  };
  for (const signal of ['SIGINT', 'SIGTERM']) process.once(signal, () => { void close(); });
  process.stdin.once('end', () => { void close(); });
  try {
    await downstream.connect(transport);
    const tools = [];
    let cursor;
    const cursors = new Set();
    do {
      const page = await downstream.listTools(cursor ? { cursor } : {}, { timeout: config.requestTimeoutMs });
      tools.push(...page.tools);
      cursor = page.nextCursor;
      if (cursor && cursors.has(cursor)) throw new Error('Downstream catalog repeated a cursor');
      cursors.add(cursor);
    } while (cursor);
    if (tools.length !== allowed.length || new Set(tools.map(tool => tool.name)).size !== tools.length ||
        allowed.some(name => !tools.some(tool => tool.name === name))) throw new Error('Downstream catalog does not match benchmark arm');
    const instructions = downstream.getInstructions();
    if (typeof instructions !== 'string' || !instructions.trim()) throw new Error('Downstream instructions missing');
    log({ event: 'catalog', arm: config.arm, tools, instructions, limits: LIMITS, server: downstream.getServerVersion() });
    server = new Server({ name: 'octocode-benchmark', version: '1.0.0' }, { capabilities: { tools: {} }, instructions });
    const spent = { ordinary: 0, jev: 0 };
    let sequence = 0;
    server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools }));
    server.setRequestHandler(CallToolRequestSchema, async (request, extra) => {
      const id = ++sequence;
      const startedAt = new Date().toISOString();
      const start = performance.now();
      const { name, arguments: args = {} } = request.params;
      let cost;
      let admitted = false;
      let result;
      let errorCode;
      try {
        cost = costOf(name, args, allowed);
        if (Object.keys(LIMITS).some(key => spent[key] + cost[key] > LIMITS[key])) {
          errorCode = 'benchmarkBudgetExceeded';
          throw new Error('Benchmark query budget exceeded');
        }
        spent.ordinary += cost.ordinary;
        spent.jev += cost.jev;
        admitted = true;
        log({ event: 'callStarted', id, startedAt, name, input: args, cost, countersAfter: { ...spent } });
        result = await downstream.request({ method: 'tools/call', params: request.params }, CallToolResultSchema,
          { signal: extra.signal, timeout: config.requestTimeoutMs });
      } catch (error) {
        errorCode ??= admitted ? 'downstreamRequestFailed' : 'benchmarkScopeRejected';
        // SDK/transport errors can contain credentials; expose a stable safe error instead.
        result = { isError: true, content: [{ type: 'text', text: JSON.stringify({ errorCode,
          error: admitted ? 'Downstream MCP request failed; provider usage may be unknown.' : error.message }) }] };
      }
      log({ event: 'call', id, startedAt, durationMs: performance.now() - start, name, input: args,
        admitted, cost: cost ?? null, countersAfter: { ...spent }, errorCode: errorCode ?? null,
        isError: result.isError === true, result,
        errorRows: Array.isArray(result.structuredContent?.results)
          ? result.structuredContent.results.filter(row => row.status === 'error').map(row => row.index) : [],
        providerUsage: name === 'jev' && admitted ? providerUsage(result, cost.jev) : null });
      return result;
    });
    await server.connect(new StdioServerTransport());
  } catch (error) {
    log({ event: 'startupError', error: 'Proxy initialization failed', type: error?.name ?? 'Error' });
    await close();
    throw error;
  }
}

main().catch(() => {
  process.stderr.write('Benchmark MCP proxy failed to initialize.\n');
  process.exitCode = 1;
});
