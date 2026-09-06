import type {PromptMode} from '@octocodeai/agent-contracts/protocols';
import fs from 'node:fs';
import path from 'node:path';
import {propagateOctocodeEnv, getOctocodeHome, isPersistentStorageEnabled} from '@octocodeai/config';
import {extensionWorkspaceRoot} from './extension-paths.js';
import {connectDb, defaultDbPath, insertEditLog} from '@octocodeai/octocode-awareness';
import {ensurePrivateDirectory, hardenPrivateFile, PRIVATE_FILE_MODE} from '@octocodeai/agent-contracts/permissions';
import {openPersistentAwareness} from './tools/storage-policy.js';
import {DISABLED_BUILTIN_TOOL_NAMES, OVERRIDDEN_BUILTIN_TOOL_NAMES, OCTOCODE_SUPPORT_TOOL_NAMES} from './constants.js';
import {checkForCoreUpdate, readOwnVersion} from './core-update-check.js';
import {ensureAdaptiveThinkingCompatibility} from './model-compat.js';
import {getAssetPaths, readTextIfExists, listBundledSkills, getInstallSource, getAwarenessCLIPath, resolveAwarenessCliPath, runAwarenessPreEdit, resolveAwarenessCoordinationScope} from './assets.js';

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
import {resolvePromptMode, composeSystemPrompt, stripProjectContext, stripPiSkillsSection} from './prompt.js';
import {registerSkillTool, discoverSkills, discoverSkillStates, type DiscoveredSkill} from './tools/skill-tool.js';
import {writeDiscoveryFile} from './tools/discovery-file.js';
import {estimateTokens, getAppendSystemTarget} from './utils.js';
import {getDirectToolContractStats, registerUniqueTool} from './tools/octocode-tools.js';
import {registerCompactionHooks, resetCompactionCheckpointDedupe, setCompactionRehydrationSegmentsProvider} from './tools/compaction-hooks.js';
import {registerCompactionPolicyGuidance} from './tools/compaction-policy-guidance.js';
import {budgetToolResult} from './tools/tool-result-budget.js';
import {cleanupSpawnedAgentsForShutdown, formatAgentLedger, listWorkerLedgerEntries, refreshAgentLedgerUi, setAgentLedgerMetricsRefreshForUi, isSubagentProcess, pruneDroppableAgentsForSession} from './tools/agent-tools.js';
import {registerWebTool} from './tools/web-tool.js';
import {registerChromeDebugTool} from './tools/chrome-debug-tool.js';
import {registerUnifiedAgentTool} from './tools/unified-agent-tool.js';
import {registerCallTool} from './tools/call-tool.js';
import {registerFileTool} from './tools/file-tool.js';
import {registerReadMediaTool} from './tools/read-media-tool.js';
import {registerRunFfmpegTool} from './tools/run-ffmpeg-tool.js';
import {registerMediaTool} from './tools/create-media-tool.js';
import {renderRuntimeCapabilitiesAddendum} from './tools/image-render.js';
import {setPeerWipBaseline, setPeerWipStatusPainter} from './tools/peer-wip.js';
import {registerBashTool} from './tools/bash-tool.js';
import {createAwarenessMutationGate} from './tools/awareness-mutation-gate.js';
import {INITIAL_CONTEXT_TOKEN_BUDGET, PROVIDER_CONTEXT_TOKEN_BUDGET, assembleContextSegments, assertContextTokenBudget} from './tools/context-segments.js';
import {clearCurrentContextSources, mergeCurrentContextSources, readLatestSessionUserRequest, readSessionPeerEvent, readSessionToolResult, registerCurrentContextSource, sessionPeerEventOrigin, sessionToolResultOrigin, sessionUserRequestOrigin} from './tools/context-source-registry.js';
import {applyStartupPermissionLevel, resetApprovalStore} from './tools/approval.js';
import {getCachedMcpCatalogAddendum, getCachedMcpCounts, isCompactMcpEnabled, mcpCatalogReady, registerMcpTool, startMcpConfigWatcher, stopAllMcpServers, stopMcpConfigWatchers, warmMcpCatalog} from './tools/mcp-tool.js';
import {openMcpManager, closeConfiguration} from './tools/mcp-html.js';
import {getDynamicCapabilitiesAddendum} from './tools/dynamic-catalog.js';
import {renderAvailableSkillsAddendum} from './tools/skill-catalog.js';
import {registerPlanTool} from './tools/plan-tool.js';
import {registerLocalServerTool} from './tools/local-server-tool.js';
import {registerAskUserTool} from './tools/ask-user-tool.js';
import {registerInteractionBrokerAdapter, type InteractionBrokerAdapterRegistry, type RegisteredInteractionBrokerAdapter} from './tools/interaction-broker-adapter.js';
import {brokerSessionId, clearInMemoryInteractionState, configureInteractionBrokerRoute} from './tools/interaction-broker.js';
import {registerMemoryTool} from './tools/memory-tool.js';
import {registerAwarenessCoordinationTools} from './tools/awareness-coordination-tools.js';
import {awarenessEventStatusText, registerAwarenessEventConsumer} from './tools/awareness-event-consumer.js';
import {getAwarenessAgentId} from './tools/awareness-shared.js';
import {activePlanScope, adoptPlanFromBranch, getPlan, getPlanReviewState, bumpPlanTurn, setPlanEntryAppender, PLAN_ENTRY_TYPE} from './tools/active-plan.js';
import {getCurrentPlanReadModel, renderPlanContext} from './tools/plan-read-model.js';
import {getCachedAwarenessStatus, refreshAwarenessPanel, suppressAwarenessPanel, resumeAwarenessPanel, clearAwarenessCacheEntry, setAwarenessMetricsRefreshForUi} from './tools/awareness-status.js';
import {deriveSessionName} from './ui-extras.js';
import {paintUi} from './tui/palette.js';
import {setUiTickSubscriber} from './tui/ui-ticker.js';
import {closeAllChromeConnections} from './chrome-connection-cache.js';
import {setPlanMetricsRefreshForUi} from './tools/plan-tool.js';
import {adoptPlanModePolicy, evaluateToolCapability, exitPlanMode, getPlanModePolicy} from './tools/plan-mode.js';
import {clearAllReadStates} from './tools/file-state.js';
import {registerAgentInbox, type AgentInboxRegistration} from './tools/agent-inbox.js';
import {collectPublicCommands} from './tools/commands-command.js';
import {runCleanupOnInit} from './tools/cleanup-command.js';
import {probeGitHubAuth} from './tools/github-auth-status.js';
import {registerOctocodeAutocomplete} from './tools/autocomplete-providers.js';
import {registerOctocodeMessageRenderers} from './tools/custom-messages.js';
import {initCheckpointStore, type CheckpointEngine} from './tools/checkpoints.js';
import {createSessionArtifactContext, type SessionArtifactContext} from './tools/session-artifacts.js';
import {initializeSessionMemory, projectSessionMemoryUpdate, readSessionMemory, renderSessionArtifactPaths, SESSION_MEMORY_MAX_BYTES} from './tools/session-memory.js';
import {initializeSessionIndexes} from './tools/session-index.js';
import {appendSessionAuditEntry, appendSessionAuditForContext, initializeSessionAudit} from './tools/session-audit.js';
import {cleanupEphemeralToolOutputs} from './tools/ephemeral-tool-output.js';
import {consumeValidatedRehydration, runAndRecordRehydration, REHYDRATION_RECEIPT_ENTRY_TYPE, type CurrentRehydrationSource} from './tools/rehydration-orchestrator.js';
import {createCheckpointInputHook} from './tools/rewind-command.js';
import {restoreDialOnStartup} from './tools/effort-dial.js';
import {runtimeStoreFor, setManagedActivity, setManagedStatus} from './tools/runtime-renderer.js';
import {SessionRuntime} from './session-runtime.js';
import {applyOctocodeUi, execGitSummary, formatContextUsage, getThinkingStatus, OCTOCODE_BANNER_ENTRY_TYPE, refreshFooterDirtyState, resetOctocodeFooterRegistration, updateOctocodeMetricsUi} from './extension-ui.js';
import {APPROVED_PI_HOST_VERSION, assertSupportedPiHostVersion, resolvePiHostVersion} from './adapters/pi-host-compatibility.js';
import {capturePiSdkLifecycle, createPiSdkScenarioSuite, type ProductionPiLifecycleCapture, type ProductionPiScenarioSuite} from './adapters/pi-production-probe.js';
import {createPiCanonicalRegistryComposition} from './adapters/pi-registry-adapters.js';
import {makeComponentRenderer} from './tools/render-helpers.js';
import {renderBannerWithTagline, type BannerSessionInfo, type BannerTheme} from './branding/banner.js';
import {pickProvider} from './web.js';
import {createHookComposer} from './hook-composer.js';
import {createOctocodeCronScheduler} from './scheduler.js';
import type {BeforeAgentStartEvent, PiInstance, PiContext, OctocodePiExtensionOptions, SessionShutdownEvent, ThinkingLevelEvent, SkillInfo, NotifyFn} from './types.js';

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
      return aw.startWork({
        filePath: target,
        agentId,
        reason: 'Automatic Pi mutation presence',
        testPlan: 'Inspect the resulting file and run applicable repository checks before marking this mutation verified',
      }).runId;
    } finally {
      aw.close();
    }
  },
  endWork: (target, workspace, agentId, runId) => {
    const aw = openPersistentAwareness({ workspace, scope: resolveAwarenessCoordinationScope(workspace) });
    try {
      aw.endWork({ filePath: target, agentId, runId });
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
  return path.join(extensionWorkspaceRoot(cwd), 'logs', 'error.txt');
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
    extensionCommands: ['/configuration'],
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
    '/configuration (local browser controls)',
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
  if (steps.length === 0) return ['local plan: none — call plan with action:"set" inside queries[] for multi-step work'];
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
    '/configuration (local browser controls)',
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
  registerCompactionPolicyGuidance(pi, notify);
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
    return makeComponentRenderer((_props, { width }) =>
      renderBannerWithTagline(theme as BannerTheme, width, readOwnVersion(getAssetPaths().baseDir), sessionInfo),
      undefined,
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
  let sessionArtifactContext: SessionArtifactContext | undefined;
  let sessionArtifactPathsContext = '';
  let deliveredSessionMemorySignature: string | undefined;
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

    hooks.on('tool_call', 'octocode-plan-mode-audit', async (event: { toolName?: string; input?: Record<string, unknown> }, ctx: PiContext | undefined) => {
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
      appendSessionAuditForContext(ctx, { event: 'session.shutdown', detail: { reason } });
      const canUseShutdownContext = reason === 'quit';
      awarenessMutationGate.cleanup();
      updateAwarenessRegistry('leave', pi, undefined, latestSessionCwd);
      cronScheduler.stop();
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
      if (ctx) clearCurrentContextSources(ctx);
      const cleanedAgents = cleanupSpawnedAgentsForShutdown();
      const stoppedMcpServers = stopAllMcpServers();
      const closedChrome = closeAllChromeConnections();
      if (closedChrome > 0 && canUseShutdownContext) notify(ctx, `Closed ${closedChrome} cached CDP connection(s).`, 'info');
      setPeerWipStatusPainter(undefined);
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
      frozenSystemPrompt = undefined;
      deliveredPlanSignature = undefined;
      sessionArtifactContext = undefined;
      sessionArtifactPathsContext = '';
      deliveredSessionMemorySignature = undefined;
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
      if (ctx) {
        try {
          const artifacts = createSessionArtifactContext(ctx);
          sessionArtifactContext = artifacts;
          initializeSessionIndexes(artifacts);
          const memoryPath = initializeSessionMemory(artifacts);
          const auditPath = initializeSessionAudit(artifacts);
          sessionArtifactPathsContext = renderSessionArtifactPaths({ memoryPath, auditPath });
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
            readCurrent: () => readSessionMemory(artifacts),
          });
          appendSessionAuditEntry(artifacts, {
            event: 'session.start',
            detail: { reason: reason ?? 'new' },
          });
        } catch {
          // Session artifacts are continuity aids; initialization must not block Pi startup.
        }
      }
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

    // Auto-snapshot the working tree (shadow git) before each real user prompt
    // so /octocode-rewind can restore files. Fire-and-forget inside the hook —
    // it never blocks input. Return no result so the snapshot does not alter
    // the input middleware's response.
    const checkpointInputHook = createCheckpointInputHook({ getEngine: getCheckpointEngine });
    hooks.on('input', 'octocode-checkpoint-snapshot', async (event: { text: string; source?: string; streamingBehavior?: string }, ctx: PiContext | undefined) => {
      await checkpointInputHook(event, ctx);
      return undefined;
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
      const currentSessionMemory = sessionArtifactContext
        ? readSessionMemory(sessionArtifactContext) ?? ''
        : '';
      const sessionMemoryUpdate = projectSessionMemoryUpdate(
        currentSessionMemory,
        deliveredSessionMemorySignature,
      );
      const sessionMemoryContent = sessionMemoryUpdate.content;

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
          'session-artifact-contract': sessionArtifactPathsContext,
        };
        const currentAssembly = assembleContextSegments([
          { id: 'octocode-product-policy', content: currentContents['octocode-product-policy']!, kind: 'product-policy', origin: 'octocode-harness', authority: 'product', scope: 'session', visibility: 'hidden-policy', rehydrate: 'always', tokenBudget: 20_000 },
          { id: 'mcp-tool-contracts', content: currentMcpCatalog, kind: 'tool-contract', origin: 'octocode-harness', authority: 'product', scope: 'session', visibility: 'inspectable', rehydrate: 'always', tokenBudget: 30_000 },
          { id: 'runtime-tool-contracts', content: currentRuntimeCapabilities, kind: 'tool-contract', origin: 'octocode-harness', authority: 'product', scope: 'session', visibility: 'inspectable', rehydrate: 'always', tokenBudget: 10_000 },
          { id: 'dynamic-tool-contracts', content: currentDynamic, kind: 'tool-contract', origin: 'octocode-harness', authority: 'product', scope: 'session', visibility: 'inspectable', rehydrate: 'always', tokenBudget: 20_000 },
          { id: 'available-skills', content: currentSkills, kind: 'skill', origin: 'installed-skills', authority: 'project', scope: 'session', visibility: 'inspectable', rehydrate: 'on-trigger', tokenBudget: 20_000 },
          { id: 'session-artifact-contract', content: sessionArtifactPathsContext, kind: 'tool-contract', origin: 'octocode-harness', authority: 'product', scope: 'session', visibility: 'inspectable', rehydrate: 'always', tokenBudget: 1_000 },
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
        { id: 'session-memory', content: sessionMemoryContent, kind: 'memory-lead', origin: 'session-memory', authority: 'external-data', scope: 'session', visibility: 'inspectable', rehydrate: 'always', tokenBudget: Math.ceil(SESSION_MEMORY_MAX_BYTES / 4) },
      ]);
      const contextMessage =
        contextAssembly.manifest.length > 0 || frozenRehydration?.content
          ? { customType: 'octocode-context-update', content: [contextAssembly.content, frozenRehydration?.content].filter(Boolean).join('\n\n'), display: false, details: { version: 1, segments: [...contextAssembly.manifest, ...(frozenRehydration?.segments ?? [])], ...(frozenRehydration ? { rehydration: frozenRehydration.receipt } : {}) } }
          : undefined;

      if (frozenSystemPrompt !== undefined) {
        deliveredPlanSignature = planSig;
        deliveredSessionMemorySignature = sessionMemoryUpdate.signature;
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
        { id: 'session-artifact-contract', content: sessionArtifactPathsContext, kind: 'tool-contract', origin: 'octocode-harness', authority: 'product', scope: 'session', visibility: 'inspectable', rehydrate: 'always', tokenBudget: 1_000 },
      ], { totalTokenBudget: INITIAL_CONTEXT_TOKEN_BUDGET });
      const initialContents: Record<string, string> = {
        'octocode-product-policy': cachedSystemPromptText,
        'mcp-tool-contracts': mcpCatalog,
        'runtime-tool-contracts': runtimeCapabilities,
        'dynamic-tool-contracts': dynamicCatalog,
        'available-skills': availableSkills,
        'session-artifact-contract': sessionArtifactPathsContext,
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
      deliveredSessionMemorySignature = sessionMemoryUpdate.signature;
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

  pi.registerCommand?.('configuration', {
    description: 'Open Octocode configuration in your local browser.',
    handler: async (_args, ctx) => {
      try {
        const opened = await openMcpManager(ctx, latestPiSkills, 'overview', collectPublicCommands(pi), pi);
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
