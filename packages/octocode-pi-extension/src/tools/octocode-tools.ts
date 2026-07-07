/**
 * Registration of the 13 native Octocode direct tools.
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

export async function registerOctocodeTools(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
): Promise<void> {
  for (const toolName of OCTOCODE_DIRECT_TOOL_NAMES) {
    let schema: OctocodeToolSchema;
    try {
      schema = await getOctocodeToolSchema(toolName);
    } catch (error) {
      // A malformed schema for ONE tool (e.g. bad JSON from core) must not take
      // down registration of the other 12. Skip it and continue.
      // eslint-disable-next-line no-console
      console.error(`Octocode: skipping tool "${toolName}" — schema load failed: ${(error as Error)?.message ?? error}`);
      continue;
    }
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

}
