import type {PromptMode} from './contracts/protocols.js';
import fs from 'node:fs';
import { propagateOctocodeEnv, getOctocodeHome, isPersistentStorageEnabledForExtension as isPersistentStorageEnabled } from "@octocodeai/config";
import { openPersistentAwareness } from './tools/storage-policy.js';
import { getInternalErrorLogPath, logInternalError } from './internal-error-log.js';
export { getInternalErrorLogPath, logInternalError } from './internal-error-log.js';
export type { InternalErrorLogOptions } from './internal-error-log.js';
import { DISABLED_BUILTIN_TOOL_NAMES, OVERRIDDEN_BUILTIN_TOOL_NAMES } from './constants.js';
import { checkForCoreUpdate } from './core-update-check.js';
import { readOwnVersion } from './package-metadata.js';
import {
  getAssetPaths,
  readTextIfExists,
  listBundledSkills,
  getInstallSource,
  getAwarenessCLIPath,
  resolveAwarenessCliPath,
} from './assets.js';
// Expose the Awareness CLI for agents. The env var holds the SCRIPT PATH
// ONLY so `node "$OCTOCODE_AWARENESS_CLI" <command>` works in every shell; a
// two-token "node /path" value breaks under quoting or zsh. A broken install
// must not throw at import time and kill the whole extension load.
try {
  process.env.OCTOCODE_AWARENESS_CLI = resolveAwarenessCliPath();
} catch {
  // Awareness unresolved — leave the env var unset; prompt/status
  // surfaces fall back to the npx form.
}
// Mark this process tree as the Octocode harness so generated agent names
// (workers here, `agent join` rows in Awareness) tag as octo-* even when
// the session was launched from a Claude Code / Cursor terminal whose host
// env vars are inherited. Respect an explicit override.
process.env.OCTOCODE_AGENT_HOST ||= 'octo';
import { resolvePromptMode } from './prompt.js';
import { discoverSkills, discoverSkillStates } from './tools/skill-discovery.js';
import { writeDiscoveryFile } from './tools/discovery-file.js';
import { estimateTokens } from './utils.js';
import { getDirectToolContractStats } from './tools/octocode-tools.js';
import { resetCompactionCheckpointDedupe } from './tools/compaction-hooks.js';
import { collectPublicCommands, EXTENSION_COMMANDS } from './commands.js';
import { bindExecutionJournal, emitExecution, restoreExecutionJournal } from './tools/execution-runtime.js';
import { cleanupSpawnedAgentsForShutdown } from './tools/agents/process.js';
import { listWorkerLedgerEntries } from './tools/agents/ledger.js';
import { isSubagentProcess, pruneDroppableAgentsForSession } from './tools/agents/registry.js';
import {
  refreshAgentLedgerUi,
  setAgentLedgerMetricsRefreshForUi,
} from './tools/agents/rendering.js';
import type { JobManager } from './tools/bash-bg-tool.js';
import {
  activeSupportToolNames,
  disableBuiltinTools,
  registerSupportTools,
} from './tools/tool-registration.js';
export { disableBuiltinTools } from './tools/tool-registration.js';
import {
  clearCurrentContextSources,
  readSessionToolResult,
  registerCurrentContextSource,
  sessionToolResultOrigin,
} from './tools/context-source-registry.js';
import { applyStartupPermissionLevel, resetApprovalStore } from './tools/approval.js';
import {
  mcpCatalogReady,
  startMcpConfigWatcher,
  stopAllMcpServers,
  stopMcpConfigWatchers,
  waitForMcpShutdown,
  warmMcpCatalog,
} from './tools/mcp-tool.js';
import { initializeCapabilityAdapters, getCapabilityAdapters, disposeCapabilityAdapters } from './adapters/pi-capability-adapters.js';
import { PI_DECLARATIVE_HOOK_EVENTS } from './adapters/pi-hook-runtime.js';
import { clearSessionCapabilities } from './tools/capability-session.js';
import { disposeWorkerCapabilityRuntime, refreshCurrentWorkerCapabilities, assertCurrentWorkerNativeTool } from './tools/worker-capabilities.js';
import { openMcpManager, closeConfiguration } from './tools/mcp/html.js';
import {
  registerInteractionBrokerAdapter,
  type InteractionBrokerAdapterRegistry,
  type RegisteredInteractionBrokerAdapter,
} from './tools/interaction-broker-adapter.js';
import {
  brokerSessionId,
  clearInMemoryInteractionState,
  configureInteractionBrokerRoute,
} from './tools/interaction-broker.js';
import { claimNativeHookOwner } from '@octocodeai/octocode-awareness/host';
import { getAwarenessAgentId } from './tools/awareness-shared.js';
import { registerRuntimeUiPhase } from './tools/runtime-ui-registration.js';
import {
  activePlanScope,
  adoptPlanFromBranch,
  getPlan,
  getPlanReviewState,
  releasePlanScope,
  setPlanEntryAppender,
  PLAN_ENTRY_TYPE,
} from './tools/planning/plan-store.js';
import {
  refreshAwarenessPanel,
  suppressAwarenessPanel,
  resumeAwarenessPanel,
  clearAwarenessCacheEntry,
  setAwarenessMetricsRefreshForUi,
} from './tools/awareness-status.js';
import { deriveSessionName } from './ui-extras.js';
import { paintUi } from './tui/palette.js';
import { setUiTickSubscriber } from './tui/ui-ticker.js';
import { closeAllChromeConnections } from './chrome-connection-cache.js';
import { setPlanMetricsRefreshForUi } from './tools/planning/plan-command.js';
import { adoptPlanModePolicy, evaluateToolCapability, exitPlanMode, getPlanModePolicy } from './tools/plan-mode.js';
import { clearAllReadStates } from './tools/file-state.js';
import { registerAgentInbox, type AgentInboxRegistration } from './tools/agents/inbox.js';
import { probeGitHubAuth } from './tools/github-auth-status.js';
import { registerOctocodeAutocomplete } from './tools/autocomplete-providers.js';
import { initCheckpointStore } from './tools/checkpoints.js';
import { registerRewindCommand } from './tools/rewind-command.js';
import { createSessionArtifactContext } from './tools/session-artifacts.js';
import { freshSessionScopedState } from './session-scoped-state.js';
import {
  initializeSessionMemory,
  renderSessionArtifactPaths,
  SESSION_MEMORY_MAX_BYTES,
} from './tools/session-memory.js';
import { readSessionMemoryForContext } from './tools/session-memory-runtime.js';
import { initializeSessionIndexes } from './tools/session-index.js';
import {
  appendSessionAuditEntry,
  appendSessionAuditForContext,
  initializeSessionAudit,
} from './tools/session-audit.js';
import { cleanupEphemeralToolOutputs } from './tools/ephemeral-tool-output.js';
import { readSessionUserRequestContext, USER_REQUEST_CONTEXT_MAX_CHARS } from './tools/user-request-context.js';
import { cleanupImplicitImageArtifacts } from './tools/create-image-tool.js';
import { runAndRecordRehydration } from './tools/rehydration-orchestrator.js';
import { restoreDialOnStartup } from './tools/effort-dial.js';
import { runtimeStoreFor, setManagedActivity, setManagedStatus } from './tools/runtime-renderer.js';
import { SessionRuntime } from './session-runtime.js';
import {
  applyOctocodeUi,
  getThinkingStatus,
  OCTOCODE_BANNER_ENTRY_TYPE,
  resetOctocodeFooterRegistration,
  updateOctocodeMetricsUi,
} from './extension-ui.js';
import {
  assertSupportedPiHostVersion,
  resolvePiHostVersion,
} from './adapters/pi-host-compatibility.js';
import { createPiCanonicalRegistryComposition } from './adapters/pi-registry-adapters.js';
import { pickProvider } from './web.js';
import { createHookComposer, type HookMiddleware } from './hook-composer.js';
import { registerPiPhysiology } from './adapters/pi-physiology.js';
import { createPiAwarenessObservationSink } from './adapters/pi-awareness-observation.js';
import { createPiHistoryAdapter } from './adapters/pi-history-adapter.js';
import {
  awarenessMutationGate,
  runAwarenessMutationGate,
  updateAwarenessRegistry,
} from './adapters/pi-awareness-mutation.js';
export { readPiPhysiology } from './adapters/pi-physiology.js';
import { createPromptPreflightController } from './tools/prompt-preflight.js';
import type {PiInstance, PiContext, OctocodePiExtensionOptions, SessionShutdownEvent, ThinkingLevelEvent, NotifyFn} from './types.js';

function notify(ctx: PiContext | undefined, message: string, level = 'info'): void {
  if (level === 'error') {
    logInternalError('notify', new Error(message), { mode: ctx?.mode }, ctx);
  }

  if (ctx?.ui?.notify) {
    ctx.ui.notify(message, level);
    return;
  }

  const log = level === 'error' ? console.error : level === 'warning' ? console.warn : console.info;
  log(`[octocode:${level}] ${message}`);
}

function formatOctocodeToolStatus(): string {
  return `MCP research (octocode server) · ${activeSupportToolNames().length} support · ${OVERRIDDEN_BUILTIN_TOOL_NAMES.length} guarded built-ins · ${DISABLED_BUILTIN_TOOL_NAMES.length} replaced`;
}

export function formatStatus(baseDir?: string): string {
  const paths = getAssetPaths(baseDir);
  const skills = listBundledSkills(baseDir);
  const promptStatus = fs.existsSync(paths.systemPrompt) ? 'found' : 'missing';

  const searchProvider = pickProvider({});
  const searchKeys = ['TAVILY_API_KEY', 'TAVILY_API_TOKEN', 'SERPER_API_KEY'].filter(
    (k) => process.env[k],
  );
  const searchStatus = `${searchProvider}${searchKeys.length ? ` (keys: ${searchKeys.join(', ')})` : ' (no key — DuckDuckGo fallback)'}`;

  return [
    'Octocode Pi extension',
    `system prompt: ${promptStatus}`,
    `skills: ${skills.length}${skills.length > 0 ? ` (${skills.join(', ')})` : ''}`,
    `octocode tools: ${formatOctocodeToolStatus()}`,
    `awareness CLI: ${getAwarenessCLIPath()} — user CLI: npx -p @octocodeai/octocode-awareness octocode-awareness <concept> <operation> --workspace "$PWD"`,
    `management CLI: npx octocode skill | lsp-server | auth (no bundled CLI — use npx octocode for management tasks)`,
    `disabled/replaced built-ins: overridden: ${OVERRIDDEN_BUILTIN_TOOL_NAMES.join(', ')}${DISABLED_BUILTIN_TOOL_NAMES.length ? `; removed: ${DISABLED_BUILTIN_TOOL_NAMES.join(', ')}` : ''}`,
    `web search: ${searchStatus}`,
    `internal error log: ${getInternalErrorLogPath(process.cwd())}`,
    `package assets: ${paths.baseDir}`,
    `flags: --no-context (suppress project context files for this run)`,
  ].join('\n');
}

/**
 * Approximate per-turn prompt cost of each Octocode system-prompt addition.
 * Compaction is budget: this makes the "helpful default prompt inventory"
 * (static prompt, MCP catalog, skills, dynamic capabilities, active plan)
 * visible so oversized blocks can be spotted. ~4 chars/token heuristic.
 */
export function formatPromptBudget(parts: Array<{ label: string; text: string }>): string {
  const est = (chars: number): string => `${chars} chars (~${estimateTokens(chars)} tokens)`;
  const lines = parts.map((part) =>
    `- ${part.label}: ${part.text.trim().length === 0 ? '(empty)' : est(part.text.length)}`,
  );
  const total = parts.reduce((sum, part) => sum + part.text.length, 0);
  return [
    'Prompt budget (per-turn Octocode system-prompt additions; ~4 chars/token):',
    ...lines,
    `- total: ${est(total)}`,
  ].join('\n');
}

export interface ExtensionHarness {
  tools: string[];
  supportTools: string[];
  overriddenBuiltins: string[];
  disabledBuiltins: string[];
  passthroughBuiltins: string[];
  extensionCommands: string[];
  skills: string[];
  cliNote: string;
  awarenessCliNote: string;
}

export function listExtensionHarness(baseDir?: string): ExtensionHarness {
  return {
    tools: [], // research tools served via MCPTool → octocode MCP server
    supportTools: [...activeSupportToolNames()],
    overriddenBuiltins: [...OVERRIDDEN_BUILTIN_TOOL_NAMES],
    disabledBuiltins: [...DISABLED_BUILTIN_TOOL_NAMES],
    passthroughBuiltins: [],
    extensionCommands: Object.values(EXTENSION_COMMANDS).map(command => `/${command.name}`),
    skills: listBundledSkills(baseDir),
    cliNote: `management: npx octocode skill | lsp-server | auth (no bundled CLI — use npx octocode for management tasks)`,
    awarenessCliNote: `Awareness CLI: ${getAwarenessCLIPath()}; user CLI: npx -p @octocodeai/octocode-awareness octocode-awareness <concept> <operation> --workspace "$PWD"`,
  };
}

function existingDirectory(filePath: string): string | null {
  return fs.existsSync(filePath) ? filePath : null;
}

// ─── Pi wiring ────────────────────────────────────────────────────────────────


interface TurnMetricsRegistrationArgs {
  pi: PiInstance;
  startMetricsTicker: (ctx: PiContext | undefined) => void;
  stopMetricsTicker: () => void;
  toolStartTimes: Map<string, number>;
  toolInputs: Map<string, unknown>;
}

function registerTurnMetricsPhase({ pi, startMetricsTicker, stopMetricsTicker, toolStartTimes, toolInputs }: TurnMetricsRegistrationArgs): void {
  if (typeof pi.on !== 'function') return;
  pi.on('turn_start', async (_event: unknown, ctx: PiContext) => {
    updateOctocodeMetricsUi(ctx);
    startMetricsTicker(ctx); // live `active`/`session` durations during the turn
  });
  pi.on('turn_end', async (_event: unknown, ctx: PiContext) => {
    stopMetricsTicker();
    // Evict timing entries for tools whose tool_execution_end never fired
    // (aborted turns) — the map otherwise grows for the session lifetime.
    toolStartTimes.clear();
    toolInputs.clear();
    updateOctocodeMetricsUi(ctx);
  });
}

interface WorkerToolRegistrationArgs {
  pi: PiInstance;
  registeredToolNames: Set<string>;
  notify: NotifyFn;
}

function registerWorkerToolPhase({ pi, notify }: WorkerToolRegistrationArgs): AgentInboxRegistration {
  setAgentLedgerMetricsRefreshForUi((ctx) => updateOctocodeMetricsUi(ctx));
  setPlanMetricsRefreshForUi((ctx) => updateOctocodeMetricsUi(ctx));
  setAwarenessMetricsRefreshForUi((ctx) => updateOctocodeMetricsUi(ctx));

  // Worker inbox overlay (/octocode-inbox) + desktop notifications. The unified
  // agent facade initializes the shared ledger runtime during support-tool setup.
  return registerAgentInbox(pi, notify);
}

async function wireOctocodePiExtension(
  pi: PiInstance,
  opts: { promptMode: PromptMode },
): Promise<void> {
  pi = createPiCanonicalRegistryComposition(pi).pi;
  const { promptMode } = opts;
  // One active session per extension instance. `session` is replaced wholesale on
  // session_start; see SessionScopedState for what that boundary guarantees.
  let session = freshSessionScopedState();
  // Live footer ticker: while a turn is active, re-render the footer every second
  // so `active`/`session` durations advance. Awareness refreshes asynchronously
  // behind its own throttle; git is refreshed on boundaries. Runs on the shared ui-ticker
  // clock so this and the agent-ledger refresh never double-render the footer
  // from two out-of-phase timers.
  const METRICS_TICK_KEY = 'octocode-metrics';
  const stopMetricsTicker = (): void => setUiTickSubscriber(METRICS_TICK_KEY, undefined);
  // No self-stop guard needed: every site that clears activeTurnStartedAt
  // (turn_end, session_start, session_shutdown) also calls stopMetricsTicker, so
  // the ticker is never left subscribed against an inactive turn.
  const startMetricsTicker = (ctx: PiContext | undefined): void =>
    setUiTickSubscriber(METRICS_TICK_KEY, () => {
      refreshAwarenessPanel(ctx);
      updateOctocodeMetricsUi(ctx);
    });
  const toolStartTimes = new Map<string, number>();
  const toolInputs = new Map<string, unknown>();
  let providerRequestStartedAt: number | undefined;
  // Agent inbox handle: assigned during tool registration, referenced by the
  // session_shutdown hook — its suppress flag must flip BEFORE
  // cleanupSpawnedAgentsForShutdown() kills workers, or the teardown burst of
  // killed/exit ledger events would spam desktop notifications.
  let agentInbox: AgentInboxRegistration | undefined;
  let bashBgManager: JobManager | undefined;
  let sessionRuntime: SessionRuntime | undefined;
  let pendingMcpDiscoveryWrite: Promise<void> | undefined;
  let interactionBrokerAdapter: RegisteredInteractionBrokerAdapter | undefined;
  const hostBrokerRegistry = pi as PiInstance & Partial<InteractionBrokerAdapterRegistry>;
  const hasHostInteractionAnswerRoute = typeof hostBrokerRegistry.registerInteractionBrokerAdapter === 'function';
  registerInteractionBrokerAdapter({
    registerInteractionBrokerAdapter: (adapter) => {
      interactionBrokerAdapter = adapter;
      // This is a host-only capability boundary. It is deliberately not
      // registered as a model tool: only a trusted RPC/UI host may submit the
      // user's answer, after which it calls adapter.drain(ctx).
      hostBrokerRegistry.registerInteractionBrokerAdapter?.(adapter);
    },
  }, {
    deliver: (_continuation, prompt) => {
      pi.sendUserMessage(prompt, { deliverAs: 'followUp' });
    },
  });
  // Model-callable tool names, shared between registration (uniqueness check)
  // and the discovery-file inventory. Builtin overrides register through the
  // same helper as support tools, so no manual pre-seeding is needed.
  const registeredToolNames = new Set<string>();
  registerRewindCommand(pi, {
    getEngine: ctx => isPersistentStorageEnabled()
      ? initCheckpointStore(ctx?.cwd ?? process.cwd(), { agentId: getAwarenessAgentId(ctx) })
      : undefined,
    notify,
  });
  let latestSessionCwd: string | undefined;

  // Register --no-context CLI flag before any session starts so Pi can parse it.
  // default:false → context files load normally (octocode-agent launcher already
  // passes --no-context-files at the pi CLI level for its own sessions).
  // Pass --no-context to suppress AGENTS.md / CLAUDE.md for any single run.
  pi.registerFlag?.('no-context', {
    description: 'Suppress AGENTS.md / CLAUDE.md context files from the system prompt',
    type: 'boolean',
    default: false,
  });

  // Best-effort early disable so weak builtins are absent immediately on load.
  // Real Pi runtimes also re-run this in session_start and after tool registration
  // — the calls are idempotent.
  disableBuiltinTools(pi);

  if (typeof (pi as { on?: unknown }).on === 'function') {
    const hooks = createHookComposer(pi, {
      onError: (error, event, middleware, args) => {
        const ctx = args[1] as PiContext | undefined;
        const shutdownEvent = event === 'session_shutdown' ? args[0] as SessionShutdownEvent | undefined : undefined;
        const contextIsStale = shutdownEvent !== undefined && shutdownEvent.reason !== 'quit';
        const safeCtx = contextIsStale ? undefined : ctx;
        logInternalError('hook', error, { event, middleware }, safeCtx);
        if (!contextIsStale) {
          notify(safeCtx, `Octocode hook ${event}/${middleware} failed: ${(error as Error)?.message ?? String(error)}`, 'warning');
        }
      },
    });

    const physiologyObservationSink = createPiAwarenessObservationSink({
      onError: error => logInternalError('runtime-observation', error),
    });
    const physiology = registerPiPhysiology({
      on(event, handler) { hooks.on(event, 'octocode-physiology', handler as HookMiddleware); },
    }, { onObservation: physiologyObservationSink });
    const promptPreflight = createPromptPreflightController({
      pi,
      promptMode,
      notify,
      getSession: () => session,
      getFallbackTools: () => [...activeSupportToolNames()],
      readPhysiology: ctx => physiology.read(ctx),
    });
    const localHistory = createPiHistoryAdapter({ onError: error => logInternalError('local-history', error) });

    hooks.on('agent_start', 'octocode-prompt-preflight', promptPreflight.guardAgentStart);

    hooks.on('resources_discover', 'bundled-skills', async () => {
      if (isSubagentProcess()) return {};
      const paths = getAssetPaths();
      const skillPath = existingDirectory(paths.skillsDir);
      return skillPath ? { skillPaths: [skillPath] } : {};
    });

    hooks.on('tool_call', 'octocode-plan-mode-audit', async (event: { toolName?: string; input?: Record<string, unknown> }, ctx: PiContext | undefined) => {
      if (isSubagentProcess()) {
        try {
          await refreshCurrentWorkerCapabilities();
          assertCurrentWorkerNativeTool(event.toolName ?? '');
        } catch (error) {
          return { block: true, reason: error instanceof Error ? error.message : String(error) };
        }
      }
      const policy = getPlanModePolicy(ctx);
      const receipt = evaluateToolCapability({ toolName: event.toolName, toolInput: event.input, ...(policy ? { phase: policy.phase } : {}) });
      if (!process.env['VITEST']) {
        try {
          const awareness = openPersistentAwareness({ workspace: ctx?.cwd ?? process.cwd() });
          try { awareness.recordCapabilityReceipt(receipt); } finally { awareness.close(); }
        } catch { /* audit persistence cannot weaken the synchronous deny decision */ }
      }
      return undefined;
    });

    hooks.on('tool_call', 'awareness-lock-gate', async (event: { toolCallId: string; toolName: string; input: Record<string, unknown> }, ctx: PiContext | undefined) => {
      const decision = await runAwarenessMutationGate(event, ctx);
      if (decision?.block) return decision;
      await localHistory.before(event, ctx);
      return decision;
    });
    hooks.on('tool_execution_end', 'awareness-history-after', async (event: { toolCallId: string; toolName: string; result: unknown; isError: boolean }, ctx: PiContext | undefined) => {
      await localHistory.after(event, ctx);
    });

    // Snapshot every plan mutation into a session CustomEntry (state channel —
    // never rendered, never in LLM context) so /fork and /tree roll plan state
    // back with the conversation instead of leaking the forked-from plan.
    setPlanEntryAppender((steps, rfcPath, decisions, lifecycle, review, coordination, meta, cleared) => pi.appendEntry?.(PLAN_ENTRY_TYPE, { version: 4, cleared, ...review, snapshotId: meta.snapshotId, branchSnapshotId: meta.snapshotId, generation: meta.generation, capturedAt: meta.capturedAt, updatedAt: meta.capturedAt, steps, phase: lifecycle, coordination, ...(rfcPath ? { rfcPath } : {}), ...(decisions && decisions.length ? { decisions } : {}) }));

    hooks.on('session_tree', 'octocode-plan-tree-sync', async (_event: unknown, ctx: PiContext | undefined) => {
      // /tree navigation moved the leaf — re-adopt the plan snapshot that was
      // current on the new branch, and re-render the panel with it.
      if (ctx) restoreExecutionJournal(ctx);
      const scope = activePlanScope(ctx);
      const adopted = adoptPlanFromBranch(scope, ctx?.sessionManager?.getBranch?.() ?? [], { clearWhenMissing: true });
      if (adopted) adoptPlanModePolicy(ctx, getPlanReviewState(scope));
      else exitPlanMode(ctx);
      if (ctx) runAndRecordRehydration(pi, ctx, 'tree');
      updateOctocodeMetricsUi(ctx);
    });

    const disposeSessionResources = async (reason: string, ctx: PiContext | undefined): Promise<void> => {
      await getCapabilityAdapters(ctx)?.hooks.dispatch('session_shutdown', { reason }, ctx);
      disposeCapabilityAdapters(ctx);
      appendSessionAuditForContext(ctx, { event: 'session.shutdown', detail: { reason } });
      const canUseShutdownContext = reason === 'quit';
      awarenessMutationGate.cleanup();
      updateAwarenessRegistry('leave', undefined, latestSessionCwd);
      stopMcpConfigWatchers();
      closeConfiguration(ctx);
      stopMetricsTicker();
      runtimeStoreFor(ctx)?.getState().setFooter({ activeTurnStartedAt: undefined });
      suppressAwarenessPanel();
      agentInbox?.shutdown({ restoreTitle: canUseShutdownContext });
      setAgentLedgerMetricsRefreshForUi(undefined);
      setPlanMetricsRefreshForUi(undefined);
      setAwarenessMetricsRefreshForUi(undefined);
      // Fix 2: clear this session’s registered context sources by ctx identity.
      // The no-ctx clear-all that used to live in compaction-hooks’ session_shutdown
      // handler races with a concurrently starting session, so we clear only the
      // shutting-down session’s entry here, where the ctx is known.
      if (ctx) {
        clearCurrentContextSources(ctx);
        releasePlanScope(activePlanScope(ctx));
      }
      const cleanedAgents = cleanupSpawnedAgentsForShutdown();
      await disposeWorkerCapabilityRuntime();
      clearSessionCapabilities(ctx?.cwd ?? process.cwd());
      const stoppedMcpServers = stopAllMcpServers();
      await waitForMcpShutdown();
      await pendingMcpDiscoveryWrite?.catch(() => undefined);
      pendingMcpDiscoveryWrite = undefined;
      const closedChrome = closeAllChromeConnections();
      if (closedChrome > 0 && canUseShutdownContext) notify(ctx, `Closed ${closedChrome} cached CDP connection(s).`, 'info');
      const interactionWorkspace = ctx?.cwd ?? latestSessionCwd;
      if (interactionWorkspace) {
        clearInMemoryInteractionState({
          workspace: interactionWorkspace,
          ...(ctx ? { sessionId: brokerSessionId(ctx) } : {}),
        });
      }
      latestSessionCwd = undefined;
      resetOctocodeFooterRegistration(ctx);
      if (canUseShutdownContext && ctx?.hasUI) {
        if (cleanedAgents > 0) ctx.ui?.notify?.(`Octocode closed ${cleanedAgents} spawned subagent(s).`, 'info');
        if (stoppedMcpServers > 0) ctx.ui?.notify?.(`Octocode stopped ${stoppedMcpServers} MCP server(s).`, 'info');
      }
    };

    const initializeOctocodeSession = async (ctx: PiContext | undefined, reason?: string): Promise<void> => {
      if (ctx) configureInteractionBrokerRoute(ctx, hasHostInteractionAnswerRoute);
      session = freshSessionScopedState();
      await sessionRuntime?.dispose('replace');
      const runtime = new SessionRuntime({ ctx, onDispose: (reason) => disposeSessionResources(reason ?? 'shutdown', ctx) });
      runtime.addCleanup(() => bashBgManager?.dispose());
      sessionRuntime = runtime;
      if (ctx) {
        try {
          bindExecutionJournal(pi, ctx);
        } catch (error) {
          runtime.store.getState().failed(error);
          throw error;
        }
      }
      const runtimeStore = runtime.store;
      const initializationTasks: Promise<unknown>[] = [];
      // Environment is a prerequisite for every process/config consumer, notably
      // MCP discovery. It must run before any server warm starts.
      await runtime.runTask({
        name: 'environment',
        message: 'loading configuration',
        critical: true,
        readyMessage: 'configuration loaded',
        run: async () => {
        const trusted = ctx?.isProjectTrusted ? Boolean(await ctx.isProjectTrusted()) : false;
        const { applied, skippedProtected } = propagateOctocodeEnv({
          home: getOctocodeHome(),
          cwd: ctx?.cwd ?? process.cwd(),
          trusted,
        });
        if (applied.length > 0) notify(ctx, `Octocode env: ${applied.join(', ')}`, 'info');
        if (skippedProtected.length > 0) {
          notify(ctx, `Octocode env: skipped protected key(s): ${skippedProtected.join(', ')}.`, 'warning');
        }
        },
      });
      runtimeStore.getState().setStage('restoring session');
      initializeCapabilityAdapters(ctx);
      // Undo the shutdown-time suppression from a previous session in this process.
      resumeAwarenessPanel();
      // Re-arm worker desktop notifications: the inbox is registered once per
      // process and session_shutdown suppresses + detaches its ledger listener,
      // so without this resume a single /new or /resume kills notifications for
      // the rest of the process (mirrors the two panel resumes above).
      agentInbox?.resume();
      // Auto-naming is a per-session, once-per-session action. Seed the flag from
      // whether this session already has a name: a fresh /new session has none →
      // its first prompt names it; a resumed/forked already-named session keeps
      // its name and skips renaming. Without this reset the flag stayed true from
      // session 1 and no later session was ever auto-named.
      sessionAutoNamed = Boolean(pi.getSessionName?.());
      // Re-register the footer for THIS session's ctx/tui/theme (idempotent
      // registration is keyed by ctx; deleting here forces exactly one
      // re-registration per session, e.g. after /new or a theme change).
      resetOctocodeFooterRegistration(ctx);
      setAgentLedgerMetricsRefreshForUi((ctx) => updateOctocodeMetricsUi(ctx));
      setPlanMetricsRefreshForUi((ctx) => updateOctocodeMetricsUi(ctx));
      setAwarenessMetricsRefreshForUi((ctx) => updateOctocodeMetricsUi(ctx));
      // Read-states recorded in a previous session must not satisfy the edit
      // tool's stale-read gate in this one, and the auto-compaction edge
      // trigger must not carry the old session's threshold crossing.
      clearAllReadStates();
      if (ctx) {
        registerCurrentContextSource(ctx, {
          version: 1, id: 'user-request-history', kind: 'user-request',
          origin: 'session-user:history', authority: 'user', scope: 'task',
          visibility: 'transcript', rehydrate: 'always',
          tokenBudget: Math.ceil(USER_REQUEST_CONTEXT_MAX_CHARS / 4),
          readCurrent: readSessionUserRequestContext,
        });
        try {
          const artifacts = createSessionArtifactContext(ctx);
          session.sessionArtifactContext = artifacts;
          initializeSessionIndexes(artifacts);
          const memoryPath = initializeSessionMemory(artifacts);
          const auditPath = initializeSessionAudit(artifacts);
          session.sessionArtifactPathsContext = renderSessionArtifactPaths({ memoryPath, auditPath });
          registerCurrentContextSource(ctx, {
            version: 1,
            id: 'session-memory',
            kind: 'memory-lead',
            origin: 'session-memory',
            authority: 'external-data',
            scope: 'session',
            visibility: 'inspectable',
            rehydrate: 'always',
            tokenBudget: Math.ceil(SESSION_MEMORY_MAX_BYTES / 4),
            readCurrent: () => readSessionMemoryForContext(ctx, artifacts),
          });
          appendSessionAuditEntry(artifacts, {
            event: 'session.start',
            detail: { reason: reason ?? 'new' },
          });
        } catch {
          // Session artifacts are continuity aids; initialization must not block Pi startup.
        }
      }
      // A new session gets a fresh checkpoint-card dedupe set. Pi owns all
      // compaction retry/continuation state; Octocode keeps no parallel arbiter.
      resetCompactionCheckpointDedupe();
      // Sensitive-action "always allow" consent is session-scoped: a new session
      // must re-earn it, never inherit a prior session's approvals.
      resetApprovalStore(ctx);
      // Operator/CI can pin the session's starting level (strict|default|relaxed).
      applyStartupPermissionLevel(ctx);
      // One banner card per FRESH session. "Fresh" = no conversation yet: the
      // branch is NEVER empty at session_start (pi already appended
      // model_change / thinking_level_change entries), so test for the absence
      // of `message` entries — and of a prior banner, so /resume never doubles it.
      const sessionBranch = (ctx?.sessionManager?.getBranch?.() ?? []) as Array<{ type?: string; customType?: string }>;
      const hasConversation = sessionBranch.some((e) => e?.type === 'message');
      const hasBannerEntry = sessionBranch.some(
        (e) => e?.type === 'custom' && e?.customType === OCTOCODE_BANNER_ENTRY_TYPE,
      );
      if (ctx?.hasUI && typeof pi.registerEntryRenderer === 'function' && !hasConversation && !hasBannerEntry) {
        pi.appendEntry?.(OCTOCODE_BANNER_ENTRY_TYPE, {
          model: ctx?.model?.id,
          provider: ctx?.model?.provider,
          thinking: pi.getThinkingLevel?.(),
        });
      }
      // Force a fresh Awareness poll: never paint a prior session's cached status for this cwd.
      if (ctx?.cwd) clearAwarenessCacheEntry(ctx.cwd);
      // Drop dead worker records so the agent ledger reflects only this session.
      pruneDroppableAgentsForSession();
      runtimeStore.getState().setFooter({
        // The execution journal already restored the session clock and counts.
        githubAuth: { status: 'checking' },
        usage: undefined,
      });
      stopMetricsTicker();
      latestSessionCwd = ctx?.cwd;
      // Branch-correct plan state: adopt the newest octocode-plan snapshot on
      // this session's branch (pi copies entries up to the fork point, so a
      // fork restores exactly the plan that existed there). clearWhenMissing
      // ensures branches without a snapshot clear any stale fallback-scoped
      // plan from a prior session rather than leaving orphaned state.
      const planScope = activePlanScope(ctx);
      const adoptedPlan = adoptPlanFromBranch(planScope, ctx?.sessionManager?.getBranch?.() ?? [], { clearWhenMissing: true, fork: reason === 'fork' });
      if (adoptedPlan || getPlan(planScope).length > 0) adoptPlanModePolicy(ctx, getPlanReviewState(planScope));
      else exitPlanMode(ctx);
      if (ctx) runAndRecordRehydration(pi, ctx, reason ?? 'new');
      // Answers accepted by a headless/RPC host survive process restarts in the
      // broker outbox. Resume them at the first session boundary; failed sends
      // remain unacknowledged and will be retried with the same continuationId.
      if (ctx) await interactionBrokerAdapter?.drain(ctx);
      // Re-apply the persisted effort dial (thinking level + worker cap) before
      // the footer renders so `◉ <level>` is correct from the first frame.
      await restoreDialOnStartup(pi, ctx);
      // Editor autocomplete for @worker/@skill and #plan-step mentions. The
      // registration is internally once-per-process (pi has no removal API).
      if (ctx?.ui) {
        registerOctocodeAutocomplete(ctx.ui, {
          listWorkers: () => listWorkerLedgerEntries(),
          getPlanSteps: () => getPlan(activePlanScope(ctx)),
                    listSkills: () => discoverSkills(sessionCwd, session.latestAvailableSkills),
        });
      }
      applyOctocodeUi(ctx, pi.getThinkingLevel?.());
      // Context is not measurable until before_agent_start provides Pi's base
      // prompt and project context. Publish an explicit pending state instead of
      // showing a misleading partial total during initialization.
      if (session.cachedSystemPromptText === null) {
        session.cachedSystemPromptText = readTextIfExists(getAssetPaths().systemPrompt);
      }
      const directToolStats = getDirectToolContractStats(new Set(pi.getActiveTools?.() ?? registeredToolNames));
      runtimeStore.getState().setContext({
        status: 'pending',
        mode: 'routing',
        directToolChars: directToolStats.totalChars,
      });
      updateOctocodeMetricsUi(ctx);
      // Credential resolution belongs to Octocode (env → Octocode storage → gh CLI).
      // Probe once per session without delaying startup, and ignore stale results after
      // /new, /resume, /fork, reload, or shutdown.
      initializationTasks.push(runtime.runTask({
        name: 'github-auth',
        message: 'checking GitHub authentication',
        readyMessage: 'GitHub authentication checked',
        run: () => probeGitHubAuth(pi.exec?.bind(pi)),
      }).then((authState) => {
        if (!authState) return;
        if (!runtime.isCurrent()) return;
        runtimeStore.getState().setFooter({ githubAuth: authState });
        updateOctocodeMetricsUi(ctx);
      }));
      // Announce this session in the shared Awareness agent registry with
      // its name and provider. Peers discover it through context.orient and
      // communicate with message.send.
      updateAwarenessRegistry('join', ctx);
      // Full MCP discovery at init: connect every enabled configured server and
      // cache only enabled tools with descriptions and exact input schemas.
      // Fire-and-forget here; before_agent_start awaits it (bounded) so turn 1's
      // system prompt already carries the catalog. Once discovery lands, write
      // the machine-readable inventory (.octocode/discovery.json): all skills +
      // full MCP configuration + native tool surface, for users/peer agents.
      const sessionCwd = ctx?.cwd ?? process.cwd();
      runtimeStore.getState().setStage('loading MCP catalog');
      const liveMcpWarm = warmMcpCatalog(ctx, runtime.signal);
      initializationTasks.push(runtime.runTask({
        name: 'mcp',
        message: 'loading MCP catalog',
        readyMessage: 'MCP catalog ready',
        run: async () => {
          if (!await mcpCatalogReady(ctx)) throw new Error('MCP prompt catalog was not ready before the startup deadline');
        },
      }));
      pendingMcpDiscoveryWrite = liveMcpWarm.then(() => {
        // The old warm may settle after /new invalidates its ctx. Shutdown and
        // the next session both advance this generation before microtasks resume.
        if (!runtime.isCurrent()) return;
        const liveMcpState = runtimeStore.getState().mcp;
        if (liveMcpState.status === 'degraded' || liveMcpState.status === 'failed') {
          runtimeStore.getState().degradeTask('mcp', liveMcpState.message ?? 'MCP live refresh failed');
        }
        writeDiscoveryFile(ctx, {
          skills: discoverSkillStates(sessionCwd, session.latestAvailableSkills),
          nativeTools: [...registeredToolNames],
        });
      }).catch((error) => {
        logInternalError('mcp-discovery-write', error, {}, ctx);
      });
      // Check for a newer @octocodeai/pi-extension on npm — fire-and-forget, never
      // awaited before the session becomes usable, matching how Pi checks its own
      // version and installed packages (interactive-mode.js#run). Interactive-only:
      // Pi's own checks never run in print/rpc mode either, and ctx.hasUI is false
      // there, so this also skips the npm-view subprocess entirely for scripted use.
      if (ctx?.hasUI && process.env['NODE_ENV'] !== 'test' && !process.env['VITEST']) {
        initializationTasks.push(runtime.runTask({
          name: 'update-check',
          message: 'checking for updates',
          readyMessage: 'update check complete',
          run: () => checkForCoreUpdate(readOwnVersion(getAssetPaths().baseDir)),
        }).then((update) => {
          if (!update || !runtime.isCurrent()) return;
          notify(
            ctx,
            `@octocodeai/pi-extension ${update.latestVersion} is available (current: ${update.currentVersion}). Run: pi update ${getInstallSource()}`,
            'info',
          );
        }));
      }
      // Interactive sessions watch mcp.json for connection/catalog invalidation. Headless
      // sessions intentionally avoid long-lived filesystem resources; each MCP action still
      // resolves the current configuration. Prompt changes take effect on the next session.
      if (ctx?.hasUI && process.env['NODE_ENV'] !== 'test' && !process.env['VITEST']) {
        try {
          const watched = startMcpConfigWatcher(ctx, notify);
          if (watched > 0) notify(ctx, `Octocode watching mcp.json for live changes; use /new after catalog changes to refresh the agent prompt.`, 'info');
        } catch (error) {
          notify(ctx, `Octocode MCP config watcher failed to start: ${(error as Error)?.message ?? String(error)}`, 'warning');
        }
      }
      // Disable replaced built-ins: research uses Octocode MCP and mutations use file.
      try {
        if (disableBuiltinTools(pi)) {
          notify(
            ctx,
            `Octocode disabled Pi built-ins (${DISABLED_BUILTIN_TOOL_NAMES.join(', ')}); select a research tool from <mcp_catalog_index>, load it with MCPTool action:describe, then call the returned exact-schema Pi tool. Overrides: ${OVERRIDDEN_BUILTIN_TOOL_NAMES.join(', ')}.`, 
            'info',
          );
        }
      } catch (error) {
        notify(
          ctx,
          `Octocode could not disable Pi built-ins: ${(error as Error)?.message ?? String(error)}`,
          'warning',
        );
      }
      await Promise.allSettled(initializationTasks);
      if (!runtime.isCurrent()) return;
      const degradedTasks = Object.values(runtimeStore.getState().tasks)
        .filter((task) => task.status === 'degraded' || task.status === 'failed').length;
      const mcp = runtimeStore.getState().mcp;
      const mcpSummary = mcp.status === 'ready'
        ? ` · MCP ${mcp.servers} server${mcp.servers === 1 ? '' : 's'} · ${mcp.tools} tools${mcp.source === 'cache' ? ' · cached' : ''}`
        : ' · MCP loading in background';
      runtime.settleInitialization({
        readyMessage: `Octocode ready${mcpSummary}`,
        degradedMessage: `Octocode ready with ${degradedTasks} warning${degradedTasks === 1 ? '' : 's'}${mcpSummary}`,
      });
    };

    hooks.on('session_start', 'octocode-session-start', async (event: { reason?: string }, ctx: PiContext | undefined) => {
      try {
        claimNativeHookOwner({ workspace: ctx?.cwd ?? process.cwd(), host: 'pi' });
        await initializeOctocodeSession(ctx, event?.reason);
      } catch (error) {
        sessionRuntime?.store.getState().failed(error);
        throw error;
      }
    });

    // Clean up status labels and spawned workers when the session tears down
    // so they don't leak across /new, /resume, /fork, reload, or quit.
    hooks.on('session_shutdown', 'octocode-session-shutdown', async (event: SessionShutdownEvent, _ctx: PiContext | undefined) => {
      try {
        if (sessionRuntime && event.reason === 'quit') emitExecution(_ctx, 'session.completed', { reason: event.reason }, 'debug');
        await sessionRuntime?.dispose(event.reason);
      } finally {
        sessionRuntime = undefined;
        cleanupEphemeralToolOutputs();
        // Harness-persisted inline-display fallbacks are session-scoped; explicit
        // saveTo output is never tracked and therefore survives.
        cleanupImplicitImageArtifacts();
      }
    });

    hooks.on('model_select', 'octocode-model-select', async (_event: unknown, ctx: PiContext | undefined) => {
      updateAwarenessRegistry('join', ctx);
      // thinking_level_select fires before model_select when the model change
      // clamps the thinking level, so pi.getThinkingLevel() is already updated.
      applyOctocodeUi(ctx, pi.getThinkingLevel?.());
      // refreshAgentLedgerUi refreshes the footer metrics too (via the wired
      // refresher), so calling updateOctocodeMetricsUi here built the footer
      // twice per model switch.
      refreshAgentLedgerUi(ctx);
    });

    hooks.on('session_info_changed', 'octocode-awareness-name-refresh', async (_event: unknown, ctx: PiContext | undefined) => {
      updateAwarenessRegistry('join', ctx);
    });

    hooks.on('thinking_level_select', 'octocode-thinking-select', async (event: ThinkingLevelEvent, ctx: PiContext | undefined) => {
      applyOctocodeUi(ctx, event.level);
      updateOctocodeMetricsUi(ctx);
    });

    // agent_settled fires once after ALL retries, auto-compaction retries, and
    // queued follow-up messages complete — a more precise "agent is done" signal
    // than agent_end (which fires per-run, possibly before a continuation starts).
    // Use it as a definitive safety net to clear the active-turn indicator.
    hooks.on('agent_settled', 'octocode-agent-settled', async (_event: unknown, ctx: PiContext | undefined) => {
      runtimeStoreFor(ctx)?.getState().setFooter({ activeTurnStartedAt: undefined });
      updateOctocodeMetricsUi(ctx);
    });

    let sessionAutoNamed = false;
    hooks.on('input', 'octocode-session-autoname', async (event: { text: string; source?: string; streamingBehavior?: string }, ctx: PiContext | undefined) => {
      // Name the session from the first real user prompt so /resume, the session
      // picker, and the terminal title are readable. Skip steering/extension input.
      if (event.source === 'extension' || event.streamingBehavior === 'steer') return { action: 'continue' as const };
      if (sessionAutoNamed) return { action: 'continue' as const };
      const name = deriveSessionName(event.text ?? '');
      if (name) applyOctocodeUi(ctx, pi.getThinkingLevel?.(), name);
      sessionAutoNamed = true;
      try {
        if (!pi.getSessionName?.() && name) pi.setSessionName?.(name);
      } catch { /* naming is best-effort */ }
      return { action: 'continue' as const };
    });

    hooks.on('tool_execution_start', 'octocode-tool-error-timing', async (event: { toolCallId?: string; toolName?: string; args?: unknown }) => {
      const key = event.toolCallId ?? event.toolName;
      if (key) {
        toolStartTimes.set(key, Date.now());
        toolInputs.set(key, event.args);
      }
    });

    hooks.on('tool_execution_end', 'octocode-tool-error-log', async (event: { toolCallId?: string; toolName?: string; result?: unknown; isError?: boolean }, ctx: PiContext | undefined) => {
      const key = event.toolCallId ?? event.toolName;
      const startedAt = key ? toolStartTimes.get(key) : undefined;
      if (key) toolStartTimes.delete(key);
      const toolInput = key ? toolInputs.get(key) : undefined;
      if (key) toolInputs.delete(key);
      awarenessMutationGate.complete(
        {
          toolName: event.toolName,
          input: toolInput && typeof toolInput === 'object'
            ? toolInput as Record<string, unknown>
            : {},
        },
        ctx?.cwd ?? process.cwd(),
        getAwarenessAgentId(ctx),
        !event.isError,
      );
      // Re-open the bash suppression window at completion too: a long-running
      // bash command's fs churn lands at the end of the call, not the start.
      if (!event.isError && ctx && event.toolCallId && event.toolName) {
        const input = toolInput && typeof toolInput === 'object' ? toolInput as Record<string, unknown> : {};
        const queries = Array.isArray(input['queries']) ? input['queries'] as Array<Record<string, unknown>> : [];
        const memoryRecall = event.toolName === 'memory' && queries.some((query) => query['action'] === 'recall');
        const resultKind = memoryRecall ? 'memory-lead' : 'tool-result';
        const callId = event.toolCallId;
        registerCurrentContextSource(ctx, {
          version: 1,
          id: `${resultKind}:${callId}`,
          kind: resultKind,
          origin: sessionToolResultOrigin(callId),
          authority: 'external-data',
          scope: 'task',
          visibility: 'inspectable',
          rehydrate: resultKind === 'memory-lead' ? 'on-trigger' : 'summary-only',
          capture: false,
          readCurrent: (current) => readSessionToolResult(current, callId),
        });
        if (event.toolName === 'skill') {
          const requested = queries.find((query) => query['type'] === 'load' || query['action'] === 'load')?.['name'];
          if (typeof requested === 'string') {
            const skill = session.latestAvailableSkills?.find((candidate) => candidate.name.toLowerCase() === requested.trim().toLowerCase());
            if (skill) promptPreflight.registerSelectedSkillContext(ctx, skill);
          }
        }
        return;
      }
      if (!event.isError) return;
      logInternalError('tool_execution_end', new Error(`Tool ${event.toolName ?? 'unknown'} failed`), {
        toolCallId: event.toolCallId,
        toolName: event.toolName,
        durationMs: startedAt === undefined ? undefined : Date.now() - startedAt,
        result: event.result,
      }, ctx, { severity: 'warning', stack: false });
    });

    hooks.on('before_provider_request', 'octocode-provider-error-timing', async () => {
      providerRequestStartedAt = Date.now();
    });

    hooks.on('after_provider_response', 'octocode-provider-error-log', async (event: { status?: number; headers?: Record<string, string> }, ctx: PiContext | undefined) => {
      const status = Number(event.status);
      const durationMs = providerRequestStartedAt === undefined ? undefined : Date.now() - providerRequestStartedAt;
      providerRequestStartedAt = undefined;
      if (!Number.isFinite(status) || status < 400) return;
      logInternalError('after_provider_response', new Error(`Provider response HTTP ${status}`), {
        status,
        durationMs,
        headers: event.headers,
      }, ctx);
    });

    hooks.on('before_agent_start', 'octocode-system-prompt', promptPreflight.prepare);
    for (const event of PI_DECLARATIVE_HOOK_EVENTS) {
      if (event !== 'session_shutdown') hooks.on(event, 'octocode-declarative-hooks', (payload: unknown, ctx: PiContext | undefined) => getCapabilityAdapters(ctx)?.hooks.dispatch(event, payload, ctx));
    }
  }

  if (pi.registerTool) {
    bashBgManager = registerSupportTools({
      pi,
      registeredToolNames,
      notify,
      getPiSkills: () => session.latestPiSkills,
    });
    registerRuntimeUiPhase({ pi, notify });
    registerTurnMetricsPhase({ pi, startMetricsTicker, stopMetricsTicker, toolStartTimes, toolInputs });
    agentInbox = registerWorkerToolPhase({ pi, registeredToolNames, notify });

    // ── Foreground activity fallback: bracket generic model reasoning ────────────
    // Registered AFTER all phase hooks so these sit at the END of the turn_start
    // and turn_end handler arrays, never displacing earlier handlers (e.g. the
    // auto-compact handler that tests access via handlers.get('turn_end')![0]).
    // Uses pi.on directly (hooks is defined in the sibling if-block above).
    // Pi auto-shows its working row only during model streaming. Explicitly calling
    // setWorkingVisible(true) on turn_start keeps "Thinking…" visible through tool
    // execution gaps too, so the user always knows the agent is working.
    if (typeof pi.on === 'function') {
      pi.on('turn_start', (_event: unknown, ctx: PiContext | undefined) => {
        try {
          if (!ctx?.hasUI) return;
          const ui = ctx.ui;
          if (!ui) return;
          // A specific plan/work lifecycle always outranks generic model reasoning.
          if (['idle', 'complete', 'failed', 'ready_to_work'].includes(runtimeStoreFor(ctx)?.getState().activity.kind ?? 'idle')) {
            setManagedActivity(ctx, { kind: 'thinking' });
          }
          // The footer owns lifecycle text. Pi's working row supplies motion only,
          // so active turns never render two competing "Thinking…" labels.
          setManagedStatus(ctx, 'octocode-thinking', undefined);
        } catch {
          // UI operations are best-effort; never propagate to Pi’s event system.
        }
      });
      pi.on('turn_end', (_event: unknown, ctx: PiContext | undefined) => {
        try {
          if (!ctx?.hasUI) return;
          const ui = ctx.ui;
          if (!ui) return;
          // Clear only the fallback we own; review/start/work states survive the turn.
          if (runtimeStoreFor(ctx)?.getState().activity.kind === 'thinking') {
            setManagedActivity(ctx, { kind: 'idle' });
          }
          // Restore the quiet thinking-level chip (or clear it if unsupported).
          const level = pi.getThinkingLevel?.();
          const status = getThinkingStatus(ctx, level);
          setManagedStatus(ctx, 'octocode-thinking', status ? paintUi(ui, 'dim', status) : undefined);
        } catch {
          // UI operations are best-effort; never propagate to Pi’s event system.
        }
      });
    }

    // Re-assert disabled builtins after registration so a concurrent setActiveTools
    // (or Pi defaulting its builtin set) cannot restore read/edit/write/grep/find/ls.
    disableBuiltinTools(pi);
  }

  for (const name of [EXTENSION_COMMANDS.config.name, EXTENSION_COMMANDS.configuration.name]) pi.registerCommand?.(name, {
    description: EXTENSION_COMMANDS.configuration.description,
    handler: async (_args, ctx) => {
      try {
        const opened = await openMcpManager(ctx, session.latestPiSkills, 'overview', collectPublicCommands(pi), pi);
        if (!opened.ok) {
          notify(ctx, `${opened.message ?? 'Could not open the browser.'}${opened.url ? ` Open ${opened.url} manually.` : ''}`, 'error');
          return;
        }
        notify(ctx, `Configuration opened: ${opened.url}`, 'info');
      } catch (error) {
        notify(ctx, `Could not open configuration: ${error instanceof Error ? error.message : String(error)}`, 'error');
      }
    },
  });
}

// ─── Public factory ───────────────────────────────────────────────────────────

/**
 * Factory: returns the `(pi) => {...}` wiring function Pi invokes as `default(pi)`.
 * `export default createOctocodePiExtension()` preserves the historical single-arg
 * default-export contract exactly; the octocode-agent launcher opts into octocode-first
 * mode.
 */
export function createOctocodePiExtension(
  options: OctocodePiExtensionOptions = {},
): (pi: PiInstance) => Promise<void> {
  const promptMode = resolvePromptMode(options.promptMode);
  return async function octocodePiExtension(pi: PiInstance): Promise<void> {
    const piVersion = options.hostVersion ?? resolvePiHostVersion(pi);
    assertSupportedPiHostVersion(piVersion);
    return wireOctocodePiExtension(pi, { promptMode });
  };
}

// The evaluation API is host-adapter based: importing it performs no model or network call.
export {
  FROZEN_TRAJECTORY_CORPUS,
  FROZEN_TRAJECTORY_CORPUS_SHA256,
  TrajectoryReceiptSchema,
  buildTrajectoryReceipt,
  gradeTrajectory,
  runFrozenTrajectoryEvaluation,
} from './evals/prompt-trajectory.js';
export type {
  ScenarioGrade,
  TrajectoryEvent,
  TrajectoryModelAdapter,
  TrajectoryReceipt,
  TrajectoryScenario,
} from './evals/prompt-trajectory.js';

// Default export preserves the historical single-arg contract: Pi calls `default(pi)`.
export default createOctocodePiExtension();
