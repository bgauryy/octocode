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
import { truncateUserVisibleToolOutput } from '../utils.js';
import type { TSchema, ToolDefinition, ToolCallResult, PiTheme } from '../types.js';

// ─── ANSI-safe line-width helpers (inline — no external dep) ─────────────────
// Matches CSI sequences (ESC [ ... m) and other 2-char ESC sequences.
const ANSI_ESC_RE = /\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])/g;

function visibleWidth(str: string): number {
  return str.replace(ANSI_ESC_RE, '').length;
}

/**
 * Truncate `str` so its *visible* width (ANSI codes excluded) does not exceed
 * `maxWidth`. An ellipsis is appended and an SGR reset is added so open color
 * sequences from the truncated portion don't bleed into the next line.
 */
function truncateToWidth(str: string, maxWidth: number, ellipsis = '\u2026'): string {
  if (maxWidth <= 0) return '';
  if (visibleWidth(str) <= maxWidth) return str;
  const ellipsisLen = visibleWidth(ellipsis);
  const target = maxWidth - ellipsisLen;
  if (target <= 0) return ellipsis.slice(0, maxWidth);

  let visible = 0;
  let i = 0;
  while (i < str.length) {
    // Skip an ANSI escape sequence without counting visible width.
    const esc = ANSI_ESC_RE.exec(str.slice(i));
    if (esc && esc.index === 0) {
      i += esc[0].length;
      ANSI_ESC_RE.lastIndex = 0;
      continue;
    }
    ANSI_ESC_RE.lastIndex = 0;
    if (visible >= target) break;
    visible++;
    i++;
  }
  return str.slice(0, i) + ellipsis + '\x1b[0m';
}
// ─────────────────────────────────────────────────────────────────────────────

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
  return {
    content,
    details,
  };
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
    async execute(_toolCallId: string, params: Record<string, unknown>) {
      // Route directly to localBinaryInspect — shared execution, no duplication.
      return executeOctocodeToolForPi('localBinaryInspect', params);
    },
    renderCall(args: unknown, theme?: PiTheme) {
      const preview = JSON.stringify(args ?? {});
      const short = preview.length > 96 ? `${preview.slice(0, 96)}…` : preview;
      const rawLine = `${theme?.fg('toolTitle', theme.bold('unzip')) ?? 'unzip'} ${theme?.fg('dim', short) ?? short}`;
      return {
        render: (width: number) => [truncateToWidth(rawLine, width)],
        invalidate() { /* no-op */ },
      };
    },
    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
      if (opts.isPartial) {
        return {
          render: (width: number) => [truncateToWidth(theme?.fg('warning', 'Unpacking…') ?? 'Unpacking…', width)],
          invalidate() { /* no-op */ },
        };
      }
      const ok = !result.isError;
      const details = result.details as { results?: Array<{ data?: { localPath?: string } }> } | null;
      const localPath = details?.results?.[0]?.data?.localPath;
      const suffix = localPath ? ` → ${localPath}` : '';
      const header = `${theme?.fg(ok ? 'success' : 'error', ok ? '✓' : '✗') ?? (ok ? '✓' : '✗')} ${theme?.fg('toolTitle', 'unzip') ?? 'unzip'}${theme?.fg('dim', suffix) ?? suffix}`;
      if (!opts.expanded) {
        return {
          render: (width: number) => [truncateToWidth(`${header}${theme?.fg('dim', ' · expand for full output') ?? ' · expand for full output'}`, width)],
          invalidate() { /* no-op */ },
        };
      }
      const text = (result.content as Array<{ type: string; text: string }>)?.find?.((p) => p.type === 'text')?.text ?? '';
      const preview = truncateUserVisibleToolOutput(text);
      return {
        render: (width: number) => [
          truncateToWidth(header, width),
          ...preview.text.split('\n').map((line) => truncateToWidth(theme?.fg('dim', line) ?? line, width)),
        ],
        invalidate() { /* no-op */ },
      };
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
      async execute(_toolCallId: string, params: Record<string, unknown>) {
        return executeOctocodeToolForPi(toolName, params);
      },
      renderCall(args: unknown, theme?: PiTheme) {
        const preview = JSON.stringify(args ?? {});
        // Pre-trim to 96 raw chars so the visible portion is already short;
        // render() will truncate further if the terminal is narrower.
        const shortPreview = preview.length > 96 ? `${preview.slice(0, 96)}…` : preview;
        const rawLine = `${theme?.fg('toolTitle', theme.bold(toolName)) ?? toolName} ${theme?.fg('dim', shortPreview) ?? shortPreview}`;
        return {
          render: (width: number) => [truncateToWidth(rawLine, width)],
          invalidate() { /* no-op */ },
        };
      },
      renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
        if (opts.isPartial)
          return {
            render: (width: number) => [
              truncateToWidth(
                theme?.fg('warning', 'Octocode tool running…') ?? 'Octocode tool running…',
                width,
              ),
            ],
            invalidate() { /* no-op */ },
          };
        const details = result.details as {
          results?: unknown[];
          data?: { files?: unknown[] };
        } | null;
        const ok = !result.isError;
        const count = Array.isArray(details?.results)
          ? details.results.length
          : Array.isArray(details?.data?.files)
            ? details?.data?.files.length
            : undefined;
        const suffix =
          typeof count === 'number'
            ? ` · ${count} item${count === 1 ? '' : 's'}`
            : '';
        const header = `${theme?.fg(ok ? 'success' : 'error', ok ? '✓' : '✗') ?? (ok ? '✓' : '✗')} ${theme?.fg('toolTitle', toolName) ?? toolName}${suffix}`;
        if (!opts.expanded) {
          return {
            render: (width: number) => [
              truncateToWidth(
                `${header}${theme?.fg('dim', ' · expand for full output') ?? ' · expand for full output'}`,
                width,
              ),
            ],
            invalidate() { /* no-op */ },
          };
        }
        const text =
          (result.content as Array<{ type: string; text: string }>)
            ?.find?.((part) => part.type === 'text')?.text ?? '';
        const preview = truncateUserVisibleToolOutput(text);
        return {
          // Each element of the returned array must be a single line ≤ width.
          // Split preview.text on newlines so multi-line tool output is handled
          // correctly, then truncate every line individually.
          render: (width: number) => {
            const previewLines = preview.text
              .split('\n')
              .map((line) => truncateToWidth(theme?.fg('dim', line) ?? line, width));
            const truncationNotice = preview.truncated
              ? [
                  truncateToWidth(
                    theme?.fg(
                      'muted',
                      `… user preview truncated (${preview.omittedChars} chars hidden; full output stays available to the agent)`,
                    ) ?? `… ${preview.omittedChars} chars hidden`,
                    width,
                  ),
                ]
              : [];
            return [
              truncateToWidth(header, width),
              ...previewLines,
              ...truncationNotice,
            ];
          },
          invalidate() { /* no-op */ },
        };
      },
    });
  }

  await registerUnzipTool(pi, Type, registeredToolNames);
}
