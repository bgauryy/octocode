/** Parent grant ownership and the worker's private broker client. */
import path from 'node:path';
import {
  createWorkerCapabilityGrant,
  isForbiddenWorkerTool,
  type CapabilitySnapshot,
  type WorkerCapabilityGrant,
  type WorkerCapabilitySelection,
} from '../contracts/capabilities.js';
import {
  createWorkerMcpBroker,
  createWorkerBrokerClient,
  type WorkerBrokerBinding,
  type WorkerCapabilityView,
  type WorkerMcpBroker,
} from './mcp/broker.js';
import type { ToolCallResult } from '../types.js';
import type { SpawnAgentParams } from './agents/types.js';

export const WORKER_CAPABILITY_BINDING_ENV = 'OCTOCODE_WORKER_CAPABILITY_BINDING';
const bindingJson = process.env[WORKER_CAPABILITY_BINDING_ENV];
// Consume credentials before any model-accessible shell is launched. Credentials
// remain in this module's closure and never become prompt/session/tool data.
delete process.env[WORKER_CAPABILITY_BINDING_ENV];
const binding = bindingJson ? JSON.parse(bindingJson) as WorkerBrokerBinding : undefined;
const workerClient = binding ? createWorkerBrokerClient(binding) : undefined;
let currentWorkerView: WorkerCapabilityView | undefined;
let parentBroker: WorkerMcpBroker | undefined;
const leanWorkers = new Set<string>();
const LEAN_NATIVE_TOOLS = new Set(['read', 'write', 'edit', 'bash', 'grep', 'find', 'ls']);

export function isWorkerCapabilityClient(): boolean {
  return Boolean(workerClient) || process.env['OCTOCODE_PI_SUBAGENT'] === '1';
}

export async function initializeWorkerCapabilityRuntime(options: {
  snapshot: CapabilitySnapshot;
  refreshSnapshot?: () => Promise<CapabilitySnapshot>;
  dispatchMcp: (params: Record<string, unknown>, signal?: AbortSignal, worker?: WorkerCapabilityView) => Promise<ToolCallResult>;
}): Promise<void> {
  if (isWorkerCapabilityClient()) return;
  await disposeWorkerCapabilityRuntime();
  parentBroker = await createWorkerMcpBroker(options);
}

export function updateParentCapabilitySnapshot(snapshot: CapabilitySnapshot): void {
  if (isWorkerCapabilityClient()) throw new Error('Only the parent can update the effective capability snapshot.');
  if (!parentBroker) throw new Error('Parent capability runtime is not initialized.');
  parentBroker.setSnapshot(snapshot);
}

export function getParentCapabilitySnapshot(): CapabilitySnapshot | undefined {
  return parentBroker?.getSnapshot();
}

export function inspectWorkerCapabilityGrants(): Array<{ grant: WorkerCapabilityGrant; pendingGrant?: WorkerCapabilityGrant }> {
  return parentBroker?.inspectWorkerGrants() ?? [];
}

export async function disposeWorkerCapabilityRuntime(): Promise<void> {
  const previous = parentBroker;
  parentBroker = undefined;
  currentWorkerView = undefined;
  leanWorkers.clear();
  await previous?.dispose();
}

export async function refreshCurrentWorkerCapabilities(options: { beginTurn?: boolean; signal?: AbortSignal } = {}): Promise<WorkerCapabilityView | undefined> {
  if (!isWorkerCapabilityClient()) return undefined;
  if (!workerClient) throw new Error('Worker has no authenticated parent capability binding. Spawn a fresh worker from the parent.');
  currentWorkerView = await workerClient.readCapabilities(options.beginTurn, options.signal);
  return getCurrentWorkerCapabilities();
}

export function getCurrentWorkerCapabilities(): WorkerCapabilityView | undefined {
  return currentWorkerView ? structuredClone(currentWorkerView) : undefined;
}

export function assertCurrentWorkerNativeTool(name: string): void {
  if (!isWorkerCapabilityClient()) return;
  if (isForbiddenWorkerTool(name) || !currentWorkerView?.snapshot.nativeTools.includes(name)) {
    throw new Error(`Native tool is not granted to this worker: ${name}. Request missing access from the parent.`);
  }
}

export function filterCurrentWorkerSkills<T extends { sourceId?: string; id?: string; name: string; dir?: string; path?: string }>(skills: T[]): T[] {
  if (!isWorkerCapabilityClient()) return skills;
  const allowed = new Map(currentWorkerView?.snapshot.skills.map(skill => [skill.id, skill]) ?? []);
  return skills.filter(skill => {
    const identity = skill.sourceId ?? skill.id;
    const granted = identity ? allowed.get(identity) : undefined;
    return Boolean(granted && granted.name === skill.name && granted.path === (skill.path ?? skill.dir));
  });
}

export async function dispatchWorkerMcpAction(params: Record<string, unknown>, signal?: AbortSignal): Promise<ToolCallResult> {
  if (!workerClient) throw new Error('Worker has no authenticated parent capability binding.');
  return workerClient.dispatch(params, signal);
}

const ROLE_SKILLS: Record<string, string[]> = {
  researcher: ['octocode-research'],
  planner: ['octocode-research', 'octocode-rfc-generator'],
  architect: ['octocode-research', 'octocode-architect'],
  implementer: ['octocode-research'],
  reviewer: ['octocode-research'],
  browser: ['octocode-chrome-devtools', 'browser-agent'],
};
const LOCAL_MCP_TOOLS = ['localSearch', 'localFetch', 'astSearch', 'lspSearch'];
const ROLE_MCP_TOOLS: Record<string, string[]> = {
  researcher: [...LOCAL_MCP_TOOLS, 'ghSearch', 'ghGetFileContent', 'ghSearchHistory', 'ghGetHistoryItem', 'ghCloneRepo', 'artifactSearch'],
  planner: [...LOCAL_MCP_TOOLS, 'ghSearch', 'ghGetFileContent', 'artifactSearch'],
  architect: [...LOCAL_MCP_TOOLS, 'ghSearchHistory', 'ghGetHistoryItem'],
  implementer: LOCAL_MCP_TOOLS,
  reviewer: [...LOCAL_MCP_TOOLS, 'ghSearch', 'ghGetFileContent', 'ghSearchHistory', 'ghGetHistoryItem'],
  browser: LOCAL_MCP_TOOLS,
};

function assertWorkerCapabilityGateways(selection: WorkerCapabilitySelection): void {
  const nativeTools = new Set(selection.nativeTools ?? []);
  if ((selection.skills?.length ?? 0) > 0 && !nativeTools.has('skill')) {
    throw new Error('Worker skill grants require the native tool "skill".');
  }
  if ((selection.mcpTools?.length ?? 0) > 0 && !nativeTools.has('MCPTool')) {
    throw new Error('Worker MCP grants require the native tool "MCPTool".');
  }
}

/** Omitted fields use a focused role default; explicit empty arrays stay empty. */
export function resolveWorkerCapabilitySelection(params: SpawnAgentParams, snapshot: CapabilitySnapshot): WorkerCapabilitySelection {
  if ((params.resourceMode ?? 'lean') === 'lean') {
    const nativeTools = params.capabilities?.nativeTools ?? (params.tools ?? []).filter(name => !isForbiddenWorkerTool(name));
    if (nativeTools.some(name => !LEAN_NATIVE_TOOLS.has(name))) throw new Error('Lean workers support Pi builtin native tools only; use resourceMode:"octocode" for extension tools.');
    return { nativeTools, skills: [], mcpTools: [] };
  }
  const profile = params.capabilityProfile ?? 'custom';
  const requested = params.capabilities;
  const nativeTools = requested?.nativeTools ?? (params.tools ?? []).filter(name => !isForbiddenWorkerTool(name) && (profile === 'custom' || snapshot.nativeTools.includes(name)));
  const skillNames = new Set(ROLE_SKILLS[profile] ?? []);
  const selectedPaths = params.skills === undefined ? undefined : new Set(params.skills);
  const defaultSkills = snapshot.skills.filter(skill => selectedPaths ? selectedPaths.has(skill.path) || selectedPaths.has(path.dirname(skill.path)) : skillNames.has(skill.name)).map(skill => skill.id);
  const mcpNames = new Set(ROLE_MCP_TOOLS[profile] ?? LOCAL_MCP_TOOLS);
  const selection = {
    nativeTools,
    skills: requested?.skills ?? (nativeTools.includes('skill') ? defaultSkills : []),
    mcpTools: requested?.mcpTools ?? (nativeTools.includes('MCPTool') ? snapshot.mcpTools.filter(tool => tool.server === 'octocode' && mcpNames.has(tool.tool)).map(({ server, tool }) => ({ server, tool })) : []),
  };
  assertWorkerCapabilityGateways(selection);
  return selection;
}

/** Synchronous spawn seam after root initialization; a worker cannot call this. */
export function bindSpawnedWorkerCapabilities(workerId: string, params: SpawnAgentParams): { env: NodeJS.ProcessEnv; grant?: WorkerCapabilityGrant; params: SpawnAgentParams } {
  if (isWorkerCapabilityClient()) throw new Error('Workers cannot grant capabilities or spawn recursive workers.');
  if (!parentBroker) {
    if ((params.resourceMode ?? 'lean') !== 'lean') throw new Error('Parent capability runtime is not initialized; wait for the effective catalog before spawning workers.');
    return { env: {}, params: { ...params, skills: [] } };
  }
  const snapshot = parentBroker.getSnapshot();
  const selection = resolveWorkerCapabilitySelection(params, snapshot);
  createWorkerCapabilityGrant(snapshot, { workerId, selection, snapshotRevision: params.capabilitySnapshotRevision });
  const childBinding = parentBroker.registerWorker(workerId, selection);
  if ((params.resourceMode ?? 'lean') === 'lean') leanWorkers.add(workerId);
  const { grant, snapshot: projected } = parentBroker.getWorkerCapabilities(workerId);
  return {
    env: { [WORKER_CAPABILITY_BINDING_ENV]: JSON.stringify(childBinding) },
    grant,
    params: { ...params, tools: grant.nativeTools, skills: projected.skills.map(skill => skill.path) },
  };
}

export function configureWorkerCapabilities(workerId: string, update: { snapshotRevision: string; selection: WorkerCapabilitySelection; expectedGrantRevision?: number }): WorkerCapabilityGrant {
  if (isWorkerCapabilityClient() || !parentBroker) throw new Error('Only the active parent runtime can configure worker capabilities.');
  if (leanWorkers.has(workerId) && ((update.selection.skills?.length ?? 0) > 0 || (update.selection.mcpTools?.length ?? 0) > 0 || update.selection.nativeTools?.some(name => !LEAN_NATIVE_TOOLS.has(name)))) throw new Error('Lean workers support Pi builtin native tools only and keep skill/MCP grants empty. Spawn an Octocode worker for those resources.');
  assertWorkerCapabilityGateways(update.selection);
  return parentBroker.configureWorker(workerId, update);
}

export function getParentWorkerCapabilities(workerId: string): WorkerCapabilityView | undefined {
  if (!parentBroker?.inspectWorkerGrants().some(item => item.grant.workerId === workerId)) return undefined;
  return parentBroker.getWorkerCapabilities(workerId);
}

export function revokeWorkerCapabilities(workerId: string): void {
  leanWorkers.delete(workerId);
  parentBroker?.removeWorker(workerId);
}
