/**
 * Registration of the 14 native Octocode direct tools.
 *
 * The tool schema + description are loaded from @octocodeai/octocode-tools-core/schema
 * (engine-free). Execution loads /direct + /config lazily so the native addon is
 * never required during extension boot or schema inspection.
 */
import {
  formatDirectToolSchemaText,
  getDirectToolCategory as getCoreDirectToolCategory,
  getDirectToolDescription as getCoreDirectToolDescription,
  loadToolContent,
} from '@octocodeai/octocode-tools-core/schema';
import { OCTOCODE_DIRECT_TOOL_NAMES } from '../constants.js';
import { recordFileReadState } from './edit-tool.js';
import type { TSchema, ToolDefinition, ToolCallResult, PiTheme } from '../types.js';

// ─── Shared rendering helpers (ANSI truncation + smart call/result renderers) ──
import {
  truncateToWidth,
  buildOctocodeRenderCall,
  buildOctocodeRenderResult,
} from './render-helpers.js';

// ─── TypeBox (dynamic import — Pi runtime dep) ────────────────────────────────

type TypeBoxBuilder = (typeof import('typebox'))['Type'];

// ─── Tool metadata helpers ────────────────────────────────────────────────────

let octocodeToolMetadataPromise: Promise<unknown> | null = null;

async function getOctocodeToolMetadata(): Promise<unknown> {
  if (!octocodeToolMetadataPromise) {
    octocodeToolMetadataPromise = loadToolContent().catch(() => null);
  }
  return octocodeToolMetadataPromise;
}

interface OctocodeToolSchema {
  kind: 'octocode.toolSchema';
  version: 1;
  name: string;
  category: string;
  description: string;
  fullDescription: string;
  inputSchema: Record<string, unknown>;
}

async function getOctocodeToolSchema(toolName: string): Promise<OctocodeToolSchema> {
  const metadata = await getOctocodeToolMetadata();
  const fullDescription = getCoreDirectToolDescription(toolName, metadata as Parameters<typeof getCoreDirectToolDescription>[1]);
  return {
    kind: 'octocode.toolSchema',
    version: 1,
    name: toolName,
    category: getCoreDirectToolCategory(toolName) as string,
    description: firstSentence(fullDescription),
    fullDescription,
    inputSchema: JSON.parse(formatDirectToolSchemaText(toolName)) as Record<string, unknown>,
  };
}

// ─── Private helpers ─────────────────────────────────────────────────────────

function toTitleCaseName(toolName: string): string {
  return toolName
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/^gh\b/, 'GitHub')
    .replace(/^npm\b/, 'npm')
    .replace(/^lsp\b/, 'LSP')
    .replace(/^local\b/, 'Local')
    .trim();
}

function firstSentence(text: string | null | undefined): string {
  const normalized = String(text ?? '')
    .replace(/\s+/g, ' ')
    .trim();
  if (normalized.length === 0) return '';
  const pipeParts = normalized
    .split(/\s+\|\s+/)
    .map((part) => part.trim())
    .filter(Boolean);
  return (
    pipeParts.find(
      (part) =>
        part.length > 0 &&
        !/^(github|local|npm|package|search|other)$/i.test(part),
    ) ?? normalized
  );
}

function buildOctocodeToolGuidelines(toolName: string): string[] {
  const guidelines = [
    `${toolName} is a native Pi tool backed by the bundled Octocode CLI tools command; pass arguments using this tool's Pi schema directly.`,
  ];
  if (toolName.startsWith('local') || toolName.startsWith('lsp')) {
    guidelines.push(
      `${toolName} local paths should be absolute when possible; strip a leading @ if the model copied a Pi file reference.`,
    );
  }
  if (toolName === 'localSearchCode') {
    guidelines.push(
      'Use localSearchCode mode:"discovery" for paths first, then localGetFileContent for exact slices.',
    );
  }
  return guidelines;
}

function getToolFieldPreview(schema: OctocodeToolSchema): string {
  const required = Array.isArray(schema.inputSchema['required'])
    ? (schema.inputSchema['required'] as string[])
    : [];
  return required.slice(0, 4).join(', ');
}

function buildOctocodeToolParameters(Type: TypeBoxBuilder, schema: OctocodeToolSchema): TSchema {
  return Type.Unsafe(schema.inputSchema);
}

function getOctocodeToolCategory(schema: OctocodeToolSchema): string {
  return typeof schema.category === 'string' ? schema.category : 'Octocode';
}

// ─── Execution ───────────────────────────────────────────────────────────────

interface DirectToolResult {
  content?: Array<{ type: string; text: string }>;
  isError?: boolean;
  structuredContent?: unknown;
}

async function executeOctocodeToolForPi(
  toolName: string,
  params: Record<string, unknown>,
  signal?: AbortSignal,
  ctx?: { cwd?: string },
): Promise<ToolCallResult> {
  if (signal?.aborted) throw new Error(`Octocode tool ${toolName} was cancelled before it started.`);
  const { setRuntimeSurface, invalidateConfigCache } = await import(
    '@octocodeai/octocode-tools-core/config'
  );
  const { executeDirectTool } = await import('@octocodeai/octocode-tools-core/direct');
  (setRuntimeSurface as (s: string) => void)('cli');
  (invalidateConfigCache as () => void)();
  const result = (await (executeDirectTool as (name: string, params: unknown) => Promise<DirectToolResult>)(toolName, params));
  if (signal?.aborted) throw new Error(`Octocode tool ${toolName} was cancelled.`);
  const details = result.structuredContent ?? result;
  const content =
    Array.isArray(result.content) && result.content.length > 0
      ? (result.content as Array<{ type: 'text'; text: string }>)
      : [{ type: 'text' as const, text: JSON.stringify(details) }];
  if (result.isError) {
    const text = content.find((part) => part.type === 'text')?.text ?? JSON.stringify(details);
    throw new Error(text);
  }
  if (toolName === 'localGetFileContent') {
    await recordLocalGetFileContentReads(params, ctx?.cwd ?? process.cwd());
  }
  return {
    content,
    details,
  };
}

async function recordLocalGetFileContentReads(params: Record<string, unknown>, cwd: string): Promise<void> {
  const queries = Array.isArray(params['queries']) ? params['queries'] : [];
  await Promise.all(queries.map(async (query) => {
    if (!query || typeof query !== 'object') return;
    const filePath = (query as Record<string, unknown>)['path'];
    if (typeof filePath !== 'string' || filePath.trim().length === 0) return;
    try {
      await recordFileReadState(filePath, cwd);
    } catch {
      // Read-state tracking is an edit-safety enhancement; never fail localGetFileContent because tracking failed.
    }
  }));
}

// ─── Registration ─────────────────────────────────────────────────────────────

export function registerUniqueTool(
  pi: { registerTool?(def: ToolDefinition): void },
  registeredToolNames: Set<string>,
  toolDefinition: ToolDefinition,
): void {
  if (registeredToolNames.has(toolDefinition.name)) {
    throw new Error(
      `Octocode Pi extension tool name collision: ${toolDefinition.name}`,
    );
  }
  registeredToolNames.add(toolDefinition.name);
  pi.registerTool?.(toolDefinition);
}

/**
 * Register the `unzip` tool.
 *
 * Schema and execution both delegate to `localBinaryInspect` — no schema
 * duplication. The tool is a named alias that makes the unpack workflow
 * discoverable without agents having to know about mode:'unpack'.
 */
async function registerUnzipTool(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
): Promise<void> {
  // Reuse the canonical localBinaryInspect schema from octocode-core.
  const schema = await getOctocodeToolSchema('localBinaryInspect');
  const description =
    'Unpack an archive (.zip, .jar, .tar.gz, .7z, .deb, .dmg, etc.) to a local directory ' +
    'under Octocode home, then research it with local tools. ' +
    'Pass mode:"unpack" — execution routes to localBinaryInspect. ' +
    'Returns localPath — follow up with localViewStructure, localSearchCode, or localGetFileContent.';

  registerUniqueTool(pi, registeredToolNames, {
    name: 'unzip',
    label: 'Local Code: Unzip Archive',
    description,
    promptSnippet: 'Unpack an archive for local research. Pass mode:"unpack". Required: path.',
    promptGuidelines: [
      'unzip is a native Pi tool backed by the bundled Octocode CLI tools command; pass arguments using this tool\'s Pi schema directly.',
      'unzip local paths should be absolute when possible; strip a leading @ if the model copied a Pi file reference.',
      'unzip uses the localBinaryInspect schema — always pass mode:"unpack". Follow the returned localPath with localViewStructure, localSearchCode, or localGetFileContent.',
    ],
    parameters: buildOctocodeToolParameters(Type, schema),
    async execute(_toolCallId: string, params: Record<string, unknown>, signal?: AbortSignal, _onUpdate?: unknown, ctx?: { cwd?: string }) {
      // Route directly to localBinaryInspect — shared execution, no duplication.
      return executeOctocodeToolForPi('localBinaryInspect', params, signal, ctx);
    },
    renderCall(args: unknown, theme?: PiTheme) {
      // unzip is an alias for localBinaryInspect; show path + mode
      return buildOctocodeRenderCall('localBinaryInspect', args, theme);
    },
    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
      if (opts.isPartial) {
        const msg = theme?.fg('warning', 'Unpacking…') ?? 'Unpacking…';
        return { render: (w: number) => [truncateToWidth(msg, w)], invalidate() { /* no-op */ } };
      }
      // Show localPath prominently in collapsed view
      const ok = !result.isError;
      const details = result.details as { results?: Array<{ data?: { localPath?: string } }> } | null;
      const localPath = details?.results?.[0]?.data?.localPath;
      const icon = theme?.fg(ok ? 'success' : 'error', ok ? '✓' : '✗') ?? (ok ? '✓' : '✗');
      const nameStr = theme?.fg('toolTitle', 'unzip') ?? 'unzip';
      const pathStr = localPath ? (theme?.fg('dim', ` → ${localPath}`) ?? ` → ${localPath}`) : '';
      const header = `${icon} ${nameStr}${pathStr}`;
      if (!opts.expanded) {
        const hint = theme?.fg('dim', ' · expand for full output') ?? ' · expand for full output';
        return { render: (w: number) => [truncateToWidth(`${header}${hint}`, w)], invalidate() { /* no-op */ } };
      }
      return buildOctocodeRenderResult('localBinaryInspect', result, opts, theme);
    },
  });
}

export async function registerOctocodeTools(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
): Promise<void> {
  for (const toolName of OCTOCODE_DIRECT_TOOL_NAMES) {
    // unzip is registered separately — it wraps localBinaryInspect, not a schema tool.
    if (toolName === 'unzip') continue;
    const schema = await getOctocodeToolSchema(toolName);
    const description = schema.fullDescription || schema.description || `${toolName} Octocode tool`;
    const fieldPreview = getToolFieldPreview(schema);
    const promptSnippet = fieldPreview
      ? `${firstSentence(description)} Required: ${fieldPreview}.`
      : firstSentence(description);

    registerUniqueTool(pi, registeredToolNames, {
      name: toolName,
      label: `${getOctocodeToolCategory(schema)}: ${toTitleCaseName(toolName)}`,
      description,
      promptSnippet,
      promptGuidelines: buildOctocodeToolGuidelines(toolName),
      parameters: buildOctocodeToolParameters(Type, schema),
      async execute(_toolCallId: string, params: Record<string, unknown>, signal?: AbortSignal, _onUpdate?: unknown, ctx?: { cwd?: string }) {
        return executeOctocodeToolForPi(toolName, params, signal, ctx);
      },
      renderCall(args: unknown, theme?: PiTheme) {
        // Smart per-tool-category call summary: shows keywords, owner/repo, path, symbol, etc.
        return buildOctocodeRenderCall(toolName, args, theme);
      },
      renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
        // Smart per-tool-category result stats: match counts, file paths, repo names, etc.
        return buildOctocodeRenderResult(toolName, result, opts, theme);
      },
    });
  }

  await registerUnzipTool(pi, Type, registeredToolNames);
}
