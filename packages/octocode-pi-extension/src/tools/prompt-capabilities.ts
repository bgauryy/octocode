import path from 'node:path';
import { discoverAgentInstructionFiles } from '../contracts/capability-sources.js';
import { escapePromptMetadata } from './prompt-safety.js';
import type { PiContext, PiInstance, SkillInfo, NotifyFn } from '../types.js';
import type { SessionScopedState } from '../session-scoped-state.js';
import { buildEffectiveCapabilitySnapshot, publishSessionCapabilities } from './capability-session.js';
import {
  getEffectiveMcpSnapshot,
  refreshMcpCapabilities,
  handleMcpAction,
  getGrantedDynamicMcpProxyTools,
  isDynamicMcpProxyTool,
} from './mcp-tool.js';
import { loadMcpConfig } from './mcp/config.js';
import { discoverSkills, discoverSkillCandidates } from './skill-discovery.js';
import { initializeWorkerCapabilityRuntime, updateParentCapabilitySnapshot, getParentCapabilitySnapshot, refreshCurrentWorkerCapabilities, getCurrentWorkerCapabilities } from './worker-capabilities.js';

/** Resolve once at the turn boundary; the broker also revalidates before use. */
export async function preparePromptCapabilities(options: {
  pi: PiInstance; ctx?: PiContext; session: SessionScopedState; worker: boolean;
  piSkills?: SkillInfo[]; fallbackTools: string[]; notify: NotifyFn;
}) {
  const { pi, ctx, session, worker, notify } = options;
  const cwd = ctx?.cwd ?? process.cwd();
  if (worker) {
    const view = await refreshCurrentWorkerCapabilities({ beginTurn: true });
    const dynamicMcpTools = view
      ? getGrantedDynamicMcpProxyTools(pi, view.snapshot.mcpTools)
      : [];
    pi.setActiveTools?.([...(view?.snapshot.nativeTools ?? []), ...dynamicMcpTools]);
    const signature = view ? `${view.grant.revision}:${view.snapshot.revision}` : '';
    if (view && signature !== session.workerGrantSignature) {
      pi.appendEntry?.('octocode-worker-capabilities', { schemaVersion: 1, grant: view.grant, capabilityRevision: view.snapshot.revision });
      session.workerGrantSignature = signature;
    }
  }
  const activeTools = new Set(pi.getActiveTools?.() ?? options.fallbackTools);
  if (activeTools.has('MCPTool')) {
    // Race the MCP catalog refresh against a 800 ms deadline so a slow or
    // reconnecting server cannot freeze before_agent_start (and the whole Pi
    // UI with it).  The stale cached catalog is used for this turn; the fresh
    // result lands in the background and will be used starting next turn.
    // 800 ms is conservative: local stdio servers typically respond in < 50 ms;
    // only remote / restarting servers take longer.
    const MCP_REFRESH_TIMEOUT_MS = 800;
    await Promise.race([
      refreshMcpCapabilities(ctx),
      new Promise<void>(resolve => setTimeout(resolve, MCP_REFRESH_TIMEOUT_MS)),
    ]);
  }
  session.latestPiSkills = options.piSkills;
  session.latestAvailableSkills = worker
    ? (getCurrentWorkerCapabilities()?.snapshot.skills ?? []).map(skill => ({ ...skill, sourceId: skill.id, description: skill.description ?? '', dir: path.dirname(skill.path), source: 'parent grant' }))
    : activeTools.has('skill') ? discoverSkills(cwd, options.piSkills, undefined, { trusted: ctx?.isProjectTrusted?.() === true }) : [];
  const snapshot = worker
    ? getCurrentWorkerCapabilities()?.snapshot
    : buildEffectiveCapabilitySnapshot(
      [...activeTools].filter(name => !isDynamicMcpProxyTool(pi, name)),
      session.latestAvailableSkills,
      activeTools.has('MCPTool') ? getEffectiveMcpSnapshot(ctx) : undefined,
    );
  if (snapshot) {
    session.capabilityRevision = snapshot.revision;
    publishSessionCapabilities(cwd, snapshot);
    if (!worker) {
      if (getParentCapabilitySnapshot()) updateParentCapabilitySnapshot(snapshot);
      else await initializeWorkerCapabilityRuntime({ snapshot, dispatchMcp: (params, signal) => handleMcpAction(params, signal, ctx), refreshSnapshot: async () => {
        await refreshMcpCapabilities(ctx);
        const tools = pi.getActiveTools?.() ?? [...activeTools];
        const current = buildEffectiveCapabilitySnapshot(
          tools.filter(name => !isDynamicMcpProxyTool(pi, name)),
          tools.includes('skill') ? discoverSkills(cwd, session.latestPiSkills, undefined, { trusted: ctx?.isProjectTrusted?.() === true }) : [],
          tools.includes('MCPTool') ? getEffectiveMcpSnapshot(ctx) : undefined,
        );
        publishSessionCapabilities(cwd, current);
        return current;
      } });
    }
  }
  if (!worker && !session.announcedImports) {
    const foreignSkills = discoverSkillCandidates(cwd, options.piSkills).some(candidate => !candidate.defaultEnabled && !candidate.selected);
    const foreignMcp = [...(await loadMcpConfig(ctx)).configuredServers.values()].some(config => config.discovered && config.discovered.reviewStatus !== 'active');
    if (foreignSkills || foreignMcp) {
      notify(ctx, 'Additional skills/MCPs found. Import them with /config.', 'info');
      session.announcedImports = true;
    }
  }
  return activeTools;
}

export function renderAgentsProtocolInstructions(
  ctx?: PiContext,
  contextFiles: unknown[] = [],
  suppress = false,
  cache?: { get: () => { revisionKey: string; rendered: string } | undefined; set: (entry: { revisionKey: string; rendered: string }) => void },
): string {
  if (suppress) return '';
  const excludePaths = contextFiles.flatMap(value => value && typeof value === 'object' && 'path' in value && typeof value.path === 'string' ? [value.path] : []);
  const { files, diagnostics } = discoverAgentInstructionFiles(ctx?.cwd ?? process.cwd(), { trusted: ctx?.isProjectTrusted?.() === true, excludePaths });
  if (!files.length && !diagnostics.length) return '';
  // Cache by the NUL-joined revision digests of every discovered file plus a
  // diagnostic count suffix.  When AGENTS.md and other instruction files are
  // stable across turns (the common case) this avoids re-allocating the
  // rendered string and all the escaping work that goes with it.
  const revisionKey = files.map(f => f.revision).join('\x00') + (diagnostics.length ? `\x00d${diagnostics.length}` : '');
  const cached = cache?.get();
  if (cached?.revisionKey === revisionKey) return cached.rendered;
  const rendered = ['<agents_protocol>', ...files.map(file => `Instructions from ${file.scope} source ${escapePromptMetadata(file.path)}:\n${file.content}`), ...(diagnostics.length ? [`Source diagnostics: ${escapePromptMetadata(JSON.stringify(diagnostics))}`] : []), '</agents_protocol>'].join('\n\n');
  cache?.set({ revisionKey, rendered });
  return rendered;
}
