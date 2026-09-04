import fs from 'node:fs';
import path from 'node:path';
import { propagateOctocodeEnv, getOctocodeHome, isPersistentStorageEnabled } from './env.js';
import { extensionHome } from './extension-paths.js';
import { connectDb, defaultDbPath, insertEditLog } from '@octocodeai/octocode-awareness';
import { ensurePrivateDirectory, hardenPrivateFile, PRIVATE_FILE_MODE } from '@octocodeai/octocode-awareness/mcp-state';
import { openPersistentAwareness } from './tools/storage-policy.js';
import {
  DISABLED_BUILTIN_TOOL_NAMES,
  OVERRIDDEN_BUILTIN_TOOL_NAMES,
  OCTOCODE_SUPPORT_TOOL_NAMES,
} from './constants.js';
import { checkForCoreUpdate, readOwnVersion } from './core-update-check.js';
import { ensureAdaptiveThinkingCompatibility } from './model-compat.js';
import {
  getAssetPaths,
  readTextIfExists,
  listBundledSkills,
  getInstallSource,
  getAwarenessCLIPath,
  resolveAwarenessCliPath,
  runAwarenessPreEdit,
  resolveAwarenessCoordinationScope,
} from './assets.js';

// Expose the Awareness CLI for agents. The env var holds the SCRIPT PATH
// ONLY so the documented `node "$OCTOCODE_AWARENESS_CLI" <command>` invocation
// works in every shell (a "node /path" two-token string breaks under quoting
// and under zsh's no-word-split default). Guarded: a broken/missing install
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
import {
  mergeManagedAppendSystem,
  resolvePromptMode,
  composeSystemPrompt,
  stripProjectContext,
  stripPiSkillsSection,
} from './prompt.js';
import { registerSkillTool, discoverSkills, discoverSkillStates, formatSkillUsageLines, type DiscoveredSkill } from './tools/skill-tool.js';
import { writeDiscoveryFile, getDiscoveryFilePath } from './tools/discovery-file.js';
import {
  parseSetupScope,
  getAppendSystemTarget,
  estimateTokens,
} from './utils.js';
import { getDirectToolContractStats, registerUniqueTool } from './tools/octocode-tools.js';
import { registerCompactionHooks, resetCompactionCheckpointDedupe, setCompactionRehydrationSegmentsProvider } from './tools/compaction-hooks.js';
import { budgetToolResult } from './tools/tool-result-budget.js';
import {
  cleanupSpawnedAgentsForShutdown,
  formatAgentLedger,
  handleOctocodeAgentsCommand,
  listWorkerLedgerEntries,
  OCTOCODE_AGENTS_COMMAND_COMPLETIONS,
  OCTOCODE_AGENTS_COMMAND_DESCRIPTIONS,
  OCTOCODE_AGENTS_COMMAND_USAGE,
  refreshAgentLedgerUi,
  setAgentLedgerMetricsRefreshForUi,
  isSubagentProcess,
  pruneDroppableAgentsForSession,
} from './tools/agent-tools.js';
import { registerWebTool } from './tools/web-tool.js';
import { registerChromeDebugTool } from './tools/chrome-debug-tool.js';
import { registerUnifiedAgentTool } from './tools/unified-agent-tool.js';
import { registerCallTool } from './tools/call-tool.js';
import { registerFileTool } from './tools/file-tool.js';
import { registerReadMediaTool } from './tools/read-media-tool.js';
import { registerRunFfmpegTool } from './tools/run-ffmpeg-tool.js';
import { registerMediaTool } from './tools/create-media-tool.js';
import { renderRuntimeCapabilitiesAddendum } from './tools/image-render.js';
import { setPeerWipBaseline, setPeerWipStatusPainter } from './tools/peer-wip.js';
import { registerBashTool } from './tools/bash-tool.js';
import { createAwarenessMutationGate } from './tools/awareness-mutation-gate.js';
import {
  INITIAL_CONTEXT_TOKEN_BUDGET,
  PROVIDER_CONTEXT_TOKEN_BUDGET,
  assembleContextSegments,
  assertContextTokenBudget,
} from './tools/context-segments.js';
import {
  clearCurrentContextSources,
  mergeCurrentContextSources,
  readLatestSessionUserRequest,
  readSessionPeerEvent,
  readSessionToolResult,
  registerCurrentContextSource,
  sessionPeerEventOrigin,
  sessionToolResultOrigin,
  sessionUserRequestOrigin,
} from './tools/context-source-registry.js';
import { APPROVAL_CLASSES, PERMISSION_LEVELS, applyStartupPermissionLevel, approvedClasses, cyclePermissionLevel, getPermissionLevel, parsePermissionLevel, resetApprovalStore, revokeAlways, setPermissionLevel, type ApprovalClass } from './tools/approval.js';
import { getCachedMcpCatalogAddendum, getCachedMcpCounts, getMcpDiscoverySnapshot, isCompactMcpEnabled, mcpCatalogReady, registerMcpTool, startMcpConfigWatcher, stopAllMcpServers, stopMcpConfigWatchers, warmMcpCatalog } from './tools/mcp-tool.js';
import { openMcpManager } from './tools/mcp-html.js';
import { getDynamicCapabilitiesAddendum } from './tools/dynamic-catalog.js';
import { renderAvailableSkillsAddendum, renderSkillsDashboard } from './tools/skill-catalog.js';
import { registerPlanTool } from './tools/plan-tool.js';
import { registerLocalServerTool } from './tools/local-server-tool.js';
import { registerAskUserTool } from './tools/ask-user-tool.js';
import {
  registerInteractionBrokerAdapter,
  type InteractionBrokerAdapterRegistry,
  type RegisteredInteractionBrokerAdapter,
} from './tools/interaction-broker-adapter.js';
import { configureInteractionBrokerRoute } from './tools/interaction-broker.js';
import { registerMemoryTool } from './tools/memory-tool.js';
import { registerAwarenessCoordinationTools } from './tools/awareness-coordination-tools.js';
import { awarenessEventStatusText, registerAwarenessEventConsumer } from './tools/awareness-event-consumer.js';
import { getAwarenessAgentId } from './tools/awareness-shared.js';
import { activePlanScope, adoptPlanFromBranch, getPlan, getPlanReviewState, bumpPlanTurn, setPlanEntryAppender, PLAN_ENTRY_TYPE } from './tools/active-plan.js';
import { getCurrentPlanReadModel, renderPlanContext } from './tools/plan-read-model.js';
import { getCachedAwarenessStatus, refreshAwarenessPanel, suppressAwarenessPanel, resumeAwarenessPanel, clearAwarenessCacheEntry, setAwarenessMetricsRefreshForUi } from './tools/awareness-status.js';
import { getFooterDensity, parseFooterDensity, resolveSystemThemeName, setFooterDensity, deriveSessionName, OCTOCODE_THEME_DARK, OCTOCODE_THEME_LIGHT, type OctocodeThemeName } from './ui-extras.js';
import { paintUi } from './tui/palette.js';
import { setUiTickSubscriber } from './tui/ui-ticker.js';
import { FOOTER_LEGEND, PERMISSION_LEVEL_SUMMARY } from './tui/content.js';
import { listCDPSessions, closeAllChromeConnections } from './chrome-connection-cache.js';
import { handleOctocodePlanCommand, OCTOCODE_PLAN_COMMAND_USAGE, OCTOCODE_PLAN_COMMAND_COMPLETIONS, setPlanMetricsRefreshForUi } from './tools/plan-tool.js';
import { adoptPlanModePolicy, evaluateToolCapability, exitPlanMode, getPlanModePolicy, planModeToolGate } from './tools/plan-mode.js';
import { atomicWriteUtf8, clearAllReadStates, resolveFilePath } from './tools/file-state.js';
import { registerAgentInbox, type AgentInboxRegistration } from './tools/agent-inbox.js';
import { getPaletteShortcut, registerCommandPalette } from './tools/command-palette.js';
import { collectPublicCommands, registerCommandsCommand } from './tools/commands-command.js';
import { registerCleanupCommand, runCleanupOnInit } from './tools/cleanup-command.js';
import { probeGitHubAuth } from './tools/github-auth-status.js';
import { registerOctocodeAutocomplete } from './tools/autocomplete-providers.js';
import { registerOctocodeMessageRenderers } from './tools/custom-messages.js';
import { initCheckpointStore, type CheckpointEngine } from './tools/checkpoints.js';
import { createSessionArtifactContext, workspaceAgentRoot } from './tools/session-artifacts.js';
import { cleanupEphemeralToolOutputs } from './tools/ephemeral-tool-output.js';
import { consumeValidatedRehydration, runAndRecordRehydration, REHYDRATION_RECEIPT_ENTRY_TYPE, type CurrentRehydrationSource } from './tools/rehydration-orchestrator.js';
import { createCheckpointInputHook, registerRewindCommand } from './tools/rewind-command.js';
import { registerDialCommand, restoreDialOnStartup } from './tools/effort-dial.js';
import { registerAiWatch, isWatchActive, markOwnWrite, markBashActivity, stopWatch } from './tools/ai-watch.js';
import { runtimeStoreFor, setManagedActivity, setManagedStatus } from './tools/runtime-renderer.js';
import { SessionRuntime } from './session-runtime.js';
import {
  applyOctocodeUi,
  buildRepoStateHint,
  execGitSummary,
  formatContextUsage,
  getThinkingStatus,
  OCTOCODE_BANNER_ENTRY_TYPE,
  refreshFooterDirtyState,
  resetOctocodeFooterRegistration,
  updateOctocodeMetricsUi,
} from './extension-ui.js';
import {
  APPROVED_PI_HOST_VERSION,
  assertSupportedPiHostVersion,
  resolvePiHostVersion,
} from './adapters/pi-host-compatibility.js';
import {
  capturePiSdkLifecycle,
  createPiSdkScenarioSuite,
  type ProductionPiLifecycleCapture,
  type ProductionPiScenarioSuite,
} from './adapters/pi-production-probe.js';
import { createPiCanonicalRegistryComposition } from './adapters/pi-registry-adapters.js';
import { registerExportCommand } from './tools/export-command.js';
import { assertPathAllowed } from './tools/path-guard.js';
import { makeRenderer } from './tools/render-helpers.js';
import { renderBannerWithTagline, type BannerSessionInfo, type BannerTheme } from './branding/banner.js';
import { pickProvider } from './web.js';
import { createHookComposer } from './hook-composer.js';
import { loadProfile, type Profile } from './surfaces.js';
import {
  createOctocodeCronScheduler,
  formatOctocodeCronSummary,
  handleOctocodeCronCommand,
  OCTOCODE_CRON_COMMAND_COMPLETIONS,
  OCTOCODE_CRON_COMMAND_USAGE,
} from './scheduler.js';
import type {
  BeforeAgentStartEvent,
  CommandDefinition,
  PiInstance,
  PiContext,
  PiModel,
  OctocodePiExtensionOptions,
  PromptMode,
  SessionShutdownEvent,
  ThinkingLevelEvent,
  SkillInfo,
  NotifyFn,
} from './types.js';

export {
  DISABLED_BUILTIN_TOOL_NAMES,
  OVERRIDDEN_BUILTIN_TOOL_NAMES,
  OCTOCODE_SUPPORT_TOOL_NAMES,
} from './constants.js';
export {
  PACKAGE_NAME,
  SYSTEM_PROMPT_MARKER,
  MANAGED_BLOCK_START,
  MANAGED_BLOCK_END,
} from './constants.js';
export { getAssetPaths, readTextIfExists, listBundledSkills, getInstallSource, getAwarenessCLIPath, buildAwarenessCommand } from './assets.js';
export {
  buildSurfaceSpec,
  loadProfile,
  profileToPiArgs,
} from './surfaces.js';
export type { Profile, SurfaceSpec, SurfaceVerb } from './surfaces.js';
export {
  renderSystemPromptAddendum,
  renderManagedAppendSystem,
  mergeManagedAppendSystem,
  resolvePromptMode,
  composeSystemPrompt,
  stripProjectContext,
  stripPiSkillsSection,
} from './prompt.js';
export {
  splitArgs,
  parseSetupScope,
  getAppendSystemTarget,
  truncateUserVisibleToolOutput,
} from './utils.js';
export { runWebTool, renderWebResult, pickProvider } from './web.js';
export {
  createHookComposer,
  OctocodeHookComposer,
  runHookMiddleware,
} from './hook-composer.js';
export {
  APPROVED_PI_HOST_VERSION,
  PiHostCompatibilityError,
  assertSupportedPiHostVersion,
  resolveInstalledPiHostVersion,
  resolvePiHostVersion,
} from './adapters/pi-host-compatibility.js';
export {
  PI_LIFECYCLE_MAPPINGS,
  bindPiLifecycleBus,
  createPiEventEnvelope,
  isPiLifecycleEvent,
  mapPiHookResultToDecision,
} from './adapters/pi-lifecycle-adapter.js';
export {
  PiToolRegistryAdapter,
  PiCommandRegistryAdapter,
  createPiCanonicalRegistryComposition,
  getPiRegistryRegistrationReceipts,
} from './adapters/pi-registry-adapters.js';
export { PiSettingsAdapter } from './adapters/pi-settings-adapter.js';
export { PiPluginEventAdapter } from './adapters/pi-plugin-adapter.js';
export { discoverCodexHookSources } from './adapters/pi-hook-discovery.js';
export {
  createOctocodeCronScheduler,
  formatOctocodeCronStatus,
  handleOctocodeCronCommand,
  OCTOCODE_CRON_COMMAND_COMPLETIONS,
  OCTOCODE_CRON_COMMAND_USAGE,
} from './scheduler.js';
export type {
  OctocodeCronJobDefinition,
  OctocodeCronJobSnapshot,
  OctocodeCronRunResult,
  OctocodeCronScheduler,
  OctocodeCronSchedulerOptions,
} from './scheduler.js';
export {
  cleanupSpawnedAgentsForShutdown,
  DEFAULT_SPAWN_POLICY,
  evaluateSpawnPolicy,
  OCTOCODE_AGENTS_COMMAND_COMPLETIONS,
  OCTOCODE_AGENTS_COMMAND_DESCRIPTIONS,
  OCTOCODE_AGENTS_COMMAND_USAGE,
  formatAgentLedger,
  formatAgentLedgerDetails,
  handleOctocodeAgentsCommand,
  listWorkerLedgerEntries,
  normalizeWorkerOutput,
  evaluateWorkerRecoveryRisk,
  refreshAgentLedgerUi,
  setAgentProcessFactoryForTests,
  setAgentWorktreeGitRunnerForTests,
} from './tools/agent-tools.js';
export type {
  PromptMode,
  OctocodePiExtensionOptions,
  SkillInfo,
  BuildSystemPromptOptions,
  LedgerEvent,
  SpawnPolicy,
  SpawnPolicyResult,
  WorkerLedgerEntry,
  WorkerLedgerEvent,
  WorkerLedgerEventType,
} from './types.js';
export {
  applyOctocodeUi,
  getThinkingStatus,
  OCTOCODE_BANNER_ENTRY_TYPE,
} from './extension-ui.js';
export type { OctocodeMetricsState } from './extension-ui.js';

// getAwarenessAgentId is single-sourced in tools/awareness-shared.ts (shared
// with the first-class coordination tools) and imported above.

const awarenessMutationGate = createAwarenessMutationGate({
  storeExists: (workspace) => {
    if (!isPersistentStorageEnabled()) return false;
    const scope = resolveAwarenessCoordinationScope(workspace);
    return fs.existsSync(defaultDbPath(workspace, scope));
  },
  queryTarget: (target, workspace, agentId) => {
    const scope = resolveAwarenessCoordinationScope(workspace);
    const result = runAwarenessPreEdit({
      workspace,
      scope,
      agentId,
      host: 'pi',
      event: { toolName: 'write', input: { path: target } },
    });
    return { blocked: result.blocked, message: result.message };
  },
  startWork: (target, workspace, agentId) => {
    const aw = openPersistentAwareness({ workspace, scope: resolveAwarenessCoordinationScope(workspace) });
    try {
      aw.startWork({ filePath: target, agentId, reason: 'Automatic Pi mutation presence' });
    } finally {
      aw.close();
    }
  },
  endWork: (target, workspace, agentId) => {
    const aw = openPersistentAwareness({ workspace, scope: resolveAwarenessCoordinationScope(workspace) });
    try {
      aw.endWork({ filePath: target, agentId });
    } finally {
      aw.close();
    }
  },
  recordEdit: (target, workspace, agentId) => {
    if (!isPersistentStorageEnabled()) return;
    const scope = resolveAwarenessCoordinationScope(workspace);
    const database = connectDb(defaultDbPath(workspace, scope));
    try {
      insertEditLog(database, {
        agentId,
        filePath: target,
        operation: 'update',
        workspacePath: workspace,
        artifact: 'pi-native-hook',
      });
    } finally {
      database.close();
    }
  },
  warn: (message) => console.warn(`[octocode] ${message}`),
});

/**
 * Fire-and-forget Awareness registry presence. Join at session_start with
 * the session-stable agent id — Lite generates a funny host-tagged name
 * (octo-* here, since the harness sets OCTOCODE_AGENT_HOST) so peers in other
 * runners (clawde-*, cursea-*) see WHO is active in the shared workspace.
 * Leave at shutdown so the registry doesn't accumulate stale ACTIVE rows.
 * Best-effort: never blocks the session and never throws.
 */
function updateAwarenessRegistry(action: 'join' | 'leave', _pi: PiInstance, ctx?: PiContext, cwdOverride?: string): void {
  const cwd = cwdOverride ?? ctx?.cwd ?? process.cwd();
  let aw: ReturnType<typeof openPersistentAwareness> | undefined;
  try {
    aw = openPersistentAwareness({ workspace: cwd });
    const agentId = getAwarenessAgentId(cwdOverride === undefined ? ctx : undefined);
    if (action === 'join') aw.joinAgent({ agentId, role: 'lead' });
    else aw.leaveAgent({ agentId });
  } catch { /* Awareness unresolved — skip */ }
  finally { aw?.close(); }
}

function runAwarenessMutationGate(event: { toolName?: string; input?: Record<string, unknown> }, ctx?: PiContext): { block?: boolean; reason?: string } | void {
  const workspace = ctx?.cwd ?? process.cwd();
  const agentId = getAwarenessAgentId(ctx);
  return awarenessMutationGate.preflight(event, workspace, agentId);
}

export function getInternalErrorLogPath(
  cwd = process.cwd(),
  sessionManager?: { getSessionId?(): string | undefined; getSessionFile?(): string | undefined },
): string {
  if (sessionManager) {
    try {
      // resolveSessionIdentity is pure computation (zero I/O). The session artifact dir
      // is created lazily on the first real appendFile write, not on every path lookup.
      return createSessionArtifactContext({ cwd, sessionManager }).resolve('logs/error.txt');
    } catch { /* fallback when cwd is unavailable */ }
  }
  return path.join(workspaceAgentRoot(cwd), 'logs', 'error.txt');
}

function normalizeError(error: unknown): { name?: string; message: string; stack?: string; cause?: string } {
  if (error instanceof Error) {
    return {
      name: error.name,
      message: error.message,
      stack: error.stack,
      cause: error.cause === undefined ? undefined : String(error.cause),
    };
  }
  return { message: String(error) };
}

function redactForLog(value: unknown, depth = 0, seen = new WeakSet<object>()): unknown {
  if (value === null || value === undefined) return value;
  if (typeof value === 'string') {
    return value
      .replace(/Bearer\s+[A-Za-z0-9._~+\/-]+=*/gi, 'Bearer [REDACTED]')
      .replace(/(api[_-]?key|token|secret|password)=([^\s&]+)/gi, '$1=[REDACTED]');
  }
  if (typeof value !== 'object') return value;
  if (seen.has(value)) return '[Circular]';
  if (depth >= 6) return '[MaxDepth]';
  seen.add(value);
  if (Array.isArray(value)) {
    return value.slice(0, 50).map((item) => redactForLog(item, depth + 1, seen));
  }
  const out: Record<string, unknown> = {};
  for (const [key, item] of Object.entries(value as Record<string, unknown>).slice(0, 100)) {
    if (/authorization|cookie|set-cookie|token|secret|password|api[_-]?key|access[_-]?key|credential/i.test(key)) {
      out[key] = '[REDACTED]';
    } else {
      out[key] = redactForLog(item, depth + 1, seen);
    }
  }
  return out;
}

function safeJson(value: unknown): string {
  try {
    return JSON.stringify(redactForLog(value), null, 2);
  } catch {
    return String(value);
  }
}

function formatContextForLog(ctx: PiContext | undefined): string[] {
  const usage = ctx?.getContextUsage?.();
  return [
    `cwd: ${ctx?.cwd ?? process.cwd()}`,
    ctx?.mode ? `mode: ${ctx.mode}` : '',
    ctx?.model?.id ? `model: ${ctx.model.id}` : '',
    ctx?.model ? `modelReasoning: ${String(ctx.model.reasoning)}` : '',
    usage && usage.tokens != null ? `context: ${usage.tokens}/${usage.contextWindow} (${Math.round((usage.tokens / usage.contextWindow) * 100)}%)` : usage ? 'context: unknown (post-compaction)' : '',
  ].filter(Boolean);
}

export interface InternalErrorLogOptions {
  severity?: 'error' | 'warning';
  stack?: boolean;
}

export function logInternalError(
  source: string,
  error: unknown,
  details: Record<string, unknown> = {},
  ctx?: PiContext,
  options: InternalErrorLogOptions = {},
): void {
  try {
    const severity = options.severity ?? 'error';
    const includeStack = options.stack ?? severity === 'error';
    const logPath = getInternalErrorLogPath(ctx?.cwd ?? process.cwd(), ctx?.sessionManager);
    const normalized = normalizeError(error);
    const durationMs = typeof details['durationMs'] === 'number' ? details['durationMs'] : undefined;
    const redactedDetails = Object.keys(details).length > 0 ? safeJson(details) : '';
    ensurePrivateDirectory(path.dirname(logPath));
    hardenPrivateFile(logPath);
    fs.appendFileSync(
      logPath,
      [
        severity === 'warning' ? '=== Octocode Pi Extension Warning ===' : '=== Octocode Pi Extension Error ===',
        `timestamp: ${new Date().toISOString()}`,
        `uptimeMs: ${Math.round(process.uptime() * 1000)}`,
        `source: ${source}`,
        `severity: ${severity}`,
        durationMs === undefined ? '' : `durationMs: ${durationMs}`,
        ...formatContextForLog(ctx),
        normalized.name ? `error.name: ${normalized.name}` : '',
        `error.message: ${normalized.message}`,
        normalized.cause ? `error.cause: ${normalized.cause}` : '',
        redactedDetails ? `details: ${redactedDetails}` : '',
        includeStack && normalized.stack ? `stack:\n${normalized.stack}` : '',
        '---',
      ].filter(Boolean).join('\n') + '\n',
      { encoding: 'utf8', mode: PRIVATE_FILE_MODE },
    );
    hardenPrivateFile(logPath);
  } catch {
    // Logging must never become the reason the extension fails.
  }
}

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

async function confirm(
  ctx: PiContext | undefined,
  title: string,
  message: string,
): Promise<boolean> {
  if (!ctx?.ui?.confirm) return false;
  return Boolean(await ctx.ui.confirm(title, message));
}

// ─── Status / harness ────────────────────────────────────────────────────────

function activeSupportToolNames(): readonly string[] {
  return OCTOCODE_SUPPORT_TOOL_NAMES.filter((name) => {
    if (name === 'chromeDebug' && process.env['OCTOCODE_CHROME_DEBUG'] === '0') return false;
    if (isSubagentProcess() && (name === 'agent' || name === 'callTool')) return false;
    return true;
  });
}

function formatOctocodeToolStatus(): string {
  return `MCP research (octocode server) · ${activeSupportToolNames().length} support · ${OVERRIDDEN_BUILTIN_TOOL_NAMES.length} guarded built-ins · ${DISABLED_BUILTIN_TOOL_NAMES.length} replaced`;
}

function formatToolCapabilitySummary(): string {
  return [
    `research: GitHub/local/LSP/npm via MCPTool (octocode server)`,
    `support: ${activeSupportToolNames().join(', ')}`,
    `guarded mutations: ${OVERRIDDEN_BUILTIN_TOOL_NAMES.join(', ')}`,
    `replaced weak built-ins: ${DISABLED_BUILTIN_TOOL_NAMES.join(', ')}`,
  ].join('\n');
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
    `awareness CLI: ${getAwarenessCLIPath(baseDir)} — user CLI: npx -p @octocodeai/octocode-awareness octocode-awareness <command> [action] --workspace "$PWD"`,
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
    extensionCommands: [
      '/commands',
      '/octocode',
      '/octocode-harness',
      '/octocode-now',
      '/octocode-tasks',
      '/octocode-skills',
      '/octocode-agents',
      '/octocode-cron',
      '/settings',
      '/mcp',
      '/octocode-setup',
      '/octocode-skills-update',
      '/octocode-plan',
      '/octocode-theme',
      '/octocode-chrome',
      '/octocode-footer',
      '/octocode-permissions',
      '/octocode-profile',
      '/octocode-inbox',
      '/octocode-palette',
      '/octocode-rewind',
      '/octocode-dial',
      '/octocode-watch',
      '/octocode-export',
    ],
    skills: listBundledSkills(baseDir),
    cliNote: `management: npx octocode skill | lsp-server | auth (no bundled CLI — use npx octocode for management tasks)`,
    awarenessCliNote: `Awareness CLI: ${getAwarenessCLIPath(baseDir)}; user CLI: npx -p @octocodeai/octocode-awareness octocode-awareness <command> [action] --workspace "$PWD"`,
  };
}

export function formatOctocodeDashboard(ctx?: PiContext, baseDir?: string, sessionJobs?: string): string {
  const paths = getAssetPaths(baseDir);
  const skills = listBundledSkills(baseDir);
  const context = formatContextUsage(ctx);
  const promptOk = fs.existsSync(paths.systemPrompt);
  const awarenessCliPath = getAwarenessCLIPath(baseDir);
  const searchProvider = pickProvider({});
  const warnings = [
    context.percent !== undefined && context.percent >= 80 ? `⚠ context at ${context.percent}% — Pi compacts in-run at its configured reserve threshold` : '',
    promptOk ? '' : `⚠ missing system prompt at ${paths.systemPrompt}`,
    searchProvider === 'duckduckgo' ? '⚠ web search using DuckDuckGo fallback; add Tavily/Serper for stronger results' : '',
  ].filter(Boolean);

  return [
    '◆ Octocode dashboard — extension health & setup (live work: /octocode-now)',
    '',
    'Status',
    `${promptOk ? '✓' : '⚠'} system prompt: ${promptOk ? 'found' : 'missing'}`,
    `✓ tools: ${formatOctocodeToolStatus()}`,
    `✓ metrics: ${context.text}`,
    `Awareness: ${awarenessCliPath} (user CLI: npx -p @octocodeai/octocode-awareness octocode-awareness <command> [action] --workspace "$PWD")`,
    `Management: npx octocode skill | lsp-server | auth`,
    '',
    'Agents',
    formatAgentLedger(),
    `ledger entries: ${listWorkerLedgerEntries().length} · details: /octocode-agents list`,
    '',
    'Tools',
    formatToolCapabilitySummary(),
    '',
    'Session jobs',
    sessionJobs ?? 'No session jobs scheduled — use /octocode-cron to schedule repeating tasks.',
    '',
    'Setup',
    `project APPEND_SYSTEM: ${getAppendSystemTarget('project', ctx?.cwd ?? process.cwd())}`,
    `global APPEND_SYSTEM: ${getAppendSystemTarget('global', ctx?.cwd ?? process.cwd())}`,
    'APPEND_SYSTEM is for plain-Pi sessions — not needed when this extension is loaded (prompt injected at runtime).',
    '',
    'Skills',
    `${skills.length} bundled: ${skills.join(', ') || '(none)'}`,
    '',
    'Health',
    ...(warnings.length > 0 ? warnings : ['✓ no dashboard warnings']),
    '',
    'Next actions',
    `/commands (all slash commands) · /octocode-palette${getPaletteShortcut() ? ` (${getPaletteShortcut()})` : ''}`,
    '/octocode-permissions (approval gate) · /octocode-footer legend (toolbar decoder)',
  ].join('\n');
}

function formatModelLine(ctx?: PiContext): string {
  const model = ctx?.model;
  if (!model?.id) return 'model: unknown';
  const provider = model.provider ? `${model.provider}/` : '';
  const thinking = model.reasoning ? ' · reasoning' : '';
  return `model: ${provider}${model.id}${thinking}`;
}

function formatPlanLines(ctx?: PiContext): string[] {
  const steps = getPlan(activePlanScope(ctx));
  if (steps.length === 0) return ['local plan: none — use plan(set) for multi-step work'];
  const done = steps.filter((s) => s.status === 'done').length;
  const current = steps.find((s) => s.status === 'doing') ?? steps.find((s) => s.status !== 'done');
  return [
    `local plan: ${done}/${steps.length} done`,
    current ? `now: ${current.activeForm && current.status === 'doing' ? current.activeForm : current.text}` : 'now: all steps done — verify, then clear',
  ];
}

function formatAwarenessLines(ctx?: PiContext): string[] {
  const cwd = ctx?.cwd ?? process.cwd();
  const status = getCachedAwarenessStatus(cwd);
  if (!status) return ['shared tasks: no cached Awareness status yet — refresh queued; run /octocode-now again'];
  const lines = [
    `shared tasks: plans ${status.activePlans} · ready ${status.readyTasks} · doing ${status.inProgressTasks}`,
    `verify debt: ${status.verifyTasks} · locks ${status.lockCount} · work ${status.workCount}`,
  ];
  // Surface inter-agent communication only when there is any — non-rigid, no empty noise.
  const comms: string[] = [];
  if (status.unreadInbox && status.unreadInbox > 0) {
    comms.push(status.lastInbound
      ? `✉ ${status.unreadInbox} unread (from ${status.lastInbound.from}: ${status.lastInbound.preview})`
      : `✉ ${status.unreadInbox} unread`);
  }
  if (status.messageCount > 0) {
    comms.push(status.lastMessage
      ? `peer-msgs ${status.messageCount} (last ${status.lastMessage.from}→${status.lastMessage.to}: ${status.lastMessage.preview})`
      : `peer-msgs ${status.messageCount}`);
  }
  if (comms.length) lines.push(`agent comms: ${comms.join(' · ')}`);
  return lines;
}

function compactRepoStatus(status: string): string[] {
  const lines = status.split('\n').filter(Boolean);
  if (lines.length === 0) return ['git: clean or unavailable'];
  const shown = lines.slice(0, 8);
  if (lines.length > shown.length) shown.push(`… ${lines.length - shown.length} more dirty entries`);
  return shown;
}

export function formatOctocodeTasks(ctx?: PiContext): string {
  return [
    '◆ Octocode tasks',
    '',
    'Local session plan',
    ...formatPlanLines(ctx),
    '',
    'Shared Awareness work',
    ...formatAwarenessLines(ctx),
    '',
    'Rule of thumb',
    'Use plan(...) for your current solo breakdown; use Awareness plan/task/work when state must survive sessions or coordinate agents.',
    'Commands: /octocode-plan · npx -p @octocodeai/octocode-awareness octocode-awareness status --workspace "$PWD"',
  ].join('\n');
}

export async function formatOctocodeNow(ctx: PiContext | undefined, pi: PiInstance): Promise<string> {
  refreshAwarenessPanel(ctx);
  const repoStatus = await execGitSummary(pi, ['status', '--short', '--branch'], 800);
  return [
    '◆ Octocode now — live work cockpit (extension health: /octocode)',
    '',
    'Orientation',
    formatModelLine(ctx),
    formatContextUsage(ctx).text,
    `mode: ${ctx?.mode ?? 'unknown'} · cwd: ${ctx?.cwd ?? process.cwd()}`,
    '',
    'Current work',
    ...formatPlanLines(ctx),
    '',
    'Shared work',
    ...formatAwarenessLines(ctx),
    '',
    'Agents',
    formatAgentLedger(),
    '',
    'Repository',
    ...compactRepoStatus(repoStatus),
    '',
    'Next actions',
    '/octocode-tasks · /octocode-skills · /octocode-agents · /settings · /octocode-cron',
  ].join('\n');
}

// ─── Built-in tool disable ────────────────────────────────────────────────────

/**
 * Remove Pi builtins that Octocode replaces with MCP research or `file`
 * (`read`/`edit`/`write`/`grep`/`find`/`ls`). Idempotent. Call after tool
 * registration and again on `session_start` so later `setActiveTools` resets
 * cannot silently re-enable the weak builtins.
 */
export function disableBuiltinTools(pi: PiInstance): boolean {
  if (!pi.getActiveTools || !pi.setActiveTools) return false;
  try {
    const activeTools = pi.getActiveTools();
    if (!Array.isArray(activeTools)) return false;
    const disabled = new Set<string>(DISABLED_BUILTIN_TOOL_NAMES);
    const nextTools = activeTools.filter((toolName) => !disabled.has(toolName));
    if (nextTools.length === activeTools.length) return false;
    pi.setActiveTools(nextTools);
    return true;
  } catch (error) {
    // Swallow all errors from getActiveTools/setActiveTools — the Pi API shape can
    // change across versions and races during initialization must never prevent the
    // extension from loading. Log unexpected errors for diagnostics but never rethrow.
    const msg = String((error as Error)?.message ?? error);
    if (!msg.includes('Extension runtime not initialized')) {
      console.warn('[octocode-pi-extension] disableBuiltinTools non-critical error:', msg);
    }
    return false;
  }
}

function profileFilePath(): string {
  return path.join(extensionHome(getOctocodeHome(process.env)), 'profiles.json');
}

function listProfileNames(): string[] {
  try {
    const file = profileFilePath();
    if (!fs.existsSync(file)) return [];
    const parsed = JSON.parse(fs.readFileSync(file, 'utf8')) as Record<string, unknown>;
    return Object.keys(parsed).sort((a, b) => a.localeCompare(b));
  } catch {
    return [];
  }
}

function splitProfileToolList(value: string | undefined): string[] {
  return (value ?? '')
    .split(/[\s,]+/)
    .map((tool) => tool.trim())
    .filter(Boolean);
}

function profileApproveToPermission(profile: Profile): 'strict' | 'default' | 'relaxed' | undefined {
  if (profile.approve === 'always') return 'relaxed';
  if (profile.approve === 'never') return 'strict';
  if (profile.approve === 'ask') return 'default';
  return undefined;
}

function resolveProfileModel(profileModel: string, ctx: PiContext | undefined): PiModel {
  const slash = profileModel.indexOf('/');
  const explicitProvider = slash > 0 ? profileModel.slice(0, slash) : undefined;
  const modelId = slash > 0 ? profileModel.slice(slash + 1) : profileModel;
  const provider = explicitProvider ?? ctx?.model?.provider;
  const registryMatch = provider ? ctx?.modelRegistry?.find(provider, modelId) : undefined;
  return registryMatch ?? { id: modelId, provider };
}

async function applyRuntimeProfile(pi: PiInstance, ctx: PiContext | undefined, name: string, profile: Profile): Promise<{ lines: string[]; warnings: string[] }> {
  const lines: string[] = [];
  const warnings: string[] = [];

  if (profile.model) {
    if (typeof pi.setModel !== 'function') {
      warnings.push('model unchanged: host does not support setModel');
    } else {
      const model = resolveProfileModel(profile.model, ctx);
      const ok = await pi.setModel(model);
      if (ok) lines.push(`model: ${model.provider ? `${model.provider}/` : ''}${model.id ?? profile.model}`);
      else warnings.push(`model unchanged: ${profile.model} was not accepted by Pi`);
    }
  }

  const includeTools = splitProfileToolList(profile.tools);
  const excludeTools = new Set(splitProfileToolList(profile.excludeTools));
  if (includeTools.length > 0 || excludeTools.size > 0) {
    if (!pi.getActiveTools || !pi.setActiveTools) {
      warnings.push('tools unchanged: host does not support active tool scoping');
    } else {
      const baseTools = includeTools.length > 0 ? includeTools : pi.getActiveTools();
      const scoped = baseTools.filter((tool) => !excludeTools.has(tool));
      pi.setActiveTools(scoped);
      disableBuiltinTools(pi);
      lines.push(`tools: ${pi.getActiveTools?.().join(', ') ?? scoped.join(', ')}`);
    }
  }

  const permission = profileApproveToPermission(profile);
  if (permission) {
    setPermissionLevel(permission);
    lines.push(`permissions: ${permission} (${profile.approve})`);
  }

  if (lines.length === 0 && warnings.length === 0) lines.push(`profile ${name}: no live-applicable fields`);
  return { lines, warnings };
}

// ─── APPEND_SYSTEM installer ──────────────────────────────────────────────────

async function installAppendSystem(args: string, ctx: PiContext | undefined): Promise<void> {
  const paths = getAssetPaths();
  const prompt = readTextIfExists(paths.systemPrompt);
  if (prompt.trim().length === 0) {
    notify(ctx, `Missing Octocode system prompt at ${paths.systemPrompt}`, 'error');
    return;
  }
  const scope = parseSetupScope(args);
  const targetPath = getAppendSystemTarget(scope, ctx?.cwd ?? process.cwd());
  if (!ctx?.hasUI) {
    notify(ctx, '/octocode-setup requires an interactive session to confirm. Run from the Pi UI.', 'error');
    return;
  }
  const ok = await confirm(
    ctx,
    'Install Octocode APPEND_SYSTEM.md?',
    `Write the managed Octocode harness block to ${targetPath}? ` +
      'Note: sessions running this extension already inject the prompt at runtime (marker-guarded, no double append) — install only for plain-Pi sessions without the extension.',
  );
  if (!ok) {
    notify(ctx, 'Octocode setup cancelled.', 'info');
    return;
  }
  const existing = readTextIfExists(targetPath);
  const nextContent = mergeManagedAppendSystem(existing, prompt);
  try {
    assertPathAllowed(targetPath, ctx?.cwd ?? process.cwd(), 'octocode setup');
    await atomicWriteUtf8(targetPath, nextContent);
    notify(ctx, `Octocode APPEND_SYSTEM.md installed at ${targetPath}`, 'info');
  } catch (error) {
    notify(
      ctx,
      `Failed to write ${targetPath}: ${(error as Error)?.message ?? String(error)}`,
      'error',
    );
  }
}

function existingDirectory(filePath: string): string | null {
  return fs.existsSync(filePath) ? filePath : null;
}

// ─── Pi wiring ────────────────────────────────────────────────────────────────

type TypeBoxBuilder = (typeof import('typebox'))['Type'];

interface SupportToolRegistrationArgs {
  pi: PiInstance;
  Type: TypeBoxBuilder;
  registeredToolNames: Set<string>;
  notify: NotifyFn;
  getLatestAvailableSkills: () => SkillInfo[] | undefined;
}

function registerSupportToolPhase({ pi, Type, registeredToolNames, notify, getLatestAvailableSkills }: SupportToolRegistrationArgs): void {
  registerFileTool(pi, Type, registeredToolNames, registerUniqueTool);
  registerBashTool(pi, Type, registeredToolNames, registerUniqueTool);
  registerReadMediaTool(pi, Type, registeredToolNames, registerUniqueTool);
  registerMediaTool(pi, Type, registeredToolNames, registerUniqueTool);
  registerRunFfmpegTool(pi, Type, registeredToolNames, registerUniqueTool);

  registerWebTool(pi, Type, registeredToolNames, registerUniqueTool);

  if (process.env['OCTOCODE_CHROME_DEBUG'] !== '0') {
    registerChromeDebugTool(pi, Type, registeredToolNames, registerUniqueTool, notify);
  }

  registerUnifiedAgentTool(pi, Type, registeredToolNames, registerUniqueTool);
  registerCallTool(pi, Type, registeredToolNames, registerUniqueTool);

  // Octocode-owned skill loading replaces Pi's read-based flow. The public
  // skill facade dispatches load/list and dynamic lifecycle queries.
  registerSkillTool(pi, Type, registeredToolNames, registerUniqueTool, getLatestAvailableSkills);

  registerPlanTool(pi, Type, registeredToolNames, registerUniqueTool);
  registerLocalServerTool(pi, Type, registeredToolNames, registerUniqueTool);
  registerAskUserTool(pi, Type, registeredToolNames, registerUniqueTool);
  registerMemoryTool(pi, Type, registeredToolNames, registerUniqueTool);
  registerAwarenessCoordinationTools(pi, Type, registeredToolNames, registerUniqueTool);
  registerMcpTool(pi, Type, registeredToolNames, registerUniqueTool);
}

interface RuntimeUiRegistrationArgs {
  pi: PiInstance;
  notify: NotifyFn;
}

function registerRuntimeUiPhase({ pi, notify }: RuntimeUiRegistrationArgs): void {
  registerCompactionHooks(pi, notify);
  registerAwarenessEventConsumer(pi, {
    resolveExpectedAgentId: (ctx) => getAwarenessAgentId(ctx),
    onDelivery: (message, ctx) => {
      const eventId = message.details.eventId;
      registerCurrentContextSource(ctx, {
        version: 1,
        id: `peer-event:${eventId}`,
        kind: 'peer-event',
        origin: sessionPeerEventOrigin(eventId),
        authority: 'external-data',
        scope: 'turn',
        visibility: 'inspectable',
        rehydrate: 'on-trigger',
        capture: false,
        readCurrent: (current) => readSessionPeerEvent(current, eventId),
      });
    },
    onObservability: (stats, ctx) => {
      runtimeStoreFor(ctx)?.getState().setStatus(
        'octocode-awareness-events',
        awarenessEventStatusText(stats),
      );
    },
  });
  // Branded conversation cards (compaction checkpoints / awareness handoffs)
  // — must be registered before compaction-hooks emits the first card.
  registerOctocodeMessageRenderers(pi);
  // Fresh-session banner card: a durable TUI-only transcript entry (never in
  // LLM context) — the wordmark scrolls past like a splash instead of
  // occupying the header, and re-renders on resume where it originally sat.
  pi.registerEntryRenderer?.(OCTOCODE_BANNER_ENTRY_TYPE, (entry, _options, theme) => {
    // Read the session-info snapshot stamped into the entry at append time so
    // the banner shows startup model/thinking without any time-varying bytes.
    const data = entry as { model?: string; provider?: string; thinking?: string } | undefined;
    const sessionInfo: BannerSessionInfo | undefined =
      data?.model || data?.provider || data?.thinking
        ? { model: data.model, provider: data.provider, thinking: data.thinking }
        : undefined;
    return makeRenderer((width) =>
      renderBannerWithTagline(theme as BannerTheme, width, readOwnVersion(getAssetPaths().baseDir), sessionInfo),
    );
  });
}

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
    runtimeStoreFor(ctx)?.getState().setFooter({ activeTurnStartedAt: Date.now() });
    updateOctocodeMetricsUi(ctx);
    startMetricsTicker(ctx); // live `active`/`session` durations during the turn
  });
  pi.on('turn_end', async (_event: unknown, ctx: PiContext) => {
    stopMetricsTicker();
    // Evict timing entries for tools whose tool_execution_end never fired
    // (aborted turns) — the map otherwise grows for the session lifetime.
    toolStartTimes.clear();
    toolInputs.clear();
    const now = Date.now();
    const store = runtimeStoreFor(ctx);
    const footer = store?.getState().footer;
    store?.getState().setFooter({
      lastTurnMs: footer?.activeTurnStartedAt !== undefined ? now - footer.activeTurnStartedAt : footer?.lastTurnMs,
      activeTurnStartedAt: undefined,
      completedTurns: (footer?.completedTurns ?? 0) + 1,
    });
    await refreshFooterDirtyState(pi, ctx); // dirty state may have changed this turn; branch comes from Pi footerData
    updateOctocodeMetricsUi(ctx);
  });
}

interface WorkerToolRegistrationArgs {
  pi: PiInstance;
  Type: TypeBoxBuilder;
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
  // Cache the system prompt text: the file doesn't change during a session, so
  // reading it once (lazily on the first before_agent_start) avoids a sync disk
  // read on every turn start across long sessions.
  // Trade-off: if the system prompt file is updated mid-session (e.g. after
  // /octocode-skills-update), the stale cached text persists until session reload.
  // This is intentional — prompt updates take effect on the next Pi session.
      let cachedSystemPromptText: string | null = null;
      let latestAvailableSkills: DiscoveredSkill[] | undefined;
      let latestPiSkills: SkillInfo[] | undefined;
  // One active session per extension instance. Reset on session_start so /new,
  // /resume, and /fork adopt refreshed MCP/skill/config state while every turn
  // inside one session reuses byte-identical provider prompt content.
  let frozenSystemPrompt: string | undefined;
  // Signature of the last plan projection delivered through attributed turn context.
  let deliveredPlanSignature: string | undefined;
  // Unread count last surfaced via cron callback (proactive TUI notify; separate from per-turn LLM injection).
  let lastCronUnreadAlerted = -1;
  // No pi.exec seam → the awareness status job runs in-process (no child).
  const cronScheduler = createOctocodeCronScheduler({
    // Fires once per job run. Refresh the awareness panel immediately and show a
    // TUI notification when new peer messages arrive — closes the 30-min lag gap
    // between message arrival and the next user turn.
    onJobComplete: (result, ctx) => {
      if (result.status !== 'succeeded') return;
      refreshAwarenessPanel(ctx);
      const unread = getCachedAwarenessStatus(ctx?.cwd ?? process.cwd())?.unreadInbox ?? 0;
      if (unread > 0 && unread !== lastCronUnreadAlerted) {
        lastCronUnreadAlerted = unread;
        notify(ctx, `${unread} unread peer message(s) — check inbox at your next turn.`, 'info');
      } else if (unread === 0 && lastCronUnreadAlerted > 0) {
        lastCronUnreadAlerted = 0;
      }
    },
  });
  // Live footer ticker: while a turn is active, re-render the footer every second
  // so `active`/`session` durations advance (they are otherwise only refreshed on
  // turn/session events). Reads are in-memory only (no git/disk per tick); git
  // state is refreshed separately on boundaries. Runs on the shared ui-ticker
  // clock so this and the agent-ledger refresh never double-render the footer
  // from two out-of-phase timers.
  const METRICS_TICK_KEY = 'octocode-metrics';
  const stopMetricsTicker = (): void => setUiTickSubscriber(METRICS_TICK_KEY, undefined);
  // No self-stop guard needed: every site that clears activeTurnStartedAt
  // (turn_end, session_start, session_shutdown) also calls stopMetricsTicker, so
  // the ticker is never left subscribed against an inactive turn.
  const startMetricsTicker = (ctx: PiContext | undefined): void =>
    setUiTickSubscriber(METRICS_TICK_KEY, () => updateOctocodeMetricsUi(ctx));
  const toolStartTimes = new Map<string, number>();
  const toolInputs = new Map<string, unknown>();
  let providerRequestStartedAt: number | undefined;
  const registerSkillContext = (ctx: PiContext, skill: DiscoveredSkill): void => {
    const name = skill.name.trim().toLowerCase();
    registerCurrentContextSource(ctx, {
      version: 1,
      id: `selected-skill:${name}`,
      kind: 'skill',
      origin: `skill-file:${name}`,
      authority: 'project',
      scope: 'task',
      visibility: 'inspectable',
      rehydrate: 'on-trigger',
      capture: false,
      tokenBudget: 30_000,
      readCurrent: () => readTextIfExists(skill.path),
    });
  };
  // Agent inbox handle: assigned during tool registration, referenced by the
  // session_shutdown hook — its suppress flag must flip BEFORE
  // cleanupSpawnedAgentsForShutdown() kills workers, or the teardown burst of
  // killed/exit ledger events would spam desktop notifications.
  let agentInbox: AgentInboxRegistration | undefined;
  let sessionRuntime: SessionRuntime | undefined;
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
  // Checkpoint engine is created lazily on first use (input hook / rewind
  // command) so sessions that never prompt pay no shadow-git init cost.
  let checkpointEnginePromise: Promise<CheckpointEngine | undefined> | undefined;
  const getCheckpointEngine = (ctx?: PiContext): Promise<CheckpointEngine | undefined> => {
    if (!checkpointEnginePromise) {
      const cwd = ctx?.cwd ?? process.cwd();
      checkpointEnginePromise = initCheckpointStore(cwd)
        .then((engine) => {
          // Register a pointer in the session manifest so the session artifact tree
          // tracks where the shadow git checkpoint store lives (it intentionally stays
          // outside the user repo at $OCTOCODE_HOME/extension/checkpoints/<cwd-hash>/).
          if (engine && ctx?.sessionManager) {
            try {
              const artifactCtx = createSessionArtifactContext({ cwd, sessionManager: ctx.sessionManager });
              artifactCtx.writeJson('checkpoint-ref.json', { storeDir: engine.storeDir, cwd: engine.cwd });
              artifactCtx.registerProducer('checkpoint-ref', 'checkpoint-ref.json');
            } catch { /* best-effort — never block checkpointing */ }
          }
          return engine;
        })
        .catch(() => undefined);
    }
    return checkpointEnginePromise;
  };
  // Latest session cwd for the AI! watcher (registration happens before any ctx exists).
  let latestSessionCwd: string | undefined;
  let latestSessionCtx: PiContext | undefined;
  // Feed the watch-mode loop guards: our own file mutations and bash runs
  // cause fs events that must not loop back into the agent as AI! prompts.
  const suppressWatchForTool = (event: { toolName?: string; args?: unknown }, ctx: PiContext | undefined): void => {
    const name = event.toolName ?? '';
    if (name === 'bash') {
      markBashActivity();
      return;
    }
    if (name !== 'edit' && name !== 'write') return;
    const args = event.args as { path?: unknown } | undefined;
    if (typeof args?.path === 'string' && args.path.length > 0) {
      markOwnWrite(resolveFilePath(args.path, ctx?.cwd ?? process.cwd()));
    }
  };

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

    hooks.on('tool_result', 'octocode-model-output-budget', async (event: {
      toolCallId: string;
      toolName: string;
      content: import('./types.js').ContentPart[];
      details?: unknown;
      isError?: boolean;
    }, ctx: PiContext | undefined) => budgetToolResult({
      content: event.content,
      details: event.details,
      isError: event.isError,
    }, {
      ctx,
      toolCallId: event.toolCallId,
      toolName: event.toolName,
    }));

    hooks.on('resources_discover', 'bundled-skills', async () => {
      const paths = getAssetPaths();
      const skillPath = existingDirectory(paths.skillsDir);
      return skillPath ? { skillPaths: [skillPath] } : {};
    });

    hooks.on('tool_call', 'octocode-plan-mode-gate', async (event: { toolName?: string; input?: Record<string, unknown> }, ctx: PiContext | undefined) => {
      const policy = getPlanModePolicy(ctx);
      const receipt = evaluateToolCapability({ toolName: event.toolName, toolInput: event.input, ...(policy ? { phase: policy.phase } : {}) });
      if (!process.env['VITEST']) {
        try {
          const awareness = openPersistentAwareness({ workspace: ctx?.cwd ?? process.cwd() });
          try { awareness.recordCapabilityReceipt(receipt); } finally { awareness.close(); }
        } catch { /* audit persistence cannot weaken the synchronous deny decision */ }
      }
      return planModeToolGate(event.toolName, ctx, event.input);
    });

    hooks.on('tool_call', 'awareness-lock-gate', async (event: { toolName?: string; input?: Record<string, unknown> }, ctx: PiContext | undefined) => {
      return runAwarenessMutationGate(event, ctx);
    });

    // Snapshot every plan mutation into a session CustomEntry (state channel —
    // never rendered, never in LLM context) so /fork and /tree roll plan state
    // back with the conversation instead of leaking the forked-from plan.
    setPlanEntryAppender((steps, rfcPath, decisions, lifecycle, review, coordination, meta, cleared) => pi.appendEntry?.(PLAN_ENTRY_TYPE, { version: 4, cleared, ...review, snapshotId: meta.snapshotId, branchSnapshotId: meta.snapshotId, generation: meta.generation, capturedAt: meta.capturedAt, updatedAt: meta.capturedAt, steps, phase: lifecycle, coordination, ...(rfcPath ? { rfcPath } : {}), ...(decisions && decisions.length ? { decisions } : {}) }));

    hooks.on('session_tree', 'octocode-plan-tree-sync', async (_event: unknown, ctx: PiContext | undefined) => {
      // /tree navigation moved the leaf — re-adopt the plan snapshot that was
      // current on the new branch, and re-render the panel with it.
      const scope = activePlanScope(ctx);
      const adopted = adoptPlanFromBranch(scope, ctx?.sessionManager?.getBranch?.() ?? [], { clearWhenMissing: true });
      if (adopted) adoptPlanModePolicy(ctx, getPlanReviewState(scope));
      else exitPlanMode(ctx);
      if (ctx) runAndRecordRehydration(pi, ctx, 'tree');
      updateOctocodeMetricsUi(ctx);
    });

    const disposeSessionResources = async (reason: string, ctx: PiContext | undefined): Promise<void> => {
      const canUseShutdownContext = reason === 'quit';
      awarenessMutationGate.cleanup();
      updateAwarenessRegistry('leave', pi, undefined, latestSessionCwd);
      cronScheduler.stop();
      stopMcpConfigWatchers();
      stopMetricsTicker();
      runtimeStoreFor(ctx)?.getState().setFooter({ activeTurnStartedAt: undefined });
      suppressAwarenessPanel();
      agentInbox?.shutdown({ restoreTitle: canUseShutdownContext });
      setAgentLedgerMetricsRefreshForUi(undefined);
      setPlanMetricsRefreshForUi(undefined);
      setAwarenessMetricsRefreshForUi(undefined);
      stopWatch();
      // Fix 3: clear the watch chip explicitly so it does not leak into the next
      // session’s TUI paint on non-quit shutdowns (rendererDisposer runs with
      // clearUi:false for ‘new’/‘replace’/‘resume’ and skips individual status clears).
      setManagedStatus(ctx, 'octocode-watch', undefined);
      // Fix 2: clear this session’s registered context sources by ctx identity.
      // The no-ctx clear-all that used to live in compaction-hooks’ session_shutdown
      // handler races with a concurrently starting session, so we clear only the
      // shutting-down session’s entry here, where the ctx is known.
      if (ctx) clearCurrentContextSources(ctx);
      const cleanedAgents = cleanupSpawnedAgentsForShutdown();
      const stoppedMcpServers = stopAllMcpServers();
      const closedChrome = closeAllChromeConnections();
      if (closedChrome > 0 && canUseShutdownContext) notify(ctx, `Closed ${closedChrome} cached CDP connection(s).`, 'info');
      setPeerWipStatusPainter(undefined);
      latestSessionCtx = undefined;
      latestSessionCwd = undefined;
      resetOctocodeFooterRegistration(ctx);
      if (canUseShutdownContext && ctx?.hasUI) {
        if (cleanedAgents > 0) ctx.ui?.notify?.(`Octocode closed ${cleanedAgents} spawned subagent(s).`, 'info');
        if (stoppedMcpServers > 0) ctx.ui?.notify?.(`Octocode stopped ${stoppedMcpServers} MCP server(s).`, 'info');
      }
    };

    const initializeOctocodeSession = async (ctx: PiContext | undefined, reason?: string): Promise<void> => {
      if (ctx) configureInteractionBrokerRoute(ctx, hasHostInteractionAnswerRoute);
      frozenSystemPrompt = undefined;
      deliveredPlanSignature = undefined;
      latestAvailableSkills = undefined;
      latestPiSkills = undefined;
      lastCronUnreadAlerted = -1;
      await sessionRuntime?.dispose('replace');
      const runtime = new SessionRuntime({ ctx, onDispose: (reason) => disposeSessionResources(reason ?? 'shutdown', ctx) });
      sessionRuntime = runtime;
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
      // Snapshot the working tree's pre-session dirty set so file can warn
      // before co-mingling changes into peer/user uncommitted work.
      if (ctx?.cwd) {
        const baselineCwd = ctx.cwd;
        // Wire the peer-WIP chip painter BEFORE the async baseline call so it is
        // already registered when setPeerWipBaseline fires its statusPainter callback
        // (the .then() fires as a microtask, but await points above this block could
        // let it race — wiring first eliminates the race entirely).
        if (ctx.hasUI) {
          setPeerWipStatusPainter((count) => {
            setManagedStatus(
              ctx,
              'octocode-peer-wip',
              count > 0 ? paintUi(ctx.ui, 'warning', `⚑ ${count} pre-existing dirty`) : undefined,
            );
          });
        }
        void execGitSummary(pi, ['status', '--porcelain'], 800).then((porc) => {
          if (runtime.isCurrent()) setPeerWipBaseline(baselineCwd, porc);
        });
      }
      // A new session gets a fresh checkpoint-card dedupe set. Pi owns all
      // compaction retry/continuation state; Octocode keeps no parallel arbiter.
      resetCompactionCheckpointDedupe();
      // Sensitive-action "always allow" consent is session-scoped: a new session
      // must re-earn it, never inherit a prior session's approvals.
      resetApprovalStore();
      // Operator/CI can pin the session's starting level (strict|default|relaxed).
      applyStartupPermissionLevel();
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
      checkpointEnginePromise = undefined;
      // Drop dead worker records so the agent ledger reflects only this session.
      pruneDroppableAgentsForSession();
      runtimeStore.getState().setFooter({
        sessionStartedAt: Date.now(),
        activeTurnStartedAt: undefined,
        lastTurnMs: undefined,
        completedTurns: 0,
        githubAuth: { status: 'checking' },
        usage: undefined,
        gitDirty: undefined,
        gitDirtyFiles: undefined,
      });
      stopMetricsTicker();
      latestSessionCwd = ctx?.cwd;
      latestSessionCtx = ctx;
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
                    listSkills: () => discoverSkills(sessionCwd, latestAvailableSkills),
        });
      }
      // Trim shadow-git checkpoint history in the background (keeps 30).
      initializationTasks.push(runtime.runTask({
        name: 'checkpoints',
        message: 'checking checkpoints',
        readyMessage: 'checkpoints ready',
        run: async () => {
          const engine = await getCheckpointEngine(ctx);
          await engine?.prune();
        },
      }));
      initializationTasks.push(runtime.runTask({
        name: 'dirty-state',
        message: 'checking workspace changes',
        readyMessage: 'workspace state checked',
        run: () => refreshFooterDirtyState(pi, ctx),
      }));
      applyOctocodeUi(ctx, pi.getThinkingLevel?.());
      // Context is not measurable until before_agent_start provides Pi's base
      // prompt and project context. Publish an explicit pending state instead of
      // showing a misleading partial total during initialization.
      if (cachedSystemPromptText === null) {
        cachedSystemPromptText = readTextIfExists(getAssetPaths().systemPrompt);
      }
      const directToolStats = getDirectToolContractStats(registeredToolNames);
      runtimeStore.getState().setContext({
        status: 'pending',
        mode: isCompactMcpEnabled() ? 'compact' : 'exact',
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
      // AI Watch: if OCTOCODE_WATCH=1 auto-started the watcher before this TUI
      // session existed, paint the persistent chip now that we have a UI context.
      // /octocode-watch on|off already paints via the setStatus dep for manual toggles.
      if (ctx?.hasUI && isWatchActive()) {
        setManagedStatus(ctx, 'octocode-watch', 'watch: on');
      }
      cronScheduler.start(ctx);
      // Announce this session in the shared Awareness agent registry with
      // its generated host-tagged name (fire-and-forget; peers see it via
      // `agent list` and can `message send` to it).
      updateAwarenessRegistry('join', pi, ctx);
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
      void liveMcpWarm.then(() => {
        // The old warm may settle after /new invalidates its ctx. Shutdown and
        // the next session both advance this generation before microtasks resume.
        if (!runtime.isCurrent()) return;
        const liveMcpState = runtimeStore.getState().mcp;
        if (liveMcpState.status === 'degraded' || liveMcpState.status === 'failed') {
          runtimeStore.getState().degradeTask('mcp', liveMcpState.message ?? 'MCP live refresh failed');
        }
        writeDiscoveryFile(ctx, {
          skills: discoverSkillStates(sessionCwd, latestAvailableSkills),
          nativeTools: [...registeredToolNames],
        });
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
            `Octocode disabled Pi built-ins (${DISABLED_BUILTIN_TOOL_NAMES.join(', ')}); use MCPTool({queries:[{reasoning:'research the codebase',action:'call',server:'octocode',tool:'...',arguments:{}}]}) for research. Overrides: ${OVERRIDDEN_BUILTIN_TOOL_NAMES.join(', ')}.`,
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
      // Once per process: prompt the user to clean stale clones / tmp dirs if any exist.
      runCleanupOnInit(ctx);
    };

    hooks.on('session_start', 'octocode-session-start', async (event: { reason?: string }, ctx: PiContext | undefined) => {
      try {
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
        await sessionRuntime?.dispose(event.reason);
      } finally {
        sessionRuntime = undefined;
        cleanupEphemeralToolOutputs();
      }
    });

    hooks.on('model_select', 'octocode-model-select', async (_event: unknown, ctx: PiContext | undefined) => {
      // thinking_level_select fires before model_select when the model change
      // clamps the thinking level, so pi.getThinkingLevel() is already updated.
      applyOctocodeUi(ctx, pi.getThinkingLevel?.());
      // refreshAgentLedgerUi refreshes the footer metrics too (via the wired
      // refresher), so calling updateOctocodeMetricsUi here built the footer
      // twice per model switch.
      refreshAgentLedgerUi(ctx);
    });

    hooks.on('thinking_level_select', 'octocode-thinking-select', async (event: ThinkingLevelEvent, ctx: PiContext | undefined) => {
      applyOctocodeUi(ctx, event.level);
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

    hooks.on('input', 'octocode-current-user-request', async (event: { text: string; source?: string }, ctx: PiContext | undefined) => {
      if (!ctx || event.source === 'extension' || !event.text.trim()) return undefined;
      registerCurrentContextSource(ctx, {
        version: 1,
        id: 'current-user-request',
        kind: 'user-request',
        origin: sessionUserRequestOrigin(),
        authority: 'user',
        scope: 'task',
        visibility: 'transcript',
        rehydrate: 'summary-only',
        capture: false,
        readCurrent: readLatestSessionUserRequest,
      });
      return undefined;
    });

    hooks.on('input', 'octocode-repo-state-hint', async (event: { text: string; images?: unknown[]; source?: string; streamingBehavior?: string }) => {
      const repoState = await buildRepoStateHint(pi, event);
      if (!repoState) return { action: 'continue' as const };
      return {
        action: 'transform' as const,
        text: `${event.text}\n\n${repoState}`,
        images: event.images,
      };
    });

    // Auto-snapshot the working tree (shadow git) before each real user prompt
    // so /octocode-rewind can restore files. Fire-and-forget inside the hook —
    // it never blocks input. The hook's { action: 'continue' } result is
    // swallowed: the composer merges middleware results by object spread, so
    // returning it here would clobber another input middleware's transform.
    const checkpointInputHook = createCheckpointInputHook({ getEngine: getCheckpointEngine });
    hooks.on('input', 'octocode-checkpoint-snapshot', async (event: { text: string; source?: string; streamingBehavior?: string }, ctx: PiContext | undefined) => {
      await checkpointInputHook(event, ctx);
      return undefined;
    });

    hooks.on('tool_execution_start', 'octocode-tool-error-timing', async (event: { toolCallId?: string; toolName?: string; args?: unknown }, ctx: PiContext | undefined) => {
      const key = event.toolCallId ?? event.toolName;
      if (key) {
        toolStartTimes.set(key, Date.now());
        toolInputs.set(key, event.args);
      }
      suppressWatchForTool(event, ctx);
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
      if (event.toolName === 'bash') markBashActivity();
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
            const skill = latestAvailableSkills?.find((candidate) => candidate.name.toLowerCase() === requested.trim().toLowerCase());
            if (skill) registerSkillContext(ctx, skill);
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

    // Warn-once guards: the strip regexes depend on Pi's exact wording/tags, so a
    // Pi upgrade could make them silently no-op. If a strip is requested but the
    // target markers are still present afterwards, surface it once instead of
    // shipping a double catalog / leaked context in silence.
    let warnedContextDrift = false;
    let warnedSkillsDrift = false;
    hooks.on('before_agent_start', 'octocode-system-prompt', async (event: BeforeAgentStartEvent, ctx: PiContext | undefined) => {
      // Custom Anthropic-compatible providers do not inherit Pi's built-in model
      // compatibility metadata. Normalize known adaptive models before Pi builds
      // the provider request, while preserving an explicit provider override.
      ensureAdaptiveThinkingCompatibility(ctx?.model);

      // Suppress AGENTS.md / CLAUDE.md when --no-context flag is set. Pi builds
      // the prompt BEFORE this hook fires and systemPromptOptions is
      // inspection-only, so the block must be stripped from the assembled
      // prompt text. (octocode-agent sessions also pass --no-context-files to
      // pi, which prevents the block at the source for that path.)
      const noContext = Boolean(pi.getFlag?.('no-context'));
      let piPrompt = event.systemPrompt;
      if (noContext) {
        piPrompt = stripProjectContext(event.systemPrompt);
        if (!warnedContextDrift && piPrompt.includes('<project_context>')) {
          warnedContextDrift = true;
          console.warn('[octocode-pi-extension] --no-context set but <project_context> remains after strip — Pi prompt format may have changed; update stripProjectContext.');
        }
      }

      // Workers that load this extension need Octocode tools, not the full main-agent
      // prompt layered over their typed or caller-provided --append-system-prompt file.
      // NOTE: subagents intentionally skip stripPiSkillsSection below — their prompt is
      // assembled from their own typed/append config, not the main-agent skill flow.
      if (isSubagentProcess()) {
        return piPrompt !== event.systemPrompt ? { systemPrompt: piPrompt } : undefined;
      }

      // Refresh shared Awareness state on every turn — previously skipped on frozen turns
      // because they sat after the freeze early-return.
      refreshAwarenessPanel(ctx);

      // Compute the canonical model-facing plan projection once. Plans are mutable
      // task state, so they are delivered through attributed turn context and are
      // never embedded in the frozen system prompt.
      const planScope = activePlanScope(ctx);
      bumpPlanTurn(planScope);
      // catches lifecycle, RFC revision, decisions, dependencies, acceptance,
      // verification, and Awareness mapping changes—not only status/id changes.
      const planContext = renderPlanContext(getCurrentPlanReadModel(ctx, planScope));
      const planSig = planContext;
      const planChanged = planSig !== deliveredPlanSignature;
      const planDeliveryContent = planChanged
        ? planContext || (deliveredPlanSignature === undefined ? '' : 'Plan cleared; no active task breakdown remains.')
        : '';
      const livePlanContents: Record<string, string> = { 'active-plan': planContext };
      const livePlanAssembly = assembleContextSegments([
        { id: 'active-plan', content: planContext, kind: 'plan', origin: 'plan-domain', authority: 'user', scope: 'task', visibility: 'transcript', rehydrate: 'always', tokenBudget: 15_000 },
      ]);

      const currentSourcesFrom = (manifest: ReturnType<typeof assembleContextSegments>['manifest'], contents: Record<string, string>): CurrentRehydrationSource[] =>
        manifest.map((segment) => ({ segment, content: contents[segment.id] ?? '' }));
      let frozenRehydration: ReturnType<typeof consumeValidatedRehydration>;
      if (ctx && frozenSystemPrompt !== undefined) {
        const currentMcpCatalog = getCachedMcpCatalogAddendum(ctx);
        const currentRuntimeCapabilities = renderRuntimeCapabilitiesAddendum(ctx);
        const currentSkills = renderAvailableSkillsAddendum(latestAvailableSkills);
        const currentDynamic = getDynamicCapabilitiesAddendum(latestAvailableSkills?.map((skill) => skill.name));
        const currentContents: Record<string, string> = {
          'octocode-product-policy': cachedSystemPromptText ?? '',
          'mcp-tool-contracts': currentMcpCatalog,
          'runtime-tool-contracts': currentRuntimeCapabilities,
          'dynamic-tool-contracts': currentDynamic,
          'available-skills': currentSkills,
        };
        const currentAssembly = assembleContextSegments([
          { id: 'octocode-product-policy', content: currentContents['octocode-product-policy']!, kind: 'product-policy', origin: 'octocode-harness', authority: 'product', scope: 'session', visibility: 'hidden-policy', rehydrate: 'always', tokenBudget: 20_000 },
          { id: 'mcp-tool-contracts', content: currentMcpCatalog, kind: 'tool-contract', origin: 'octocode-harness', authority: 'product', scope: 'session', visibility: 'inspectable', rehydrate: 'always', tokenBudget: 30_000 },
          { id: 'runtime-tool-contracts', content: currentRuntimeCapabilities, kind: 'tool-contract', origin: 'octocode-harness', authority: 'product', scope: 'session', visibility: 'inspectable', rehydrate: 'always', tokenBudget: 10_000 },
          { id: 'dynamic-tool-contracts', content: currentDynamic, kind: 'tool-contract', origin: 'octocode-harness', authority: 'product', scope: 'session', visibility: 'inspectable', rehydrate: 'always', tokenBudget: 20_000 },
          { id: 'available-skills', content: currentSkills, kind: 'skill', origin: 'installed-skills', authority: 'project', scope: 'session', visibility: 'inspectable', rehydrate: 'on-trigger', tokenBudget: 20_000 },
        ], { totalTokenBudget: INITIAL_CONTEXT_TOKEN_BUDGET });
        frozenRehydration = consumeValidatedRehydration(
          ctx,
          mergeCurrentContextSources(ctx, [
            ...currentSourcesFrom(currentAssembly.manifest, currentContents),
            ...currentSourcesFrom(livePlanAssembly.manifest, livePlanContents),
          ], { totalTokenBudget: INITIAL_CONTEXT_TOKEN_BUDGET }),
          { allowProjection: true },
        );
        if (frozenRehydration) pi.appendEntry?.(REHYDRATION_RECEIPT_ENTRY_TYPE, frozenRehydration.receipt);
      }

      // Combine all per-turn context signals into one message (only one message
      // per turn is supported by BeforeAgentStartEventResult). The plan appears
      // only when first delivered, changed, or cleared.
      const contextAssembly = assembleContextSegments([
        { id: 'active-plan', content: planDeliveryContent, kind: 'plan', origin: 'plan-domain', authority: 'user', scope: 'task', visibility: 'transcript', rehydrate: 'always', tokenBudget: 15_000 },
      ]);
      const contextMessage =
        contextAssembly.manifest.length > 0 || frozenRehydration?.content
          ? { customType: 'octocode-context-update', content: [contextAssembly.content, frozenRehydration?.content].filter(Boolean).join('\n\n'), display: false, details: { version: 1, segments: [...contextAssembly.manifest, ...(frozenRehydration?.segments ?? [])], ...(frozenRehydration ? { rehydration: frozenRehydration.receipt } : {}) } }
          : undefined;

      if (frozenSystemPrompt !== undefined) {
        deliveredPlanSignature = planSig;
        return contextMessage
          ? { systemPrompt: frozenSystemPrompt, message: contextMessage }
          : { systemPrompt: frozenSystemPrompt };
      }

      // Octocode owns the model-facing skill flow (the `skill` tool + the
      // <available_skills> addendum) — deterministically drop Pi's read-based
      // skills section so the model never sees two catalogs or a `read`
      // instruction for a tool this harness removes.
      piPrompt = stripPiSkillsSection(piPrompt);
      if (!warnedSkillsDrift && piPrompt.includes('The following skills provide specialized instructions')) {
        warnedSkillsDrift = true;
        console.warn('[octocode-pi-extension] Pi skills section still present after strip — Pi prompt wording may have changed; update stripPiSkillsSection (the model may see two skill catalogs).');
      }
      const stripped = piPrompt !== event.systemPrompt;

      if (cachedSystemPromptText === null) {
        cachedSystemPromptText = readTextIfExists(getAssetPaths().systemPrompt);
      }
      // Bounded wait for init-time MCP discovery. The first turn receives either
      // the compact <mcp_catalog_index> (default) or the explicit exact
      // <mcp_catalog>; subsequent turns reuse the same session bytes.
      await mcpCatalogReady(ctx);
      const mcpCatalog = getCachedMcpCatalogAddendum(ctx);
      // Resolve one effective, loadable skill inventory for the prompt, skill
      // tool, dashboard, autocomplete, and discovery artifact. Pi metadata wins
      // when it has a path; prompt-only entries resolve through shared roots.
        latestPiSkills = event.systemPromptOptions?.skills;
        latestAvailableSkills = discoverSkills(
          ctx?.cwd ?? process.cwd(),
          latestPiSkills,
      );
      if (ctx) latestAvailableSkills.forEach((skill) => registerSkillContext(ctx, skill));
      // Dynamic skills with the same case-insensitive name are omitted: the
      // installed skill owns the unqualified routing name.
      const dynamicCatalog = getDynamicCapabilitiesAddendum(latestAvailableSkills.map((skill) => skill.name));
      const availableSkills = renderAvailableSkillsAddendum(latestAvailableSkills);
      const runtimeCapabilities = renderRuntimeCapabilitiesAddendum(ctx);
      // The initial live plan is delivered through contextAssembly above. Stable
      // policy/catalog fragments alone are frozen; compaction captures the live
      // plan independently through its attributed plan segment.
      const promptAssembly = assembleContextSegments([
        { id: 'octocode-product-policy', content: cachedSystemPromptText, kind: 'product-policy', origin: 'octocode-harness', authority: 'product', scope: 'session', visibility: 'hidden-policy', rehydrate: 'always', tokenBudget: 20_000 },
        { id: 'mcp-tool-contracts', content: mcpCatalog, kind: 'tool-contract', origin: 'octocode-harness', authority: 'product', scope: 'session', visibility: 'inspectable', rehydrate: 'always', tokenBudget: 30_000 },
        { id: 'runtime-tool-contracts', content: runtimeCapabilities, kind: 'tool-contract', origin: 'octocode-harness', authority: 'product', scope: 'session', visibility: 'inspectable', rehydrate: 'always', tokenBudget: 10_000 },
        { id: 'dynamic-tool-contracts', content: dynamicCatalog, kind: 'tool-contract', origin: 'octocode-harness', authority: 'product', scope: 'session', visibility: 'inspectable', rehydrate: 'always', tokenBudget: 20_000 },
        { id: 'available-skills', content: availableSkills, kind: 'skill', origin: 'installed-skills', authority: 'project', scope: 'session', visibility: 'inspectable', rehydrate: 'on-trigger', tokenBudget: 20_000 },
      ], { totalTokenBudget: INITIAL_CONTEXT_TOKEN_BUDGET });
      const initialContents: Record<string, string> = {
        'octocode-product-policy': cachedSystemPromptText,
        'mcp-tool-contracts': mcpCatalog,
        'runtime-tool-contracts': runtimeCapabilities,
        'dynamic-tool-contracts': dynamicCatalog,
        'available-skills': availableSkills,
      };
      const initialRehydration = ctx
        ? consumeValidatedRehydration(
            ctx,
            mergeCurrentContextSources(ctx, [
              ...currentSourcesFrom(promptAssembly.manifest, initialContents),
              ...currentSourcesFrom(livePlanAssembly.manifest, livePlanContents),
            ], { totalTokenBudget: INITIAL_CONTEXT_TOKEN_BUDGET }),
            { allowProjection: false },
          )
        : undefined;
      if (initialRehydration) pi.appendEntry?.(REHYDRATION_RECEIPT_ENTRY_TYPE, initialRehydration.receipt);
      const prompt = promptAssembly.content;
      // Frozen system segments survive Pi compaction and are reloaded from their
      // owners on session start. Copying them into the recovery ledger only
      // duplicates prompt bytes on disk; none is eligible for reprojection.
      setCompactionRehydrationSegmentsProvider(() => ({ segments: [], contents: {} }));
      // Build once, then freeze these exact stable bytes for the session. Mutable
      // plan state remains outside the system prompt in attributed turn context.
      const resolvedPrompt = prompt.trim().length === 0
        ? piPrompt
        : composeSystemPrompt({
          piSystemPrompt: piPrompt,
          octocodePrompt: prompt,
          promptMode,
        });
      // Snapshot the complete initial provider context. Direct tool contracts
      // are sent beside the system prompt, so count their descriptions + JSON
      // schemas separately and expose the combined initial subtotal.
      const mcpCounts = getCachedMcpCounts(ctx);
      const directToolStats = getDirectToolContractStats(registeredToolNames);
      const turnContextChars = contextMessage?.content.length ?? 0;
      const dynamicChars = runtimeCapabilities.length + dynamicCatalog.length + availableSkills.length + turnContextChars;
      const providerSubtotalChars = resolvedPrompt.length + directToolStats.totalChars + turnContextChars;
      const estimatedProviderTokens = assertContextTokenBudget(
        'initial provider context',
        providerSubtotalChars,
        PROVIDER_CONTEXT_TOKEN_BUDGET,
      );
      runtimeStoreFor(ctx)?.getState().setContext({
        status: 'frozen',
        mode: isCompactMcpEnabled() ? 'compact' : 'exact',
        systemPromptChars: resolvedPrompt.length,
        mcpChars: mcpCatalog.length,
        dynamicChars,
        directToolChars: directToolStats.totalChars,
        providerSubtotalChars,
        estimatedTokens: estimatedProviderTokens,
        mcpServers: mcpCounts.servers,
        mcpTools: mcpCounts.tools,
        skills: latestAvailableSkills.length,
      });
      void writeDiscoveryFile(ctx, {
        skills: discoverSkillStates(ctx?.cwd ?? process.cwd(), latestAvailableSkills),
        nativeTools: [...registeredToolNames],
        overhead: {
          sysChars: piPrompt.length + (cachedSystemPromptText?.length ?? 0),
          mcpChars: mcpCatalog.length,
          dynamicChars,
          totalChars: resolvedPrompt.length + turnContextChars,
          directToolChars: directToolStats.totalChars,
          mcpServers: mcpCounts.servers,
          mcpTools: mcpCounts.tools,
          skills: latestAvailableSkills.length,
          status: 'frozen',
          mode: isCompactMcpEnabled() ? 'compact' : 'exact',
        },
      });
      frozenSystemPrompt = resolvedPrompt;
      deliveredPlanSignature = planSig;
      if (resolvedPrompt === event.systemPrompt && !stripped) {
        return contextMessage ? { message: contextMessage } : undefined;
      }
      return contextMessage
        ? { systemPrompt: resolvedPrompt, message: contextMessage }
        : { systemPrompt: resolvedPrompt };
    });
  }

  if (pi.registerTool) {
    const { Type } = await import('typebox');

    registerSupportToolPhase({
      pi,
      Type,
      registeredToolNames,
      notify,
      getLatestAvailableSkills: () => latestAvailableSkills,
    });
    registerRuntimeUiPhase({ pi, notify });
  registerTurnMetricsPhase({ pi, startMetricsTicker, stopMetricsTicker, toolStartTimes, toolInputs });
  agentInbox = registerWorkerToolPhase({ pi, Type, registeredToolNames, notify });

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
          if (runtimeStoreFor(ctx)?.getState().activity.kind === 'idle') {
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

  if (!pi.registerCommand) return;

  pi.registerCommand('octocode', {
    description: 'Extension health & setup dashboard: status, tools, setup, skills, health. For live work state use /octocode-now.',
    handler: async (_args, ctx) => {
      notify(ctx, formatOctocodeDashboard(ctx, undefined, formatOctocodeCronSummary(cronScheduler.list())), 'info');
    },
  });

  pi.registerCommand('octocode-now', {
    description: 'Live work cockpit: model, context, permissions, current plan, shared tasks, agents, and git status. For extension health use /octocode.',
    handler: async (_args, ctx) => {
      const level = getPermissionLevel();
      const grants = approvedClasses();
      const permLine = `permissions: ${level} (${PERMISSION_LEVEL_SUMMARY[level]})${grants.length > 0 ? ` · always-allowed: ${grants.join(', ')}` : ''}`;
      notify(ctx, `${await formatOctocodeNow(ctx, pi)}\n${permLine}`, 'info');
    },
  });

  // ─── /octocode-harness — full skill/MCP/tool/prompt inventory ────────────────

  pi.registerCommand('octocode-harness', {
    description: 'Harness inventory: all skills and MCP servers (with sources and tool names), native tools, and prompt overhead breakdown.',
    handler: async (_args, ctx) => {
      const contextState = runtimeStoreFor(ctx)?.getState().context;
      const skills = discoverSkills(ctx?.cwd ?? process.cwd(), latestAvailableSkills);
      const mcpSnap = await getMcpDiscoverySnapshot(ctx);
      const tools = [...registeredToolNames].sort((a, b) => a.localeCompare(b));

      // Compact char/token formatters.
      const fc = (n: number): string => n >= 1000 ? `~${(Math.round(n / 100) / 10).toFixed(1)}k` : `${n}`;
      const ft = (chars: number): string => fc(Math.round(chars / 4));

      const lines: string[] = ['Octocode Harness  @octocodeai/pi-extension'];

      // Model / session
      const modelId = (ctx as { model?: { id?: string; provider?: string } } | undefined)?.model;
      if (modelId?.id || pi.getThinkingLevel?.()) {
        const parts = [
          ...(modelId?.id ? [modelId.id] : []),
          ...(modelId?.provider ? [modelId.provider] : []),
          ...(pi.getThinkingLevel?.() ? [`thinking ${pi.getThinkingLevel?.()}`] : []),
        ];
        lines.push('', `Model    ${parts.join(' · ')}`);
      }

      // Initial provider context. Conversation messages and provider-specific
      // framing are intentionally excluded because Pi does not expose their
      // serialized bytes before the request.
      if (contextState?.status === 'frozen') {
        lines.push(
          '',
          `Initial provider subtotal   ${ft(contextState.providerSubtotalChars)} tokens  (${fc(contextState.providerSubtotalChars)} chars)`,
          `  system prompt   ${fc(contextState.systemPromptChars).padEnd(8)} chars   Pi + project context + Octocode`,
          `    mcp catalog   ${fc(contextState.mcpChars).padEnd(8)} chars   ${contextState.mode} · ${contextState.mcpServers} server${contextState.mcpServers !== 1 ? 's' : ''} · ${contextState.mcpTools} tool${contextState.mcpTools !== 1 ? 's' : ''}`,
          `    dynamic       ${fc(contextState.dynamicChars).padEnd(8)} chars   ${contextState.skills} skills · initial plan · capabilities`,
          `  direct tools    ${fc(contextState.directToolChars).padEnd(8)} chars   descriptions + input schemas`,
          '  excludes conversation messages and provider-specific request framing',
        );
      } else {
        lines.push('', 'Initial provider subtotal   pending first agent start');
      }

      // Native tools
      lines.push('', `Native tools  (${tools.length})`);
      // Wrap long tool lists at ~90 chars per line for readability.
      const toolText = tools.join(' · ');
      if (toolText.length <= 88) {
        lines.push(`  ${toolText}`);
      } else {
        // Split into ~90-char chunks at · boundaries.
        const chunks: string[] = [];
        let current = '';
        for (const t of tools) {
          const next = current ? `${current} · ${t}` : t;
          if (next.length > 88 && current) { chunks.push(current); current = t; }
          else { current = next; }
        }
        if (current) chunks.push(current);
        for (const chunk of chunks) lines.push(`  ${chunk}`);
      }

      // Skills
      lines.push('', `Skills  (${skills.length})   ←  load: skill({queries:[{reasoning:"load matching skill",type:"load",action:"load",name:"…",reason:"why"}]})`);
      for (const skill of skills) {
        const descTrunc = skill.description
          ? `  ${skill.description.length > 64 ? `${skill.description.slice(0, 63)}…` : skill.description}`
          : '';
        lines.push(`  ${skill.name.padEnd(32)}[${skill.source}]${descTrunc}`);
      }

      // MCP servers
      lines.push('', `MCP servers  (${mcpSnap.servers.length})`);
      if (mcpSnap.servers.length === 0) {
        lines.push('  (none configured — add via MCPTool action:"add")');
      }
      for (const server of mcpSnap.servers) {
        const tc = server.toolCount !== undefined ? `  (${server.toolCount} tools)` : '';
        lines.push(`  ${server.name}   cmd: ${server.command}${server.args?.length ? ` ${server.args.join(' ')}` : ''}${tc}`);
        if (server.tools?.length) {
          const names = server.tools.map((t) => t.name);
          // Show up to ~8 tool names, then "+ N more".
          const MAX_SHOW = 8;
          const shown = names.slice(0, MAX_SHOW).join(', ');
          const rest = names.length > MAX_SHOW ? `  + ${names.length - MAX_SHOW} more` : '';
          lines.push(`    ${shown}${rest}`);
        }
      }

      // Builtin tool policy (overrides, removed, passthrough)
      const harness = listExtensionHarness();
      if (harness.overriddenBuiltins.length || harness.disabledBuiltins.length) {
        lines.push('', 'Builtin tool policy');
        if (harness.overriddenBuiltins.length)
          lines.push(`  overridden   ${harness.overriddenBuiltins.join(', ')}`);
        if (harness.disabledBuiltins.length)
          lines.push(`  removed      ${harness.disabledBuiltins.join(', ')}`);
        if (harness.passthroughBuiltins.length)
          lines.push(`  passthrough  ${harness.passthroughBuiltins.join(', ')}`);
      }

      // Discovery file pointer
      const discoveryPath = getDiscoveryFilePath(ctx?.cwd ?? process.cwd());
      lines.push('', `Full inventory  ${discoveryPath}`);

      notify(ctx, lines.join('\n'), 'info');
    },
  });

  pi.registerCommand('octocode-profile', {
    description: 'Apply a named Octocode profile from ~/.octocode/profiles.json to this live Pi session: model, tools/excludeTools, and approve mode.',
    getArgumentCompletions: (prefix: string) => listProfileNames()
      .filter((name) => name.startsWith(prefix))
      .map((name) => ({ value: name, label: name, description: `/octocode-profile ${name}` })),
    handler: async (args, ctx) => {
      const name = args.trim();
      if (!name) {
        const names = listProfileNames();
        notify(ctx, names.length > 0
          ? `Available profiles: ${names.join(', ')}\nUsage: /octocode-profile <name>`
          : `No profiles found at ${profileFilePath()}. Usage: /octocode-profile <name>`, 'info');
        return;
      }
      const profile = loadProfile(name, extensionHome(getOctocodeHome(process.env)));
      if (!profile) {
        notify(ctx, `Profile "${name}" not found in ${profileFilePath()}.`, 'warning');
        return;
      }
      const { lines, warnings } = await applyRuntimeProfile(pi, ctx, name, profile);
      const warningText = warnings.length > 0 ? `\nWarnings:\n- ${warnings.join('\n- ')}` : '';
      notify(ctx, `Applied profile "${name}" live:\n- ${lines.join('\n- ')}${warningText}`, warnings.length > 0 ? 'warning' : 'info');
    },
  });

  pi.registerCommand('octocode-tasks', {
    description: 'Show local plan and shared Awareness task/verification state with guidance on which surface to use.',
    handler: async (_args, ctx) => {
      refreshAwarenessPanel(ctx);
      notify(ctx, formatOctocodeTasks(ctx), 'info');
    },
  });

  pi.registerCommand('octocode-skills', {
    description: 'Show Pi-discovered skills and how to load or install them.',
    handler: async (_args, ctx) => {
      const discoveryPath = getDiscoveryFilePath(ctx?.cwd ?? process.cwd());
                notify(ctx, renderSkillsDashboard(discoverSkills(ctx?.cwd ?? process.cwd(), latestAvailableSkills), {
        usageLines: formatSkillUsageLines(),
        discoveryPath: fs.existsSync(discoveryPath) ? discoveryPath : undefined,
      }), 'info');
    },
  });

  pi.registerCommand('octocode-chrome', {
    description: 'List or close reused CDP connections: /octocode-chrome [list|close].',
    getArgumentCompletions: (prefix: string) => ['list', 'close']
      .filter((s) => s.startsWith(prefix))
      .map((s) => ({ value: s, label: s, description: `octocode-chrome ${s}` })),
    handler: async (args, ctx) => {
      const arg = String(args ?? '').trim().toLowerCase() || 'list';
      if (arg === 'close') {
        const n = closeAllChromeConnections();
        notify(ctx, `Closed ${n} cached CDP connection(s).`, 'info');
        return;
      }
      const sessions = listCDPSessions();
      if (sessions.length === 0) { notify(ctx, 'No cached CDP connections.', 'info'); return; }
      const lines = sessions.map((s) =>
        `• :${s.port} ${s.mode} target=${s.targetId.slice(0, 8)} uses=${s.uses} idle=${Math.round(s.idleMs / 1000)}s ${s.closed ? '(closed)' : ''} ${s.url}`,
      );
      notify(ctx, `Cached CDP connections (${sessions.length}):\n${lines.join('\n')}`, 'info');
    },
  });

  pi.registerCommand('octocode-theme', {
    description: 'Apply an Octocode theme: /octocode-theme [sync|dark|light]. sync follows the system appearance (macOS) or terminal background (COLORFGBG).',
    getArgumentCompletions: (prefix: string) => ['sync', 'dark', 'light']
      .filter((s) => s.startsWith(prefix))
      .map((s) => ({ value: s, label: s, description: `octocode-${s === 'sync' ? 'dark|light (auto)' : s}` })),
    handler: async (args, ctx) => {
      const arg = String(args ?? '').trim().toLowerCase() || 'sync';
      let themeName: OctocodeThemeName | null;
      if (arg === 'dark') themeName = OCTOCODE_THEME_DARK;
      else if (arg === 'light') themeName = OCTOCODE_THEME_LIGHT;
      else {
        // sync: cross-platform detection. macOS via AppleInterfaceStyle; other
        // platforms via the terminal's COLORFGBG background code. Undetectable
        // (e.g. Linux without COLORFGBG) => keep the current theme, don't force light.
        let appleInterfaceStyle = '';
        if (process.platform === 'darwin' && pi.exec) {
          try {
            const r = await pi.exec('defaults', ['read', '-g', 'AppleInterfaceStyle'], { timeout: 1500 });
            if (r?.code === 0) appleInterfaceStyle = r.stdout.trim();
          } catch { /* key unset in light mode */ }
        }
        themeName = resolveSystemThemeName({
          platform: process.platform,
          appleInterfaceStyle,
          colorfgbg: process.env['COLORFGBG'],
        });
        if (!themeName) {
          notify(ctx, 'Could not detect system appearance on this platform. Use /octocode-theme dark|light.', 'warning');
          return;
        }
      }
      const result = ctx.ui?.setTheme?.(themeName);
      if (result && result.success === false) notify(ctx, `Could not apply ${themeName}: ${result.error ?? 'unknown error'}`, 'error');
      else notify(ctx, `Applied ${themeName}.`, 'info');
    },
  });

  pi.registerCommand('octocode-footer', {
    description: 'Set footer density (compact | default | full) or show the segment legend (legend).',
    getArgumentCompletions: (prefix: string) => (['compact', 'default', 'full', 'legend'] as const)
      .filter((mode) => mode.startsWith(prefix))
      .map((mode) => ({ value: mode, label: mode, description: `/octocode-footer ${mode}` })),
    handler: async (args, ctx) => {
      const requested = args.trim();
      if (requested === 'legend') {
        // The footer vocabulary is terse by design — this is its decoder ring.
        notify(ctx, `Footer legend:\n${FOOTER_LEGEND.map(([seg, meaning]) => `  ${seg} — ${meaning}`).join('\n')}`, 'info');
        return;
      }
      if (requested.length > 0) {
        const density = parseFooterDensity(requested);
        if (!density) {
          notify(ctx, `Unknown footer density "${requested}". Usage: /octocode-footer [compact|default|full|legend]`, 'warning');
          return;
        }
        setFooterDensity(density);
      updateOctocodeMetricsUi(ctx);
      }
      notify(ctx, `Footer density: ${getFooterDensity()} · /octocode-footer legend explains each segment`, 'info');
    },
  });

  // shift+tab (Claude Code's mode-cycle key) maps to pi's RESERVED
  // app.thinking.cycle — extension shortcuts on reserved keys are silently
  // skipped at resolution, so the default must be an unbound key. Users who
  // remap app.thinking.cycle in keybindings.json can set
  // OCTOCODE_PERMISSIONS_KEY=shift+tab and it will win the key.
  const permissionsKey = process.env['OCTOCODE_PERMISSIONS_KEY'] || 'ctrl+shift+a';

  pi.registerCommand('octocode-permissions', {
    description: 'Session approval controls: show the permission level and always-allowed classes, set the level (strict|default|relaxed), or revoke remembered approvals.',
    getArgumentCompletions: (prefix: string) => [
      ...PERMISSION_LEVELS.map((level) => ({ value: `level ${level}`, label: `level ${level}`, description: `/octocode-permissions level ${level}` })),
      { value: 'revoke all', label: 'revoke all', description: 'forget every always-allow' },
      ...APPROVAL_CLASSES.map((cls) => ({ value: `revoke ${cls}`, label: `revoke ${cls}`, description: `re-prompt for ${cls}` })),
    ].filter((item) => item.value.startsWith(prefix)),
    handler: async (args, ctx) => {
      const [verb, value] = args.trim().split(/\s+/);
      if (verb === 'level') {
        const level = parsePermissionLevel(value);
        if (!level) {
          notify(ctx, `Unknown level "${value ?? ''}". Usage: /octocode-permissions level [strict|default|relaxed]`, 'warning');
          return;
        }
        setPermissionLevel(level);
      } else if (verb === 'revoke') {
        if (value === 'all') {
          approvedClasses().forEach(revokeAlways);
        } else if ((APPROVAL_CLASSES as readonly string[]).includes(value ?? '')) {
          revokeAlways(value as ApprovalClass);
        } else {
          notify(ctx, `Unknown class "${value ?? ''}". Usage: /octocode-permissions revoke [all|${APPROVAL_CLASSES.join('|')}]`, 'warning');
          return;
        }
      } else if (verb) {
        notify(ctx, 'Usage: /octocode-permissions [level strict|default|relaxed] [revoke all|<class>]', 'warning');
        return;
      }
      const remembered = approvedClasses();
      const rememberedStr = remembered.length > 0 ? remembered.join(', ') : 'none';
      const level = getPermissionLevel();
      notify(ctx, `Permission level: ${level} (${PERMISSION_LEVEL_SUMMARY[level]}) · always-allowed this session: ${rememberedStr} · ${permissionsKey} cycles the level`, 'info');
    },
  });

  // Cycle the permission level from the keyboard (Claude Code muscle memory —
  // its shift+tab; see the reserved-key note above for why ours defaults to
  // ctrl+shift+a). Best-effort like the palette shortcut: host rejection
  // degrades to command-only (/octocode-permissions).
  try {
    if (typeof pi.registerShortcut === 'function') {
      pi.registerShortcut(permissionsKey, {
        description: 'Cycle the Octocode permission level (default → relaxed → strict)',
        handler: async (ctx: PiContext | undefined) => {
          const level = cyclePermissionLevel();
        updateOctocodeMetricsUi(ctx);
          notify(ctx, `Permissions: ${level} — ${PERMISSION_LEVEL_SUMMARY[level]}`, level === 'relaxed' ? 'warning' : 'info');
        },
      });
    }
  } catch {
    // Restricted built-in on that key — the slash command still covers it.
  }

  registerCommandsCommand(pi, () => runtimeStoreFor(latestSessionCtx)?.getState().footer.githubAuth ?? { status: 'checking' });
  registerCleanupCommand(pi);

  // octocode-harness is registered earlier with the full inventory view.

  pi.registerCommand('octocode-plan', {
    description: `Plan mode (new <goal>), or show/complete/start/remove/clear the active task plan (usage: ${OCTOCODE_PLAN_COMMAND_USAGE}).`,
    getArgumentCompletions: (prefix) => OCTOCODE_PLAN_COMMAND_COMPLETIONS
      .filter((cmd) => cmd.startsWith(prefix))
      .map((cmd) => ({ value: cmd, label: cmd.trim(), description: `/octocode-plan ${cmd}` })),
    handler: async (args, ctx) => {
      const send = typeof pi.sendUserMessage === 'function'
        ? (text: string) => pi.sendUserMessage!(text, { deliverAs: 'followUp' })
        : undefined;
      await handleOctocodePlanCommand(args, ctx, notify, send);
    },
  });
  pi.registerCommand('octocode-agents', {
    description: `Show, refresh, inspect, prune, hide, or kill Octocode spawned worker agents (usage: ${OCTOCODE_AGENTS_COMMAND_USAGE}).`,
    getArgumentCompletions: (prefix: string) => {
      return OCTOCODE_AGENTS_COMMAND_COMPLETIONS
        .filter((s) => s.startsWith(prefix))
        .map((s) => ({ value: s, label: s.trim(), description: OCTOCODE_AGENTS_COMMAND_DESCRIPTIONS[s] }));
    },
    handler: async (args, ctx) => {
      await handleOctocodeAgentsCommand(args, ctx);
    },
  });

  const cronCommand: CommandDefinition = {
    description: `List, check, or cancel Octocode session jobs (usage: ${OCTOCODE_CRON_COMMAND_USAGE}).`,
    getArgumentCompletions: (prefix: string) => {
      return OCTOCODE_CRON_COMMAND_COMPLETIONS
        .filter((s) => s.startsWith(prefix))
        .map((s) => ({ value: s, label: s, description: `/octocode-cron ${s}` }));
    },
    handler: async (args, ctx) => {
      await handleOctocodeCronCommand(args, ctx, cronScheduler, notify);
    },
  };
  pi.registerCommand('octocode-cron', cronCommand);

  const settingsSections = ['overview', 'runtime', 'appearance', 'models', 'hooks', 'plugins', 'commands', 'skills', 'connections', 'add-server', 'sources', 'agent-context', 'overrides', 'diagnostics'] as const;
  const settingsCommand: CommandDefinition = {
    description: 'Open settings.html with all discovered skills, MCP connections/tools, configuration, and enablement.',
    getArgumentCompletions: (prefix: string) => settingsSections
      .filter((section) => section.startsWith(prefix.trim().toLowerCase()))
      .map((section) => ({ value: section, label: section, description: `Open settings.html#${section}` })),
    handler: async (args, ctx) => {
      const requested = args.trim().toLowerCase();
      const section = settingsSections.includes(requested as (typeof settingsSections)[number])
        ? requested as (typeof settingsSections)[number]
        : 'overview';
      const opened = await openMcpManager(ctx, latestPiSkills, section, collectPublicCommands(pi));
      notify(ctx, opened.ok ? `Octocode settings opened${opened.url ? `: ${opened.url}` : ''}` : (opened.message ?? 'Could not open Octocode settings.'), opened.ok ? 'info' : 'error');
    },
  };
  pi.registerCommand('settings', settingsCommand);
  pi.registerCommand('mcp', {
    ...settingsCommand,
    description: 'Open settings.html#connections for MCP servers and tools.',
    handler: async (_args, ctx) => {
      const opened = await openMcpManager(ctx, latestPiSkills, 'connections', collectPublicCommands(pi));
      notify(ctx, opened.ok ? `Octocode MCP settings opened${opened.url ? `: ${opened.url}` : ''}` : (opened.message ?? 'Could not open Octocode MCP settings.'), opened.ok ? 'info' : 'error');
    },
  });

  pi.registerCommand('octocode-setup', {
    description:
      'Install the Octocode APPEND_SYSTEM.md block into .pi or ~/.pi/agent. ' +
      'Only needed for plain-Pi sessions that do not load this extension — when the extension is active, the system prompt is injected at runtime (marker-guarded) and setup is optional.',
    getArgumentCompletions: (prefix: string) => {
      return ['project', 'global']
        .filter((s) => s.startsWith(prefix))
        .map((s) => ({
          value: s,
          label: s,
          description: s === 'project' ? 'Install in project .pi/' : 'Install in ~/.pi/agent/',
        }));
    },
    handler: async (args, ctx) => {
      await installAppendSystem(args, ctx);
    },
  });

  pi.registerCommand('octocode-skills-update', {
    description: 'Update this Pi package, then reload Pi resources.',
    handler: async (_args, ctx) => {
      if (!ctx?.hasUI) {
        notify(ctx, '/octocode-skills-update requires an interactive session to confirm. Run from the Pi UI.', 'error');
        return;
      }
      const source = getInstallSource();
      const cmdStr = `pi update ${source}`;
      const ok = await confirm(ctx, 'Update Octocode Pi package?', `Execute: ${cmdStr}`);
      if (!ok) {
        notify(ctx, 'Command cancelled.', 'info');
        return;
      }
      pi.sendUserMessage(cmdStr, { deliverAs: 'followUp' });
      if (ctx?.reload) await ctx.reload();
    },
  });

  // ─── Modern-TUI feature commands (palette / dial / watch / rewind / export) ──

  // Palette: no-arg commands are auto-discovered via pi.getCommands(); list here
  // only the arg-taking ones (they get an editor prefill instead of a dispatch).
  registerCommandPalette(pi, {
    commands: [
      { name: 'octocode-harness', description: 'Harness inventory: skills, MCPs, tools, prompt overhead' },
      { name: 'octocode-plan', description: 'Plan mode (new <goal>) / manage the active task plan', takesArgs: true },
      { name: 'octocode-agents', description: 'Inspect spawned worker agents', takesArgs: true },
      { name: 'octocode-cron', description: 'Manage Octocode session jobs', takesArgs: true },
      { name: 'mcp', description: 'Open the MCP connections manager' },
      { name: 'octocode-theme', description: 'Switch the Octocode theme', takesArgs: true },
      { name: 'octocode-chrome', description: 'List or close CDP connections', takesArgs: true },
      { name: 'octocode-dial', description: 'Set the effort dial level', takesArgs: true },
      { name: 'octocode-watch', description: 'Toggle AI! comment watch mode', takesArgs: true },
      { name: 'octocode-rewind', description: 'Restore a file checkpoint', takesArgs: true },
      { name: 'octocode-export', description: 'Brand a session HTML export', takesArgs: true },
    ],
  });
  registerDialCommand(pi);
  registerAiWatch(pi, {
    cwd: () => latestSessionCwd ?? process.cwd(),
    // Paint/clear the 'watch: on' chip on every startWatch/stopWatch transition.
    setStatus: (text) => setManagedStatus(latestSessionCtx, 'octocode-watch', text),
  });
  registerRewindCommand(pi, { getEngine: getCheckpointEngine, notify });
  registerExportCommand(pi);
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

export type {
  ProductionPiLifecycleCapture,
  ProductionPiScenarioInput,
  ProductionPiScenarioProbe,
  ProductionPiScenarioReceipt,
  ProductionPiScenarioSuite,
  ProductionPiScenarioId,
} from './adapters/pi-production-probe.js';

/** Exercise supported conformance scenarios through the installed Pi SDK composition. */
export function createProductionPiScenarioSuite(
  cwd: string,
): ProductionPiScenarioSuite {
  return createPiSdkScenarioSuite(
    cwd,
    createOctocodePiExtension({ hostVersion: APPROVED_PI_HOST_VERSION }),
  );
}

/**
 * Exercises the installed Pi SDK and this extension as one real composition.
 * Cross-host conformance imports this adapter instead of importing Pi directly.
 */
export async function captureProductionPiLifecycle(
  cwd: string,
): Promise<ProductionPiLifecycleCapture> {
  const extension = createOctocodePiExtension({
    hostVersion: APPROVED_PI_HOST_VERSION,
  });
  return capturePiSdkLifecycle(cwd, extension);
}

// Default export preserves the historical single-arg contract: Pi calls `default(pi)`.
export default createOctocodePiExtension();
