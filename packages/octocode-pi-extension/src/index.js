import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFile } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { runWebTool, renderWebResult, pickProvider } from './web.js';
import { propagateOctocodeEnv, getOctocodeHome } from './env.js';
import {
  connectDb, resolveDbPath, memoryHome as resolveMemoryHome,
  getMemory, insertMemory, reflect as reflectMemory,
  preFlightIntent, releaseFileLock,
} from '@octocodeai/octocode-memory';

export const PACKAGE_NAME = '@octocodeai/pi-extension';
export const SYSTEM_PROMPT_MARKER = '<!-- octocode-pi-extension:system-prompt -->';
export const MANAGED_BLOCK_START = '<!-- OCTOCODE_PI_EXTENSION_APPEND_SYSTEM_START -->';
export const MANAGED_BLOCK_END = '<!-- OCTOCODE_PI_EXTENSION_APPEND_SYSTEM_END -->';

const extensionDir = path.dirname(fileURLToPath(import.meta.url));

export function getAssetPaths(baseDir = extensionDir) {
  return {
    baseDir,
    docsDir: path.join(baseDir, 'docs'),
    skillsDir: path.join(baseDir, 'skills'),
    systemPrompt: path.join(baseDir, 'system', 'APPEND_SYSTEM.md'),
  };
}

/**
 * Awareness memory home: delegates to @octocodeai/octocode-memory.
 * Kept as a named export for backward compat with external callers.
 */
export function getOctocodeMemoryHome() {
  return resolveMemoryHome();
}

export function readTextIfExists(filePath) {
  try {
    return fs.readFileSync(filePath, 'utf8');
  } catch (error) {
    if (error && error.code === 'ENOENT') {
      return '';
    }
    throw error;
  }
}

export function listBundledSkills(baseDir = extensionDir) {
  const { skillsDir } = getAssetPaths(baseDir);
  if (!fs.existsSync(skillsDir)) {
    return [];
  }

  return fs
    .readdirSync(skillsDir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .filter((skillName) => fs.existsSync(path.join(skillsDir, skillName, 'SKILL.md')))
    .sort();
}

export function shouldAppendSystemPrompt(systemPrompt, octocodePrompt) {
  const trimmedPrompt = octocodePrompt.trim();
  if (trimmedPrompt.length === 0) {
    return false;
  }

  if (systemPrompt.includes(SYSTEM_PROMPT_MARKER)) {
    return false;
  }

  const proofSlice = trimmedPrompt.slice(0, Math.min(160, trimmedPrompt.length));
  return !systemPrompt.includes(proofSlice);
}

export function renderSystemPromptAddendum(octocodePrompt) {
  return `${SYSTEM_PROMPT_MARKER}\n${octocodePrompt.trim()}\n${SYSTEM_PROMPT_MARKER}`;
}

export function renderManagedAppendSystem(octocodePrompt) {
  return `${MANAGED_BLOCK_START}\n${octocodePrompt.trim()}\n${MANAGED_BLOCK_END}\n`;
}

export function mergeManagedAppendSystem(existingContent, octocodePrompt) {
  const block = renderManagedAppendSystem(octocodePrompt);
  const startIndex = existingContent.indexOf(MANAGED_BLOCK_START);
  const endIndex = existingContent.indexOf(MANAGED_BLOCK_END);

  if (startIndex !== -1 && endIndex !== -1 && endIndex > startIndex) {
    const afterEnd = endIndex + MANAGED_BLOCK_END.length;
    return `${existingContent.slice(0, startIndex)}${block}${existingContent.slice(afterEnd).replace(/^\n+/, '')}`;
  }

  const prefix = existingContent.trimEnd();
  return prefix.length > 0 ? `${prefix}\n\n${block}` : block;
}

export function splitArgs(input) {
  const args = [];
  const pattern = /"([^"\\]*(?:\\.[^"\\]*)*)"|'([^'\\]*(?:\\.[^'\\]*)*)'|[^\s]+/g;
  let match;

  while ((match = pattern.exec(input)) !== null) {
    const value = match[1] ?? match[2] ?? match[0];
    args.push(value.replace(/\\(["'\\])/g, '$1'));
  }

  return args;
}

export function parseSetupScope(args) {
  const tokens = splitArgs(args);
  if (tokens.includes('--global') || tokens.includes('global')) {
    return 'global';
  }
  return 'project';
}

export function getAppendSystemTarget(scope, cwd = process.cwd(), homeDir = os.homedir()) {
  if (scope === 'global') {
    return path.join(homeDir, '.pi', 'agent', 'APPEND_SYSTEM.md');
  }
  return path.join(cwd, '.pi', 'APPEND_SYSTEM.md');
}

export function getInstallSource(baseDir = extensionDir) {
  const packageRoot = path.dirname(baseDir);
  // npm installs land inside node_modules/@octocodeai/pi-extension
  if (packageRoot.includes(path.join('node_modules', '@octocodeai', 'pi-extension'))) {
    return 'npm:@octocodeai/pi-extension';
  }
  return packageRoot;
}

export function getBundledOctocodeScript() {
  // Preferred: physically bundled into dist/bin/ during build (always present when published)
  const distBin = path.join(extensionDir, 'bin', 'octocode.js');
  if (fs.existsSync(distBin)) return distBin;

  // Fallback: resolve from node_modules (development / non-standard installs)
  try {
    const require = createRequire(import.meta.url);
    const pkgJsonPath = require.resolve('octocode/package.json');
    const pkg = JSON.parse(fs.readFileSync(pkgJsonPath, 'utf8'));
    const binEntry = typeof pkg.bin === 'string' ? pkg.bin : pkg.bin?.octocode;
    if (!binEntry) return null;
    const scriptPath = path.resolve(path.dirname(pkgJsonPath), binEntry);
    return fs.existsSync(scriptPath) ? scriptPath : null;
  } catch {
    return null;
  }
}

export function getBundledOctocodeVersion() {
  try {
    const require = createRequire(import.meta.url);
    const pkgJsonPath = require.resolve('octocode/package.json');
    const pkg = JSON.parse(fs.readFileSync(pkgJsonPath, 'utf8'));
    return pkg.version ?? null;
  } catch {
    return null;
  }
}

export function runOctocode(args, options = {}) {
  const script = getBundledOctocodeScript();
  if (script) {
    return defaultRunCommand(process.execPath, [script, ...args], options);
  }
  return defaultRunCommand('npx', ['octocode', ...args], options);
}

export function formatStatus(baseDir = extensionDir) {
  const paths = getAssetPaths(baseDir);
  const skills = listBundledSkills(baseDir);
  const promptStatus = fs.existsSync(paths.systemPrompt) ? 'found' : 'missing';
  const octocodeScript = getBundledOctocodeScript();
  const octocodeVersion = getBundledOctocodeVersion();
  const octocodeStatus = octocodeScript
    ? `bundled v${octocodeVersion ?? '?'} → ${octocodeScript}`
    : 'not bundled (fallback: npx octocode)';

  const memoryHome = getOctocodeMemoryHome();
  const dbPath = path.join(memoryHome, 'awareness.sqlite3');
  const dbStatus = fs.existsSync(dbPath) ? `found (${dbPath})` : `not yet created (${dbPath})`;

  // Web search provider + key presence (names only — never values).
  const searchProvider = pickProvider({});
  const searchKeys = ['TAVILY_API_KEY', 'TAVILY_API_TOKEN', 'SERPER_API_KEY'].filter((k) => process.env[k]);
  const searchStatus = `${searchProvider}${searchKeys.length ? ` (keys: ${searchKeys.join(', ')})` : ' (no key — DuckDuckGo fallback)'}`;

  return [
    'Octocode Pi extension',
    `system prompt: ${promptStatus}`,
    `skills: ${skills.length}${skills.length > 0 ? ` (${skills.join(', ')})` : ''}`,
    `memory DB: ${dbStatus}`,
    `memory module: @octocodeai/octocode-memory (direct import)`,
    `octocode CLI: ${octocodeStatus}`,
    `web search: ${searchStatus}`,
    `package assets: ${baseDir}`,
  ].join('\n');
}

function addPathValue(paths, value) {
  if (typeof value === 'string' && value.trim().length > 0) {
    paths.push(value.trim());
  } else if (Array.isArray(value)) {
    for (const item of value) {
      addPathValue(paths, item);
    }
  }
}

export function extractWriteTargetPaths(toolName, input = {}) {
  if (toolName !== 'write' && toolName !== 'edit') {
    return [];
  }

  const paths = [];
  addPathValue(paths, input.path);
  addPathValue(paths, input.filePath);
  addPathValue(paths, input.file_path);
  addPathValue(paths, input.paths);
  addPathValue(paths, input.filePaths);
  addPathValue(paths, input.file_paths);

  return [...new Set(paths)];
}

export function getAwarenessAgentId(ctx) {
  if (process.env.OCTOCODE_AGENT_ID) {
    return process.env.OCTOCODE_AGENT_ID;
  }

  const sessionFile = ctx?.sessionManager?.getSessionFile?.();
  if (sessionFile) {
    return `pi:${path.basename(sessionFile, path.extname(sessionFile))}`;
  }

  return `pi:${process.pid}`;
}

function defaultRunCommand(command, args, options = {}) {
  return new Promise((resolve) => {
    execFile(command, args, options, (error, stdout, stderr) => {
      resolve({
        error,
        status: error ? (Number.isInteger(error.code) ? error.code : 1) : 0,
        stderr: stderr ?? '',
        stdout: stdout ?? '',
      });
    });
  });
}

export function createAwarenessBridge(options = {}) {
  const pendingToolFiles = options.pendingToolFiles ?? new Map();

  return {
    pendingToolFiles,

    async handleToolCall(event, ctx) {
      const targetFiles = extractWriteTargetPaths(event?.toolName, event?.input);
      if (targetFiles.length === 0) return undefined;

      const agentId = getAwarenessAgentId(ctx);
      try {
        const db = connectDb(ctx?.dbPath ?? resolveDbPath(null));
        const result = preFlightIntent(db, {
          agentId,
          workspacePath: ctx?.cwd ?? process.cwd(),
          rationale: 'auto: Pi write/edit tool call via octocode-pi-extension',
          testPlan: 'post-edit verification',
          targetFiles,
          ttlMs: 15 * 60000,
        });

        if (!result.ok) {
          const detail = (result.conflicts || [])
            .map(c => `${c.file_path} (held by ${c.agent_id})`).join(', ');
          return { block: true, reason: `Octocode awareness blocked this edit: ${detail || 'conflict'}` };
        }

        if (event?.toolCallId) {
          pendingToolFiles.set(event.toolCallId, targetFiles);
        }
        return undefined;
      } catch (err) {
        notify(ctx, `Octocode awareness warning; continuing: ${err.message}`, 'warning');
        return undefined;
      }
    },

    async handleToolResult(event, ctx) {
      const targetFiles = pendingToolFiles.get(event?.toolCallId);
      if (!targetFiles) return undefined;

      pendingToolFiles.delete(event.toolCallId);
      try {
        const db = connectDb(ctx?.dbPath ?? resolveDbPath(null));
        releaseFileLock(db, {
          agentId: getAwarenessAgentId(ctx),
          targetFiles,
          status: 'PENDING',
        });
      } catch (err) {
        notify(ctx, `Octocode awareness warning; continuing: ${err.message}`, 'warning');
      }
      return undefined;
    },
  };
}

function notify(ctx, message, level = 'info') {
  if (ctx?.ui?.notify) {
    ctx.ui.notify(message, level);
  }
}

async function confirm(ctx, title, message) {
  if (!ctx?.ui?.confirm) {
    return false;
  }
  return Boolean(await ctx.ui.confirm(title, message));
}

async function installAppendSystem(args, ctx) {
  const paths = getAssetPaths();
  const prompt = readTextIfExists(paths.systemPrompt);
  if (prompt.trim().length === 0) {
    notify(ctx, `Missing Octocode system prompt at ${paths.systemPrompt}`, 'error');
    return;
  }

  const scope = parseSetupScope(args);
  const targetPath = getAppendSystemTarget(scope, ctx?.cwd ?? process.cwd());
  const ok = await confirm(
    ctx,
    'Install Octocode APPEND_SYSTEM.md?',
    `Write the managed Octocode harness block to ${targetPath}?`
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
    notify(ctx, `Failed to write ${targetPath}: ${error?.message ?? String(error)}`, 'error');
  }
}

function existingDirectory(filePath) {
  return fs.existsSync(filePath) ? filePath : null;
}

/**
 * Resolve the harness prompt mode.
 * Precedence: explicit option > OCTOCODE_PROMPT_MODE env > 'append'.
 * The octocode-agent launcher sets OCTOCODE_PROMPT_MODE=replace so a
 * CLI-loaded extension can switch modes without a code change, while the
 * bare `pi install` default export stays byte-for-byte append-mode.
 */
export function resolvePromptMode(option) {
  if (option === 'replace' || option === 'append') return option;
  return process.env.OCTOCODE_PROMPT_MODE === 'replace' ? 'replace' : 'append';
}

/**
 * Build the system prompt the extension hands back to Pi.
 * - append (default): Pi's prompt, then the Octocode harness addendum (unchanged legacy behavior).
 * - replace: the Octocode harness leads as authoritative, with Pi's own prompt preserved
 *   below it so nothing Pi injected (appendSystemPrompt, project context) is lost.
 *   NOTE: faithful reconstruction of Pi's customPrompt branch (composeSystemPrompt) is
 *   RFC Phase 2; until then replace-mode prepends rather than rebuilds.
 */
export function composeSystemPrompt({ piSystemPrompt, octocodePrompt, promptMode, bundledNote = '' }) {
  const addendum = renderSystemPromptAddendum(octocodePrompt);
  if (promptMode === 'replace') {
    return `${addendum}${bundledNote}\n\n${piSystemPrompt}`;
  }
  return `${piSystemPrompt}\n\n${addendum}${bundledNote}`;
}

/**
 * Factory: returns the `(pi) => {...}` wiring function Pi invokes as `default(pi)`.
 * `export default createOctocodePiExtension()` preserves the historical single-arg
 * default-export contract exactly; the octocode-agent launcher opts into replace mode.
 */
export function createOctocodePiExtension(options = {}) {
  const promptMode = resolvePromptMode(options.promptMode);
  return async function octocodePiExtension(pi) {
    return wireOctocodePiExtension(pi, { promptMode });
  };
}

async function wireOctocodePiExtension(pi, { promptMode }) {
  // Handoff queue (FIFO) set by handoff_context tool, drained by octocode-handoff command.
  // A queue — not one slot — so multiple delegations in a turn queue and run as follow-ups
  // instead of silently dropping (the prior single-slot overwrite footgun).
  const handoffQueue = [];

  if (pi?.on) {
    const awarenessBridge = createAwarenessBridge();

    pi.on('resources_discover', async () => {
      const paths = getAssetPaths();
      const skillPath = existingDirectory(paths.skillsDir);
      return skillPath ? { skillPaths: [skillPath] } : {};
    });

    // Propagate octocode `.env` into process.env so the web tool (TAVILY/SERPER keys),
    // bash, hooks, and skill scripts see user-supplied env. Global (~/.octocode/.env) is
    // always loaded; the project file (<cwd>/.octocode/.env) only when the project is
    // trusted. Protected keys are never overwritten; values are never logged.
    pi.on('session_start', async (_event, ctx) => {
      try {
        const trusted = ctx?.isProjectTrusted ? Boolean(await ctx.isProjectTrusted()) : false;
        const { applied, skippedProtected } = propagateOctocodeEnv({
          home: getOctocodeHome(),
          cwd: ctx?.cwd ?? process.cwd(),
          trusted,
        });
        if (applied.length > 0) {
          notify(ctx, `Octocode env: loaded ${applied.length} var(s) (${applied.join(', ')}).`, 'info');
        }
        if (skippedProtected.length > 0) {
          notify(ctx, `Octocode env: skipped protected key(s): ${skippedProtected.join(', ')}.`, 'warning');
        }
      } catch (error) {
        notify(ctx, `Octocode env load failed: ${error?.message ?? String(error)}`, 'warning');
      }
    });

    pi.on('before_agent_start', async (event) => {
      const prompt = readTextIfExists(getAssetPaths().systemPrompt);
      // In append mode, skip if the harness prompt is already present.
      // In replace mode we always (re)assert the harness as the leading authority.
      if (promptMode !== 'replace' && !shouldAppendSystemPrompt(event.systemPrompt, prompt)) {
        return;
      }
      if (prompt.trim().length === 0) {
        return;
      }

      const script = getBundledOctocodeScript();
      const version = script ? getBundledOctocodeVersion() : null;
      const bundledNote = script
        ? `\n\n<!-- octocode-pi-extension:bundled-cli -->\nBundled Octocode CLI${version ? ` v${version}` : ''} — use \`node ${script}\` instead of \`npx octocode\`.\n<!-- octocode-pi-extension:bundled-cli -->`
        : '';

      return {
        systemPrompt: composeSystemPrompt({
          piSystemPrompt: event.systemPrompt,
          octocodePrompt: prompt,
          promptMode,
          bundledNote,
        }),
      };
    });

    pi.on('tool_call', async (event, ctx) => awarenessBridge.handleToolCall(event, ctx));
    pi.on('tool_result', async (event, ctx) => awarenessBridge.handleToolResult(event, ctx));
  }

  if (pi?.registerTool) {
    // typebox lives in pi's runtime node_modules; dynamic import avoids a hard
    // static-load dep that would break the test/build environment.
    const { Type } = await import('typebox');

    // ─── Web Tool ─────────────────────────────────────────────────────────────
    // One tool so the agent can go to the web and see like an agent: pass `url` to
    // read a page as clean text, or `query` to search. No API key. SSRF-hardened
    // (see src/web.js): private/loopback/link-local/metadata IPs blocked, redirects
    // re-validated per hop, size + time caps. Failures return text, never throw.
    pi.registerTool({
      name: 'web',
      label: 'Web',
      description:
        'Browse the live web. Pass `url` to fetch and read a page as clean text (like visiting it), ' +
        'or `query` to run a web search and get ranked {title, url, snippet} results (plus an AI answer when available). ' +
        'Search uses the best configured provider (Tavily → Serper → DuckDuckGo); set a key in ~/.octocode/.env to upgrade. ' +
        'Use for docs, changelogs, error messages, and current info beyond the codebase and training data. ' +
        'One of `url` or `query` is required.',
      promptSnippet: 'Search the web or fetch and read a page',
      promptGuidelines: [
        'Prefer Octocode/local tools for code and packages; use web for external docs, news, and live info. ' +
        'Search with `query` to discover, then read the best hit with `url`.',
      ],
      parameters: Type.Object({
        url: Type.Optional(Type.String({ description: 'Absolute http(s) URL to fetch and read as text.' })),
        query: Type.Optional(Type.String({ description: 'Web search query (used when no url is given).' })),
        maxResults: Type.Optional(Type.Integer({ minimum: 1, maximum: 20, description: 'Search: max results (default 5).' })),
        maxChars: Type.Optional(Type.Integer({ minimum: 500, maximum: 50000, description: 'Fetch: max characters of page text to return per page (default 15000).' })),
        page: Type.Optional(Type.Integer({ minimum: 1, maximum: 20, description: 'Fetch: page number for long documents (default 1). Each page is maxChars chars. Pass page: 2, 3… when the result shows truncated: true.' })),
        engine: Type.Optional(Type.String({ description: 'Search: force a provider — "tavily", "serper", or "duckduckgo" (default: auto by available key).' })),
        timeRange: Type.Optional(Type.String({ description: 'Search: recency filter — "day", "week", "month", or "year".' })),
        includeDomains: Type.Optional(Type.Array(Type.String(), { description: 'Search (Tavily): allowlist domains, e.g. ["docs.python.org"].' })),
        excludeDomains: Type.Optional(Type.Array(Type.String(), { description: 'Search (Tavily): blocklist domains to drop noise.' })),
      }),
      async execute(_toolCallId, params, signal) {
        const out = await runWebTool(params, { signal });
        return {
          content: [{ type: 'text', text: renderWebResult(out) }],
          isError: Boolean(out.error),
          details: out,
        };
      },
    });

    pi.registerTool({
      name: 'compact_context',
      label: 'Compact Context',
      description:
        'Compact conversation history to free context window space. Call autonomously when context is ≥ 60 % full AND the next task is large, at a research→execution boundary, or when an unrelated task starts mid-session.',
      promptSnippet: 'Compact conversation history to free context window space',
      parameters: Type.Object({
        instructions: Type.Optional(
          Type.String({ description: 'Optional focus instructions for the compaction summary (e.g. "focus on recent file changes").' })
        ),
      }),
      async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
        ctx.compact({
          customInstructions: params.instructions,
          onComplete: () => {},
          onError: () => {},
        });
        return {
          content: [{ type: 'text', text: 'Compaction triggered. The context window will be summarized shortly.' }],
        };
      },
    });

    pi.registerTool({
      name: 'clear_context',
      label: 'Clear Context',
      description:
        'Start a new session with no prior context. Call autonomously when the next task is unrelated to the current conversation.',
      promptSnippet: 'Start a new session with no prior context',
      parameters: Type.Object({}),
      async execute(_toolCallId, _params, _signal, _onUpdate, _ctx) {
        pi.sendUserMessage('/new', { deliverAs: 'followUp' });
        return {
          content: [{ type: 'text', text: 'New session queued. The context will be cleared before the next turn.' }],
        };
      },
    });

    pi.registerTool({
      name: 'handoff_context',
      label: 'Handoff to Subagent',
      description:
        'Compact the current context and open a new session seeded with a targeted summary. Call autonomously when delegating to a fresh subagent or starting a long follow-up task with only essential context.',
      promptSnippet: 'Open a new session seeded with a targeted handoff summary',
      parameters: Type.Object({
        summary: Type.String({
          description:
            'Self-contained handoff summary: goal, constraints, progress, key decisions, next steps, and critical context (file paths, values). No noise. Must be readable by an agent with NONE of this thread\'s context.',
        }),
        kickoff: Type.Optional(
          Type.String({
            description: 'First message the subagent should act on. Defaults to "Continue from the context above."',
          })
        ),
        artifactDir: Type.Optional(Type.String({
          description: 'Workspace .octocode/<kind>/<YYYYMMDD-HHMM-slug>/ dir the subagent must write its deliverable to so the caller can verify on disk. Omit only for tasks with no artifact.',
        })),
      }),
      async execute(_toolCallId, params, _signal, _onUpdate, _ctx) {
        const kickoff = params.kickoff || 'Continue from the context above.';
        const summary = params.artifactDir
          ? `${params.summary}\n\nDeliverable path (write your result here so the caller verifies on disk): ${params.artifactDir}`
          : params.summary;
        handoffQueue.push({ summary, kickoff });
        pi.sendUserMessage('/octocode-handoff', { deliverAs: 'followUp' });
        const position = handoffQueue.length;
        return {
          content: [
            {
              type: 'text',
              text:
                position === 1
                  ? 'Handoff queued — a new session will open seeded with your summary.'
                  : `Handoff queued at position ${position} (runs after ${position - 1} pending handoff${position - 1 === 1 ? '' : 's'}). A session opens for each in order.`,
            },
          ],
        };
      },
    });

    // ─── Memory Tools ─────────────────────────────────────────────────────────
    // Direct calls into @octocodeai/octocode-memory — no subprocess overhead.

    const MEMORY_LABELS = 'BUG|FEATURE|SUGGESTION|GOTCHA|IMPROVEMENT|DECISION|ARCHITECTURE|SECURITY|PERFORMANCE|TEST|BUILD|DOCS|CONFIG|WORKFLOW|REFACTOR|API|RELEASE|INCIDENT|OTHER';
    // Smart-input default importance by label when the agent omits `importance`.
    // Critical/safety labels rank high; routine labels fall to the middling default.
    const DEFAULT_IMPORTANCE = { BUG: 8, GOTCHA: 7, IMPROVEMENT: 7, SECURITY: 9, INCIDENT: 9, RELEASE: 8, DECISION: 6, ARCHITECTURE: 6 };
    const defaultImportance = (label) => DEFAULT_IMPORTANCE[label?.toUpperCase()] ?? 5;

    pi.registerTool({
      name: 'memory_recall',
      label: 'Memory Recall',
      description: 'Query the shared agent memory store for prior lessons, decisions, and gotchas. ' +
        'Returns importance-ranked results. Memories are leads — validate code facts against current files. ' +
        'Zero results ≠ empty store: retry with smart=true.',
      promptSnippet: 'Query shared memory for prior lessons and decisions',
      promptGuidelines: [
        'Call memory_recall before non-trivial work. Use smart=true on zero results before concluding nothing is known.',
      ],
      parameters: Type.Object({
        query: Type.String({ description: 'What you are about to work on — natural language.' }),
        limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 20, description: 'Max results (default 3).' })),
        min_importance: Type.Optional(Type.Integer({ minimum: 1, maximum: 10, description: 'Floor importance (1–10). Raise to filter out low-signal noise.' })),
        smart: Type.Optional(Type.Boolean({ description: 'Broaden: lower threshold, drop filters, try semantic index.' })),
        label: Type.Optional(Type.String({ description: `Filter by label: ${MEMORY_LABELS}` })),
      }),
      async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
        try {
          const db = connectDb(ctx?.dbPath ?? resolveDbPath(null));
          const result = getMemory(db, {
            query: params.query,
            limit: params.limit ?? 3,
            minImportance: params.min_importance,
            label: params.label ? [params.label] : undefined,
            smart: params.smart,
            workspacePath: ctx?.cwd ?? process.cwd(),
          });
          // Token-efficient recall view: drop bookkeeping + null provenance. The agent
          // acts on observation/task_context/memory_id (for supersedes) + weighting + refs.
          const memories = result.memories.map((m) => {
            const lean = {
              memory_id: m.memory_id,
              observation: m.observation,
              task_context: m.task_context,
              label: m.label,
              importance: m.importance_score,
              score: Math.round((m.score ?? 0) * 100) / 100,
            };
            if (m.tags && m.tags.length) lean.tags = m.tags;
            if (m.references && m.references.length) lean.references = m.references;
            if (m.failure_signature) lean.failure_signature = m.failure_signature;
            if (m.repo) lean.repo = m.repo;
            if (m.ref) lean.ref = m.ref;
            if (m.file) lean.file = m.file;
            return lean;
          });
          const out = JSON.stringify({ count: result.count, memories });
          return { content: [{ type: 'text', text: out }], details: { exit: 0 } };
        } catch (err) {
          return { content: [{ type: 'text', text: `No memories found. ${err.message}` }], details: { exit: 1 } };
        }
      },
    });

    pi.registerTool({
      name: 'memory_record',
      label: 'Memory Record',
      description: 'Persist a lesson, decision, or finding to the shared agent memory store. ' +
        'Format: "X caused Z because W — do A instead; verify with B." ' +
        'Labels: GOTCHA/BUG for surprises, DECISION for chosen approaches, IMPROVEMENT for better paths. ' +
        'Add references for research conclusions. Pass supersedes to replace a stale memory.',
      promptSnippet: 'Save a lesson or decision to shared persistent memory',
      promptGuidelines: [
        'Call memory_record after any non-trivial discovery. Importance: 1–3 minor · 4–6 useful · 7–8 important · 9–10 critical. ' +
        'Never record: routine status, secrets, stack traces with tokens, or what git already captures.',
      ],
      parameters: Type.Object({
        observation: Type.String({ description: 'The lesson — specific enough to act on. What changed, why, what to do instead.' }),
        task_context: Type.String({ description: 'Why a future agent needs this: which decision it guides or failure it prevents.' }),
        label: Type.String({ description: `Memory category: ${MEMORY_LABELS}` }),
        importance: Type.Optional(Type.Integer({ minimum: 1, maximum: 10, description: '1–3 minor · 4–6 useful · 7–8 important · 9–10 critical/safety. Defaults from label (SECURITY/INCIDENT=9, BUG/RELEASE=8, GOTCHA/IMPROVEMENT=7, DECISION/ARCHITECTURE=6, else 5).' })),
        tags: Type.Optional(Type.Array(Type.String(), { description: 'Keyword tags for recall.' })),
        references: Type.Optional(Type.Array(Type.String(), {
          description: 'Provenance: URLs, pr:owner/repo#N, npm:pkg@v, file:/abs/path:line.',
        })),
        supersedes: Type.Optional(Type.String({ description: 'Memory ID to replace (prevents duplicate stacking).' })),
        failure_signature: Type.Optional(Type.String({ description: 'Clusterable recurring-failure signature: "mechanism:X|cause:Y". Enables mine-weakness to surface patterns across sessions.' })),
      }),
      async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
        try {
          const db = connectDb(ctx?.dbPath ?? resolveDbPath(null));
          const label = params.label?.toUpperCase() ?? 'OTHER';
          const { memory, superseded } = insertMemory(db, {
            agentId: getAwarenessAgentId(ctx),
            taskContext: params.task_context,
            observation: params.observation,
            importanceScore: params.importance ?? defaultImportance(label),
            label,
            tags: params.tags ?? [],
            references: params.references ?? [],
            supersedes: params.supersedes ? [params.supersedes] : [],
            failureSignature: params.failure_signature ?? null,
            cwd: ctx?.cwd ?? process.cwd(),
          });
          // Lean confirmation: the agent already has the text it sent — return only the
          // id + weighting it needs for a follow-up `supersedes`, plus any replaced ids.
          const payload = { memory_id: memory.memory_id, importance: memory.importance_score, label: memory.label };
          if (superseded.length) payload.superseded = superseded;
          return { content: [{ type: 'text', text: JSON.stringify(payload) }], details: { exit: 0 } };
        } catch (err) {
          return { content: [{ type: 'text', text: `Failed: ${err.message}` }], details: { exit: 1 } };
        }
      },
    });

    pi.registerTool({
      name: 'memory_reflect',
      label: 'Memory Reflect',
      description: 'Post-task self-reflection: route what worked/failed into a learning memory and/or a repo-fix note. ' +
        'Call after every non-trivial task. fix_repo creates an open refinement the next agent sees.',
      promptSnippet: 'Record task outcome and lessons; flag repo fixes for the next agent',
      promptGuidelines: [
        'Call memory_reflect after every non-trivial task. Provide outcome + at least one of lesson or didnt_work.',
      ],
      parameters: Type.Object({
        task: Type.String({ description: 'What you just did.' }),
        outcome: Type.Union([Type.Literal('worked'), Type.Literal('partial'), Type.Literal('failed')], { description: 'worked | partial | failed' }),
        lesson: Type.Optional(Type.String({ description: 'Durable lesson for the memory store.' })),
        worked: Type.Optional(Type.String({ description: 'What went well.' })),
        didnt_work: Type.Optional(Type.String({ description: 'What failed. Used as lesson if no lesson given.' })),
        fix_repo: Type.Optional(Type.String({ description: 'Concrete codebase fix note — becomes an open refinement for the next agent.' })),
        fix_harness: Type.Optional(Type.String({ description: 'Improvement to this harness/skill — tagged memory, surfaces via export-harness; a human merges it.' })),
        failure_signature: Type.Optional(Type.String({ description: 'Clusterable recurring-failure signature: "mechanism:X|cause:Y". Powers mine-weakness to surface patterns. Use on partial/failed outcomes.' })),
        importance: Type.Optional(Type.Integer({ minimum: 1, maximum: 10, description: 'Override default importance (failed=8, partial=6, worked=5).' })),
      }),
      async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
        try {
          const db = connectDb(ctx?.dbPath ?? resolveDbPath(null));
          const outcome = ['worked', 'partial', 'failed'].includes(params.outcome) ? params.outcome : 'partial';
          const result = reflectMemory(db, {
            agentId: getAwarenessAgentId(ctx),
            task: params.task,
            outcome,
            lesson: params.lesson,
            worked: params.worked,
            didntWork: params.didnt_work,
            fixRepo: params.fix_repo,
            fixHarness: params.fix_harness,
            failureSignature: params.failure_signature,
            importance: params.importance,
            workspacePath: ctx?.cwd ?? process.cwd(),
            cwd: ctx?.cwd ?? process.cwd(),
          });
          // Lean confirmation: keep the ids the agent needs for follow-ups; drop the
          // always-zero eval stubs and the envelope. Emit a short `next` hint only when
          // an action-bearing artifact was created (refinement / harness fix).
          const payload = { outcome: result.outcome, memory_id: result.learning_memory_id };
          const actions = [];
          if (result.repo_fix_refinement_id) { payload.refinement_id = result.repo_fix_refinement_id; actions.push('refine-get → repo fixes for the next agent'); }
          if (result.harness_fix) { payload.harness_fix = true; actions.push('export-harness → harness improvement (a human merges)'); }
          if (actions.length) payload.next = actions.join(' · ');
          return { content: [{ type: 'text', text: JSON.stringify(payload) }], details: { exit: 0 } };
        } catch (err) {
          return { content: [{ type: 'text', text: `Reflection recorded. ${err.message}` }], details: { exit: 1 } };
        }
      },
    });
  }

  if (!pi?.registerCommand) {
    return;
  }

  pi.registerCommand('octocode-status', {
    description: 'Show Octocode Pi extension assets and bundled skills.',
    handler: async (_args, ctx) => {
      notify(ctx, formatStatus(), 'info');
    },
  });

  pi.registerCommand('octocode-setup', {
    description: 'Install the Octocode APPEND_SYSTEM.md block into .pi or ~/.pi/agent.',
    handler: async (args, ctx) => {
      await installAppendSystem(args, ctx);
    },
  });

  pi.registerCommand('octocode-mcp-install', {
    description: 'Run the Octocode MCP installer using the bundled CLI.',
    handler: async (args, ctx) => {
      const extraArgs = splitArgs(args);
      const script = getBundledOctocodeScript();
      const cmdLabel = script
        ? `node ${path.basename(script)} install ${extraArgs.join(' ')}`.trim()
        : `npx octocode install ${extraArgs.join(' ')}`.trim();

      const ok = await confirm(ctx, 'Run Octocode MCP installer?', `Execute: ${cmdLabel}`);
      if (!ok) {
        notify(ctx, 'Command cancelled.', 'info');
        return;
      }

      notify(ctx, 'Running Octocode MCP installer…', 'info');
      const result = await runOctocode(['install', ...extraArgs], {
        cwd: ctx?.cwd ?? process.cwd(),
        timeout: 60000,
      });

      const output = [result.stdout, result.stderr].filter(Boolean).join('\n').trim();
      if (result.status !== 0) {
        notify(ctx, `MCP install failed:\n${output || 'Unknown error'}`, 'error');
      } else {
        notify(ctx, output || 'Octocode MCP installed successfully.', 'info');
      }
    },
  });

  pi.registerCommand('octocode-handoff', {
    description: 'Internal: open a new session seeded with the next queued handoff summary. Invoked by the handoff_context tool.',
    handler: async (_args, ctx) => {
      const handoff = handoffQueue.shift();

      if (!handoff) {
        notify(ctx, 'octocode-handoff: no queued handoff payload found.', 'warning');
        return;
      }
      const remaining = handoffQueue.length;
      if (remaining > 0) notify(ctx, `octocode-handoff: ${remaining} handoff${remaining === 1 ? '' : 's'} still queued.`, 'info');

      const { summary, kickoff } = handoff;
      await ctx.newSession({
        setup: (sm) => {
          sm.appendMessage({
            role: 'user',
            content: [{ type: 'text', text: `Handoff context:\n\n${summary}` }],
            timestamp: Date.now(),
          });
        },
        withSession: async (newCtx) => {
          await newCtx.sendUserMessage(kickoff);
        },
      });
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
      if (ctx?.reload) {
        await ctx.reload();
      }
    },
  });
}

// Default export preserves the historical single-arg contract: Pi calls `default(pi)`.
// append-mode by default; the octocode-agent launcher sets OCTOCODE_PROMPT_MODE=replace.
export default createOctocodePiExtension();
