/** Session-local authenticated worker gateway. Only the parent owns MCP connections. */
import { randomBytes, timingSafeEqual } from 'node:crypto';
import { createServer, request, type IncomingMessage, type ServerResponse } from 'node:http';
import {
  CapabilitySnapshotSchema,
  WorkerCapabilityGrantSchema,
  createWorkerCapabilityGrant,
  intersectWorkerCapabilityGrant,
  mcpToolIdentityKey,
  projectWorkerCapabilitySnapshot,
  type CapabilitySnapshot,
  type WorkerCapabilityGrant,
  type WorkerCapabilitySelection,
} from '../../contracts/capabilities.js';
import type { ToolCallResult } from '../../types.js';
import { readMcpCatalogPage, type McpCatalogPageQuery } from './catalog-pages.js';
import { workerMcpCatalogSnapshot } from './worker-catalog.js';

const MAX_REQUEST_BYTES = 1024 * 1024;
const MAX_RESPONSE_BYTES = 8 * 1024 * 1024;
const MAX_WORKERS = 50;
const MAX_IN_FLIGHT = 32;

export interface WorkerBrokerBinding {
  version: 1;
  endpoint: string;
  workerId: string;
  /** Transport secret: never put bindings in tool results, prompts, or session entries. */
  token: string;
}
export interface WorkerCapabilityView {
  grant: WorkerCapabilityGrant;
  snapshot: CapabilitySnapshot;
}
export interface WorkerMcpBroker {
  registerWorker(workerId: string, selection: WorkerCapabilitySelection): WorkerBrokerBinding;
  configureWorker(workerId: string, options: { snapshotRevision: string; selection: WorkerCapabilitySelection; expectedGrantRevision?: number }): WorkerCapabilityGrant;
  getWorkerCapabilities(workerId: string, beginTurn?: boolean): WorkerCapabilityView;
  setSnapshot(snapshot: CapabilitySnapshot): void;
  getSnapshot(): CapabilitySnapshot;
  inspectWorkerGrants(): Array<{ grant: WorkerCapabilityGrant; pendingGrant?: WorkerCapabilityGrant }>;
  removeWorker(workerId: string): void;
  dispose(): Promise<void>;
}
type WorkerState = { token: string; grant: WorkerCapabilityGrant; pending?: WorkerCapabilityGrant };

function sameToken(actual: string, expected: string): boolean {
  const left = Buffer.from(actual);
  const right = Buffer.from(expected);
  return left.length === right.length && timingSafeEqual(left, right);
}

function writeJson(response: ServerResponse, status: number, body: unknown): void {
  if (response.destroyed) return;
  let text = JSON.stringify(body);
  if (Buffer.byteLength(text) > MAX_RESPONSE_BYTES) {
    status = 413;
    text = JSON.stringify({ error: 'Worker broker response reached its terminal byte limit. Narrow the request or use the tool continuation.', terminalLimit: { kind: 'response-bytes', limit: MAX_RESPONSE_BYTES } });
  }
  response.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
  response.end(text);
}

async function readJson(req: IncomingMessage): Promise<Record<string, unknown>> {
  const chunks: Buffer[] = [];
  let bytes = 0;
  for await (const chunk of req) {
    const part = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    bytes += part.length;
    if (bytes > MAX_REQUEST_BYTES) throw new Error('Worker broker request reached its terminal byte limit.');
    chunks.push(part);
  }
  const value: unknown = JSON.parse(Buffer.concat(chunks).toString('utf8'));
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Expected a worker broker request object.');
  return value as Record<string, unknown>;
}

function intersectWithSelection(grant: WorkerCapabilityGrant, desired: WorkerCapabilityGrant): WorkerCapabilityGrant {
  const native = new Set(desired.nativeTools);
  const skills = new Set(desired.skills);
  const mcp = new Set(desired.mcpTools.map(mcpToolIdentityKey));
  return {
    ...grant, revision: desired.revision - 1, snapshotRevision: desired.snapshotRevision,
    nativeTools: grant.nativeTools.filter(name => native.has(name)),
    skills: grant.skills.filter(id => skills.has(id)),
    mcpTools: grant.mcpTools.filter(tool => mcp.has(mcpToolIdentityKey(tool))),
  };
}

export async function createWorkerMcpBroker(options: {
  snapshot: CapabilitySnapshot;
  refreshSnapshot?: () => Promise<CapabilitySnapshot>;
  dispatchMcp: (params: Record<string, unknown>, signal?: AbortSignal, worker?: WorkerCapabilityView) => Promise<ToolCallResult>;
}): Promise<WorkerMcpBroker> {
  let snapshot = CapabilitySnapshotSchema.parse(options.snapshot);
  const workers = new Map<string, WorkerState>();
  const controllers = new Set<AbortController>();
  let disposed = false;
  let endpoint = '';
  let inFlight = 0;
  let snapshotRefresh: Promise<void> | undefined;
  const setSnapshot = (next: CapabilitySnapshot): void => {
    snapshot = CapabilitySnapshotSchema.parse(next);
    for (const state of workers.values()) {
      state.grant = intersectWorkerCapabilityGrant(snapshot, state.grant);
      if (state.pending) state.pending = intersectWorkerCapabilityGrant(snapshot, state.pending);
    }
  };
  const refreshSnapshot = async (): Promise<void> => {
    if (!options.refreshSnapshot) return;
    snapshotRefresh ??= options.refreshSnapshot().then(setSnapshot).finally(() => { snapshotRefresh = undefined; });
    await snapshotRefresh;
  };

  const view = (workerId: string, beginTurn = false): WorkerCapabilityView => {
    const state = workers.get(workerId);
    if (!state || disposed) throw new Error('Worker capability identity is no longer active.');
    if (beginTurn && state.pending) {
      state.grant = state.pending;
      state.pending = undefined;
    }
    state.grant = intersectWorkerCapabilityGrant(snapshot, state.grant);
    return { grant: structuredClone(state.grant), snapshot: projectWorkerCapabilitySnapshot(snapshot, state.grant) };
  };

  const dispatch = async (workerId: string, params: Record<string, unknown>, signal: AbortSignal): Promise<ToolCallResult> => {
    const current = view(workerId);
    if (!current.grant.nativeTools.includes('MCPTool')) throw new Error('MCPTool is not granted to this worker. Request missing access from the parent.');
    const action = params['action'];
    if (!['call', 'describe', 'list'].includes(String(action))) throw new Error('MCP administration, resources, and prompts are parent-owned. Request missing access from the parent.');
    const server = typeof params['server'] === 'string' ? params['server'] : undefined;
    const tool = typeof params['tool'] === 'string' ? params['tool'] : undefined;
    if (server && !current.snapshot.mcpTools.some(item => item.server === server)) throw new Error(`MCP server is not granted to this worker: ${server}`);
    if (tool && !server) throw new Error('Worker MCP tool selection requires an exact server and tool identity.');
    if (server && tool && !current.snapshot.mcpTools.some(item => item.server === server && item.tool === tool)) throw new Error(`MCP tool is not granted to this worker: ${server}/${tool}`);
    if (action === 'call' && (!server || !tool)) throw new Error('Worker MCP call requires an exact server and tool identity.');
    if (action === 'list' || (action === 'describe' && !tool)) {
      const query: McpCatalogPageQuery = {};
      for (const key of ['offset', 'textOffset', 'limit'] as const) {
        const value = params[key];
        if (value !== undefined && (typeof value !== 'number' || !Number.isInteger(value))) throw new Error(`Worker MCP catalog ${key} must be an integer.`);
        if (typeof value === 'number') query[key] = value;
      }
      if (params['catalogRevision'] !== undefined && typeof params['catalogRevision'] !== 'string') throw new Error('Worker MCP catalogRevision must be a string.');
      if (typeof params['catalogRevision'] === 'string') query.catalogRevision = params['catalogRevision'];
      const selected = current.snapshot.mcpTools.filter(item => (!server || item.server === server) && (!tool || item.tool === tool));
      const page = readMcpCatalogPage(workerMcpCatalogSnapshot({ ...current.snapshot, mcpTools: selected }, process.cwd()), query);
      const next = page.next ? { ...page.next, params: { queries: page.next.params.queries.map(item => ({ ...item, ...(server ? { server } : {}), ...(tool ? { tool } : {}) })) } } : undefined;
      const result = { ...page, ...(next ? { next } : {}), snapshotRevision: snapshot.revision, grantRevision: current.grant.revision };
      return { content: [{ type: 'text', text: JSON.stringify(result) }], details: result, ...(page.diagnostic ? { isError: true } : {}) };
    }
    // No child transport/config load: the injected parent callback owns enablement,
    // schema validation, continuations, connection reuse, and provider execution.
    return options.dispatchMcp(params, signal, current);
  };

  const server = createServer((req, response) => {
    void (async () => {
      const workerId = req.headers['x-octocode-worker'];
      const authorization = req.headers.authorization ?? '';
      const state = typeof workerId === 'string' ? workers.get(workerId) : undefined;
      if (disposed || req.headers.origin || !state || !sameToken(authorization, `Bearer ${state.token}`)) {
        writeJson(response, 401, { error: 'Worker broker authentication failed.' });
        req.resume();
        return;
      }
      if (req.method !== 'POST' || !['/capabilities', '/begin-turn', '/mcp'].includes(req.url ?? '')) {
        writeJson(response, 404, { error: 'Unknown worker broker operation.' });
        req.resume();
        return;
      }
      if (inFlight >= MAX_IN_FLIGHT) {
        writeJson(response, 429, { error: 'Worker broker concurrency limit reached. Retry after the current call completes.' });
        req.resume();
        return;
      }
      inFlight += 1;
      const controller = new AbortController();
      controllers.add(controller);
      response.on('close', () => { if (!response.writableEnded) controller.abort(); });
      try {
        const params = await readJson(req);
        await refreshSnapshot();
        const result = req.url === '/mcp'
          ? await dispatch(workerId as string, params, controller.signal)
          : view(workerId as string, req.url === '/begin-turn');
        writeJson(response, 200, result);
      } catch (error) {
        writeJson(response, 403, { error: error instanceof Error ? error.message : 'Worker broker request failed.' });
      } finally {
        inFlight -= 1;
        controllers.delete(controller);
      }
    })().catch(() => writeJson(response, 500, { error: 'Worker broker request failed.' }));
  });
  server.maxConnections = MAX_IN_FLIGHT + MAX_WORKERS;
  server.requestTimeout = 60_000;
  server.headersTimeout = 10_000;
  server.keepAliveTimeout = 1000;
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => { server.off('error', reject); resolve(); });
  });
  const address = server.address();
  if (!address || typeof address === 'string') { server.close(); throw new Error('Unable to bind the local worker broker.'); }
  endpoint = `http://127.0.0.1:${address.port}`;
  server.unref();

  return {
    registerWorker(workerId, selection) {
      if (disposed) throw new Error('Worker broker is disposed.');
      if (workers.has(workerId)) throw new Error('Worker capability identity is already registered.');
      if (workers.size >= MAX_WORKERS) throw new Error('Worker capability registry is full.');
      const grant = createWorkerCapabilityGrant(snapshot, { workerId, selection });
      const token = randomBytes(32).toString('base64url');
      workers.set(workerId, { token, grant });
      return { version: 1, endpoint, workerId, token };
    },
    configureWorker(workerId, update) {
      const state = workers.get(workerId);
      if (!state || disposed) throw new Error('Worker capability identity is no longer active.');
      const previous = state.pending ?? state.grant;
      if (update.expectedGrantRevision !== undefined && update.expectedGrantRevision !== previous.revision) throw new Error('Worker grant revision is stale; inspect the worker before configuring it.');
      const desired = createWorkerCapabilityGrant(snapshot, {
        workerId, revision: previous.revision + 2, snapshotRevision: update.snapshotRevision,
        selection: {
          nativeTools: update.selection.nativeTools ?? previous.nativeTools,
          skills: update.selection.skills ?? previous.skills,
          mcpTools: update.selection.mcpTools ?? previous.mcpTools,
        },
      });
      state.grant = intersectWithSelection(state.grant, desired);
      state.pending = desired;
      return structuredClone(desired);
    },
    getWorkerCapabilities: view,
    getSnapshot: () => structuredClone(snapshot),
    inspectWorkerGrants: () => [...workers.values()].map(state => ({ grant: structuredClone(state.grant), ...(state.pending ? { pendingGrant: structuredClone(state.pending) } : {}) })),
    setSnapshot,
    removeWorker(workerId) { workers.delete(workerId); },
    async dispose() {
      if (disposed) return;
      disposed = true;
      workers.clear();
      for (const controller of controllers) controller.abort();
      server.closeAllConnections();
      await new Promise<void>(resolve => { server.close(() => resolve()); });
    },
  };
}

export function createWorkerBrokerClient(binding: WorkerBrokerBinding): {
  readCapabilities(beginTurn?: boolean, signal?: AbortSignal): Promise<WorkerCapabilityView>;
  dispatch(params: Record<string, unknown>, signal?: AbortSignal): Promise<ToolCallResult>;
} {
  const endpoint = new URL(binding.endpoint);
  if (binding.version !== 1 || endpoint.protocol !== 'http:' || endpoint.hostname !== '127.0.0.1' || endpoint.username || endpoint.password || !/^\d+$/.test(endpoint.port) || !binding.workerId || !/^[\w-]{43}$/.test(binding.token)) throw new Error('Invalid local worker broker binding.');
  const send = (route: string, body: unknown, signal?: AbortSignal): Promise<unknown> => new Promise((resolve, reject) => {
    const json = JSON.stringify(body);
    if (Buffer.byteLength(json) > MAX_REQUEST_BYTES) { reject(new Error('Worker broker request reached its terminal byte limit.')); return; }
    const req = request(new URL(route, endpoint), {
      method: 'POST', signal, agent: false,
      headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(json), 'x-octocode-worker': binding.workerId, Authorization: `Bearer ${binding.token}` },
    }, response => {
      const chunks: Buffer[] = [];
      let bytes = 0;
      response.on('data', (chunk: Buffer) => {
        bytes += chunk.length;
        if (bytes > MAX_RESPONSE_BYTES) { response.destroy(new Error('Worker broker response reached its terminal byte limit.')); return; }
        chunks.push(chunk);
      });
      response.on('error', reject);
      response.on('end', () => {
        try {
          const value = JSON.parse(Buffer.concat(chunks).toString('utf8')) as Record<string, unknown>;
          if (response.statusCode !== 200) { reject(new Error(String(value['error'] ?? 'Worker broker request failed.'))); return; }
          resolve(value);
        } catch { reject(new Error('Invalid worker broker response.')); }
      });
    });
    req.setTimeout(60_000, () => req.destroy(new Error('Worker broker request timed out.')));
    req.on('error', reject);
    req.end(json);
  });
  return {
    async readCapabilities(beginTurn = false, signal) {
      const value = await send(beginTurn ? '/begin-turn' : '/capabilities', {}, signal) as WorkerCapabilityView;
      const grant = WorkerCapabilityGrantSchema.parse(value.grant);
      if (grant.workerId !== binding.workerId) throw new Error('Worker broker returned a different capability identity.');
      return { grant, snapshot: CapabilitySnapshotSchema.parse(value.snapshot) };
    },
    async dispatch(params, signal) { return await send('/mcp', params, signal) as ToolCallResult; },
  };
}
