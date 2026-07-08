import fs from 'node:fs';
import path from 'node:path';
import {
  createPiAwarenessBridge,
  getPiAwarenessAgentId,
  resolveDbPath,
  wirePiAwarenessHooks,
} from '@octocodeai/octocode-awareness';
import { propagateOctocodeEnv, getOctocodeHome } from './env.js';
import {
  OCTOCODE_DIRECT_TOOL_NAMES,
  DISABLED_BUILTIN_TOOL_NAMES,
  OCTOCODE_SUPPORT_TOOL_NAMES,
} from './constants.js';
import { getAssetPaths, readTextIfExists, listBundledSkills, getInstallSource, getCLIPath } from './assets.js';

// Expose the bundled CLI path as an env var so agents can use: node $OCTOCODE_CLI <command>
// Set once at module load — inherited by all bash subprocesses spawned during the session.
process.env.OCTOCODE_CLI = getCLIPath();
import {
  shouldAppendSystemPrompt,
  mergeManagedAppendSystem,
  resolvePromptMode,
  composeSystemPrompt,
} from './prompt.js';
import {
  parseSetupScope,
  getAppendSystemTarget,
  splitArgs,
} from './utils.js';
import { registerOctocodeTools, registerUniqueTool } from './tools/octocode-tools.js';
import { buildMemoryToolDefinition, executeMemoryOperation } from './tools/memory.js';
import { registerContextTools } from './tools/context-tools.js';
import { cleanupSpawnedAgentsForShutdown, registerAgentTools } from './tools/agent-tools.js';
import { registerWebTool } from './tools/web-tool.js';
import { registerChromeDebugTool } from './tools/chrome-debug-tool.js';
import { registerBrowserAgentTool } from './tools/browser-agent-tool.js';
import { registerSpawnSubagentTool } from './tools/spawn-subagent-tool.js';
import { registerEditTool } from './tools/edit-tool.js';
import { pickProvider } from './web.js';
import type {
  PiInstance,
  PiContext,
  OctocodePiExtensionOptions,
  PromptMode,
} from './types.js';

// ─── Re-exports (stable public API) ──────────────────────────────────────────

export {
  OCTOCODE_DIRECT_TOOL_NAMES,
  DISABLED_BUILTIN_TOOL_NAMES,
  OCTOCODE_SUPPORT_TOOL_NAMES,
} from './constants.js';
export {
  PACKAGE_NAME,
  SYSTEM_PROMPT_MARKER,
  MANAGED_BLOCK_START,
  MANAGED_BLOCK_END,
} from './constants.js';
export { getAssetPaths, getOctocodeMemoryHome, readTextIfExists, listBundledSkills, getInstallSource, getCLIPath } from './assets.js';
export {
  shouldAppendSystemPrompt,
  renderSystemPromptAddendum,
  renderManagedAppendSystem,
  mergeManagedAppendSystem,
  resolvePromptMode,
  composeSystemPrompt,
} from './prompt.js';
export {
  splitArgs,
  parseSetupScope,
  getAppendSystemTarget,
  truncateUserVisibleToolOutput,
} from './utils.js';
export { extractPiWriteTargetPaths as extractWriteTargetPaths } from '@octocodeai/octocode-awareness';
export { runWebTool, renderWebResult, pickProvider } from './web.js';
export {
  cleanupSpawnedAgentsForShutdown,
  setAgentProcessFactoryForTests,
} from './tools/agent-tools.js';
export type { PromptMode, OctocodePiExtensionOptions, SkillInfo, BuildSystemPromptOptions } from './types.js';

export const getAwarenessAgentId = getPiAwarenessAgentId;

// ─── Awareness bridge ────────────────────────────────────────────────────────

export function createAwarenessBridge(
  options: Record<string, unknown> = {},
): ReturnType<typeof createPiAwarenessBridge> {
  return createPiAwarenessBridge(options);
}

export function createAwarenessHooksAddon(
  options: Parameters<typeof wirePiAwarenessHooks>[1] = {},
): (pi: PiInstance) => ReturnType<typeof wirePiAwarenessHooks> {
  return function octocodeAwarenessHooksAddon(pi: PiInstance): ReturnType<typeof wirePiAwarenessHooks> {
    return wirePiAwarenessHooks(pi as unknown as Parameters<typeof wirePiAwarenessHooks>[0], options);
  };
}

// ─── UI helpers ───────────────────────────────────────────────────────────────

export function getThinkingStatus(ctx: PiContext | undefined, level?: string): string {
  const model = ctx?.model;
  if (!model) return 'thinking: unknown model';
  if (!model.reasoning)
    return `thinking: off (${model.id ?? 'model'} has reasoning:false)`;
  return `thinking: ${level ?? 'default'} (${model.id ?? 'model'})`;
}

export function applyOctocodeUi(ctx: PiContext | undefined, level?: string): void {
  // setStatus / setHiddenThinkingLabel are TUI-only; guard with hasUI.
  if (!ctx?.hasUI) return;
  const ui = ctx?.ui;
  if (!ui) return;
  ui.setHiddenThinkingLabel?.('Octocode thinking');
  const label = ui.theme?.fg ? ui.theme.fg('accent', '◆ Octocode') : '◆ Octocode';
  ui.setStatus?.('octocode', label);
  const thinkingStatus = getThinkingStatus(ctx, level);
  ui.setStatus?.(
    'octocode-thinking',
    ui.theme?.fg ? ui.theme.fg('dim', thinkingStatus) : thinkingStatus,
  );
}

function notify(ctx: PiContext | undefined, message: string, level = 'info'): void {
  // Optional-chain defensively; pi's ui.notify is a no-op in non-UI modes.
  ctx?.ui?.notify?.(message, level);
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

function formatOctocodeToolStatus(): string {
  return `${OCTOCODE_DIRECT_TOOL_NAMES.length} native Pi tools`;
}

export function formatStatus(baseDir?: string): string {
  const paths = getAssetPaths(baseDir);
  const skills = listBundledSkills(baseDir);
  const promptStatus = fs.existsSync(paths.systemPrompt) ? 'found' : 'missing';

  const dbPath = resolveDbPath(null);
  const dbStatus = fs.existsSync(dbPath)
    ? `found (${dbPath})`
    : `not yet created (${dbPath})`;

  const searchProvider = pickProvider({});
  const searchKeys = ['TAVILY_API_KEY', 'TAVILY_API_TOKEN', 'SERPER_API_KEY'].filter(
    (k) => process.env[k],
  );
  const searchStatus = `${searchProvider}${searchKeys.length ? ` (keys: ${searchKeys.join(', ')})` : ' (no key — DuckDuckGo fallback)'}`;

  return [
    'Octocode Pi extension',
    `system prompt: ${promptStatus}`,
    `skills: ${skills.length}${skills.length > 0 ? ` (${skills.join(', ')})` : ''}`,
    `memory DB: ${dbStatus}`,
    `memory module: @octocodeai/octocode-awareness (direct import)`,
    `octocode tools: ${formatOctocodeToolStatus()}`,
    `bundled CLI: ${getCLIPath()} — use via: node $OCTOCODE_CLI <command>`,
    `disabled/replaced built-ins: edit (custom Octocode tool)${DISABLED_BUILTIN_TOOL_NAMES.length ? `; removed: ${DISABLED_BUILTIN_TOOL_NAMES.join(', ')}` : ''}`,
    `web search: ${searchStatus}`,
    `package assets: ${paths.baseDir}`,
    `flags: --no-context (suppress AGENTS.md/CLAUDE.md context files for this run)`,
  ].join('\n');
}

export interface ExtensionHarness {
  tools: string[];
  supportTools: string[];
  extensionCommands: string[];
  skills: string[];
  cliNote: string;
}

export function listExtensionHarness(baseDir?: string): ExtensionHarness {
  return {
    tools: [...OCTOCODE_DIRECT_TOOL_NAMES],
    supportTools: [...OCTOCODE_SUPPORT_TOOL_NAMES],
    extensionCommands: [
      '/octocode-status',
      '/octocode-harness',
      '/octocode-setup',
      '/octocode-skills-update',
      '/octocode-memory-digest',
      '/octocode-memory-forget',
    ],
    skills: listBundledSkills(baseDir),
    cliNote: `bundled CLI at ${getCLIPath()} — run via: node $OCTOCODE_CLI <command>`,
  };
}

function renderExtensionHarness(baseDir?: string): string {
  const harness = listExtensionHarness(baseDir);
  return [
    'Octocode Pi extension harness',
    `native tools (${harness.tools.length}): ${harness.tools.join(', ')}`,
    `support tools (${harness.supportTools.length}): ${harness.supportTools.join(', ')}`,
    `extension commands: ${harness.extensionCommands.join(', ')}`,
    `CLI: ${harness.cliNote}`,
    `skills (${harness.skills.length}): ${harness.skills.join(', ')}`,
  ].join('\n');
}

// ─── Built-in read tool disable ───────────────────────────────────────────────

export function disableBuiltinReadTool(pi: PiInstance): boolean {
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
    if (
      String((error as Error)?.message ?? error).includes(
        'Extension runtime not initialized',
      )
    ) {
      return false;
    }
    throw error;
  }
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
    `Write the managed Octocode harness block to ${targetPath}?`,
  );
  if (!ok) {
    notify(ctx, 'Octocode setup cancelled.', 'info');
    return;
  }
  const existing = readTextIfExists(targetPath);
  const nextContent = mergeManagedAppendSystem(existing, prompt);
  try {
    fs.mkdirSync(path.dirname(targetPath), { recursive: true });
    fs.writeFileSync(targetPath, nextContent, 'utf8');
    notify(ctx, `Octocode APPEND_SYSTEM.md installed at ${targetPath}`, 'info');
  } catch (error) {
    notify(
      ctx,
      `Failed to write ${targetPath}: ${(error as Error)?.message ?? String(error)}`,
      'error',
    );
  }
}

function parseMemoryCommandArgs(args: string): Record<string, unknown> {
  const tokens = splitArgs(args);
  const params: Record<string, unknown> = {};
  const tags: string[] = [];
  const memoryIds: string[] = [];
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i];
    const next = tokens[i + 1];
    switch (token) {
      case '--apply':
        params['apply'] = true;
        break;
      case '--yes':
        params['yes'] = true;
        break;
      case '--dry-run':
        params['dry_run'] = true;
        break;
      case '--export-doc':
        params['export_doc'] = true;
        break;
      case '--retention-days':
        if (next) params['retention_days'] = Number(tokens[++i]);
        break;
      case '--workspace':
        if (next) params['workspace_path'] = tokens[++i];
        break;
      case '--tag':
        if (next) tags.push(tokens[++i]);
        break;
      case '--id':
        if (next) memoryIds.push(tokens[++i]);
        break;
      case '--before':
        if (next) params['before'] = tokens[++i];
        break;
      case '--max-importance':
        if (next) params['max_importance'] = Number(tokens[++i]);
        break;
      default:
        break;
    }
  }
  if (tags.length) params['tags'] = tags;
  if (memoryIds.length) params['memory_ids'] = memoryIds;
  return params;
}

function memoryResultText(result: { content: Array<{ text: string }> }): string {
  return result.content[0]?.text ?? '{}';
}

async function runMemoryDigestCommand(args: string, ctx: PiContext | undefined): Promise<void> {
  const parsed = parseMemoryCommandArgs(args);
  const apply = parsed['apply'] === true;
  const params: Record<string, unknown> = {
    ...parsed,
    dry_run: !apply,
    workspace_path: (parsed['workspace_path'] as string | undefined) ?? ctx?.cwd ?? process.cwd(),
  };
  delete params['apply'];
  delete params['yes'];
  if (apply) {
    if (ctx?.hasUI) {
      const ok = await confirm(ctx, 'Run memory digest?', 'This archives/prunes memory store rows. Continue?');
      if (!ok) {
        notify(ctx, 'Memory digest cancelled.', 'info');
        return;
      }
    } else if (parsed['yes'] !== true) {
      notify(ctx, 'Pass --yes with --apply to run memory digest outside the UI.', 'error');
      return;
    }
  }
  const result = executeMemoryOperation('digest', params, (commandCtx) => getAwarenessAgentId(commandCtx), ctx);
  notify(ctx, `memory_digest ${apply ? 'applied' : 'preview'}: ${memoryResultText(result)}`, result.details && (result.details as { exit?: number }).exit ? 'error' : 'info');
}

async function runMemoryForgetCommand(args: string, ctx: PiContext | undefined): Promise<void> {
  const parsed = parseMemoryCommandArgs(args);
  const apply = parsed['apply'] === true;
  const hasFilter = Boolean(parsed['memory_ids'] || parsed['tags'] || parsed['before'] || parsed['max_importance']);
  if (!hasFilter) {
    notify(ctx, 'memory_forget requires --id, --tag, --before, or --max-importance.', 'error');
    return;
  }
  const params: Record<string, unknown> = { ...parsed, dry_run: !apply };
  delete params['apply'];
  delete params['yes'];
  if (apply) {
    if (ctx?.hasUI) {
      const ok = await confirm(ctx, 'Apply memory forget?', 'This permanently deletes matching memories. Continue?');
      if (!ok) {
        notify(ctx, 'Memory forget cancelled.', 'info');
        return;
      }
    } else if (parsed['yes'] !== true) {
      notify(ctx, 'Pass --yes with --apply to delete memories outside the UI.', 'error');
      return;
    }
  }
  const result = executeMemoryOperation('forget', params, (commandCtx) => getAwarenessAgentId(commandCtx), ctx);
  notify(ctx, `memory_forget ${apply ? 'applied' : 'preview'}: ${memoryResultText(result)}`, result.details && (result.details as { exit?: number }).exit ? 'error' : 'info');
}

function existingDirectory(filePath: string): string | null {
  return fs.existsSync(filePath) ? filePath : null;
}

// ─── Pi wiring ────────────────────────────────────────────────────────────────

async function wireOctocodePiExtension(
  pi: PiInstance,
  opts: { promptMode: PromptMode },
): Promise<void> {
  const { promptMode } = opts;

  // Register --no-context CLI flag before any session starts so Pi can parse it.
  // default:false → context files load normally (octocode-agent launcher already
  // passes --no-context-files at the pi CLI level for its own sessions).
  // Pass --no-context to suppress AGENTS.md / CLAUDE.md for any single run.
  pi.registerFlag?.('no-context', {
    description: 'Suppress AGENTS.md / CLAUDE.md context files from the system prompt',
    type: 'boolean',
    default: false,
  });

  // Best-effort early disable so the tool is absent immediately on load.
  // Real Pi runtimes also re-run this in session_start (which fires after
  // extension load) — the double call is idempotent.
  disableBuiltinReadTool(pi);

  if (pi.on) {
    createAwarenessHooksAddon()(pi);

    pi.on('resources_discover', async () => {
      const paths = getAssetPaths();
      const skillPath = existingDirectory(paths.skillsDir);
      return skillPath ? { skillPaths: [skillPath] } : {};
    });

    pi.on('session_start', async (_event, ctx) => {
      applyOctocodeUi(ctx, pi.getThinkingLevel?.());
      // Disable the built-in read tool (only once, here in session_start).
      try {
        if (disableBuiltinReadTool(pi)) {
          notify(
            ctx,
            'Octocode disabled Pi built-in read; use localGetFileContent instead.',
            'info',
          );
        }
      } catch (error) {
        notify(
          ctx,
          `Octocode could not disable Pi built-in read: ${(error as Error)?.message ?? String(error)}`,
          'warning',
        );
      }
      try {
        const trusted = ctx?.isProjectTrusted
          ? Boolean(await ctx.isProjectTrusted())
          : false;
        const { applied, skippedProtected } = propagateOctocodeEnv({
          home: getOctocodeHome(),
          cwd: ctx?.cwd ?? process.cwd(),
          trusted,
        });
        if (applied.length > 0) {
          notify(
            ctx,
            `Octocode env: loaded ${applied.length} var(s) (${applied.join(', ')}).`,
            'info',
          );
        }
        if (skippedProtected.length > 0) {
          notify(
            ctx,
            `Octocode env: skipped protected key(s): ${skippedProtected.join(', ')}.`,
            'warning',
          );
        }
      } catch (error) {
        notify(
          ctx,
          `Octocode env load failed: ${(error as Error)?.message ?? String(error)}`,
          'warning',
        );
      }
    });

    // Clean up status labels and spawned workers when the session tears down
    // so they don't leak across /new, /resume, /fork, reload, or quit.
    pi.on('session_shutdown', async (_event, ctx) => {
      const cleanedAgents = cleanupSpawnedAgentsForShutdown();
      if (ctx?.hasUI) {
        ctx.ui?.setStatus?.('octocode', '');
        ctx.ui?.setStatus?.('octocode-thinking', '');
        if (cleanedAgents > 0) {
          ctx.ui?.notify?.(`Octocode closed ${cleanedAgents} spawned subagent(s).`, 'info');
        }
      }
    });

    pi.on('model_select', async (_event, ctx) => {
      // thinking_level_select fires before model_select when the model change
      // clamps the thinking level, so pi.getThinkingLevel() is already updated.
      applyOctocodeUi(ctx, pi.getThinkingLevel?.());
    });

    pi.on('thinking_level_select', async (event, ctx) => {
      applyOctocodeUi(ctx, event.level);
    });

    pi.on('before_agent_start', async (event) => {
      // Suppress AGENTS.md / CLAUDE.md when --no-context flag is set.
      // For octocode-agent sessions the launcher already passes --no-context-files
      // to pi, so contextFiles is empty before this handler fires — this guard
      // is a belt-and-suspenders for direct pi usage.
      if (pi.getFlag?.('no-context') && event.systemPromptOptions?.contextFiles) {
        event.systemPromptOptions.contextFiles = [];
      }

      const prompt = readTextIfExists(getAssetPaths().systemPrompt);
      if (!shouldAppendSystemPrompt(event.systemPrompt, prompt)) {
        return;
      }
      if (prompt.trim().length === 0) return;
      return {
        systemPrompt: composeSystemPrompt({
          piSystemPrompt: event.systemPrompt,
          octocodePrompt: prompt,
          promptMode,
        }),
      };
    });
  }

  if (pi.registerTool) {
    const { Type } = await import('typebox');
    const registeredToolNames = new Set<string>();

    registerEditTool(pi, Type);

    await registerOctocodeTools(pi, Type, registeredToolNames);

    registerWebTool(pi, Type, registeredToolNames, registerUniqueTool);

    if (process.env['OCTOCODE_CHROME_DEBUG'] !== '0') {
      registerChromeDebugTool(pi, Type, registeredToolNames, registerUniqueTool, notify);
      registerBrowserAgentTool(pi, Type, registeredToolNames, registerUniqueTool, notify);
    }

    registerSpawnSubagentTool(pi, Type, registeredToolNames, registerUniqueTool, notify);

    registerContextTools(pi, Type, registeredToolNames, registerUniqueTool, notify);

    registerAgentTools(pi, Type, registeredToolNames, registerUniqueTool);

    buildMemoryToolDefinition(
      Type,
      (ctx) => getAwarenessAgentId(ctx),
      registerUniqueTool,
      pi,
      registeredToolNames,
      notify,
    );
  }

  if (!pi.registerCommand) return;

  pi.registerCommand('octocode-status', {
    description: 'Show Octocode Pi extension assets, tools, CLI, and bundled skills.',
    handler: async (_args, ctx) => {
      notify(ctx, formatStatus(), 'info');
    },
  });

  pi.registerCommand('octocode-harness', {
    description:
      'List every Octocode Pi extension harness surface: native tools, support tools, extension commands, CLI entry point, and skills.',
    handler: async (_args, ctx) => {
      notify(ctx, renderExtensionHarness(), 'info');
    },
  });

  pi.registerCommand('octocode-setup', {
    description: 'Install the Octocode APPEND_SYSTEM.md block into .pi or ~/.pi/agent.',
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

  pi.registerCommand('octocode-memory-digest', {
    description: 'Preview or apply memory store cleanup. Default is dry-run; pass --apply to mutate.',
    handler: async (args, ctx) => {
      await runMemoryDigestCommand(args, ctx);
    },
  });

  pi.registerCommand('octocode-memory-forget', {
    description: 'Preview or apply memory deletion by --id, --tag, --before, or --max-importance. Default is dry-run; pass --apply to mutate.',
    handler: async (args, ctx) => {
      await runMemoryForgetCommand(args, ctx);
    },
  });

  pi.registerCommand('octocode-skills-update', {
    description: 'Update this Pi package, then reload Pi resources.',
    handler: async (_args, ctx) => {
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
}

// ─── Public factory ───────────────────────────────────────────────────────────

/**
 * Factory: returns the `(pi) => {...}` wiring function Pi invokes as `default(pi)`.
 * `export default createOctocodePiExtension()` preserves the historical single-arg
 * default-export contract exactly; the octocode-agent launcher opts into octocode-first
 * mode (the 'replace' option value is accepted as a back-compat alias for it).
 */
export function createOctocodePiExtension(
  options: OctocodePiExtensionOptions = {},
): (pi: PiInstance) => Promise<void> {
  const promptMode = resolvePromptMode(options.promptMode);
  return async function octocodePiExtension(pi: PiInstance): Promise<void> {
    return wireOctocodePiExtension(pi, { promptMode });
  };
}

// Default export preserves the historical single-arg contract: Pi calls `default(pi)`.
export default createOctocodePiExtension();
