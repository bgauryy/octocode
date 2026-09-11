import type {
  PiTheme,
  RenderCallReturn,
  RenderContext,
  ToolCallResult,
} from '../../types.js';
import { truncateToWidth } from '../../tui/width.js';
import {
  buildOctocodeSingleRenderCall,
  buildOctocodeRenderCall,
  buildOctocodeRenderResult,
  buildToolView,
  extractQueryResultRows,
  makeCachedRenderer,
  makeComponentRenderer,
} from '../render-helpers.js';
import {
  DEFAULT_OCTOCODE_MCP_SERVER_NAME,
  isPlainRecord,
} from './config.js';
import type { McpSchemaValidationError } from './schema-validator.js';

export function summarizeSchema(tool: Record<string, unknown>): string {
  const schema = tool["inputSchema"];
  if (!isPlainRecord(schema)) return "";
  const required = Array.isArray(schema["required"])
    ? schema["required"].map(String).filter(Boolean)
    : [];
  const properties = isPlainRecord(schema["properties"])
    ? Object.keys(schema["properties"])
    : [];
  const fields = required.length > 0 ? required : properties;
  return fields.length > 0
    ? ` schema: ${fields.slice(0, 8).join(", ")}${fields.length > 8 ? ", …" : ""}`
    : " schema: object";
}

export function formatMcpSchemaValidationErrors(errors: McpSchemaValidationError[], _target: { server: string; tool: string }): string {
  const seen = new Set<string>();
  const lines = errors.flatMap((error) => {
    const message = /schema is false|expected never/i.test(error.message)
      ? 'field is not allowed for the selected operation'
      : error.message;
    const line = `- ${error.instancePath || "/"}: ${message}`;
    if (seen.has(line)) return [];
    seen.add(line);
    return [line];
  });
  return lines.join("\n");
}

/** Prefer stable evidence counts over transport or structural wrapper text. */
export function summarizeMcpBatchResult(result: ToolCallResult): string {
  const detailSummary =
    isPlainRecord(result.details) &&
    typeof result.details["summary"] === "string"
      ? result.details["summary"]
      : undefined;
  if (detailSummary) return detailSummary;
  const text =
    (result.content as Array<{ type: string; text?: string }>).find(
      (part) => part.type === "text",
    )?.text ?? "";
  const lines = text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  if (result.isError) return lines.at(-1) ?? "failed";

  const values: Record<string, string> = {};
  for (const line of lines) {
    const colon = line.indexOf(":");
    if (colon <= 0) continue;
    const key = line.slice(0, colon).trim();
    const value = line.slice(colon + 1).trim();
    if (value && !values[key]) values[key] = value;
  }
  if (values["summary"]) return values["summary"];
  const matches: string[] = [];
  if (values["totalOccurrences"])
    matches.push(`${values["totalOccurrences"]} matches`);
  if (
    values["filesMatched"] &&
    values["filesMatched"] !== values["totalOccurrences"]
  ) {
    matches.push(
      `${values["filesMatched"]} file${values["filesMatched"] === "1" ? "" : "s"}`,
    );
  }
  if (matches.length > 0) return matches.join(" · ");
  if (values["returnedChars"] && values["totalLines"])
    return `${values["returnedChars"]} chars · ${values["totalLines"]} lines`;
  if (values["totalLines"]) return `${values["totalLines"]} lines`;
  if (values["returnedChars"]) return `${values["returnedChars"]} chars`;
  if (values["totalEntries"]) return `${values["totalEntries"]} entries`;

  const structuralKey =
    /^(results|base|pagination|data|stats|files|next|hints|shared|status|path|result|id|meta|reasoning|text|content|modified|fileType|name|capped|searchTime|searchEngine|matchedLines|filesSearched|bytesSearched|totalFiles|totalMatches|totalMatchRows|returnedMatchRows)$/i;
  const meaningful = lines.find((line) => {
    const key = (line.split(":")[0] ?? "").trim();
    return (
      !structuralKey.test(key) &&
      !line.startsWith("-") &&
      !line.startsWith("✓") &&
      !line.startsWith("✗") &&
      line.length > 2
    );
  });
  return meaningful ?? lines[0] ?? "ok";
}

function formatMcpTarget(args: unknown): { action: string; target: string } {
  const params = isPlainRecord(args) ? args : {};
  const action = String(params["action"] ?? "operation");
  const target =
    [params["server"], params["tool"]].filter(Boolean).join("/") ||
    "configured servers";
  return { action, target };
}

function clip(text: string, width: number): string {
  return truncateToWidth(text.replace(/\s+/g, " ").trim(), width);
}

function extractQueryParams(args: unknown): Record<string, unknown> {
  const envelope = isPlainRecord(args) ? args : {};
  const queries = Array.isArray(envelope["queries"])
    ? envelope["queries"]
    : undefined;
  return queries?.length === 1 && isPlainRecord(queries[0]) ? queries[0] : {};
}

export function renderMcpCall(
  args: unknown,
  theme?: PiTheme,
): RenderCallReturn {
  const params = extractQueryParams(args);
  const server =
    typeof params["server"] === "string"
      ? params["server"]
      : DEFAULT_OCTOCODE_MCP_SERVER_NAME;
  if (params["action"] === "call" && typeof params["tool"] === "string") {
    const displayName =
      server === DEFAULT_OCTOCODE_MCP_SERVER_NAME
        ? params["tool"]
        : `${server}.${params["tool"]}`;
    const innerEnvelope = isPlainRecord(params["arguments"])
      ? params["arguments"]
      : {};
    const innerQueryCount = Array.isArray(innerEnvelope["queries"])
      ? innerEnvelope["queries"].length
      : 0;
    return innerQueryCount > 1
      ? buildOctocodeRenderCall(displayName, params["arguments"], theme)
      : buildOctocodeSingleRenderCall(displayName, params["arguments"], theme);
  }

  const { action, target } = formatMcpTarget(params);
  return makeComponentRenderer((_props, { width }) => {
    const line = `mcp ${action} · ${target}`;
    return [theme?.fg ? theme.fg("dim", clip(line, width)) : clip(line, width)];
  }, undefined);
}

export function renderMcpResult(
  resultValue: ToolCallResult,
  opts: { expanded?: boolean; isPartial?: boolean },
  theme?: PiTheme,
  context?: RenderContext,
): RenderCallReturn {
  const envelope = isPlainRecord(context?.args) ? context.args : {};
  const queryList = Array.isArray(envelope["queries"])
    ? (envelope["queries"] as Record<string, unknown>[])
    : [];
  if (queryList.length > 1) {
    const rows = extractQueryResultRows(resultValue);
    if (rows.length > 1) {
      return makeCachedRenderer((width) =>
        rows.flatMap((row) => {
          const query = queryList[row.index] ?? {};
          const queryServer =
            typeof query["server"] === "string"
              ? query["server"]
              : DEFAULT_OCTOCODE_MCP_SERVER_NAME;
          const toolName =
            query["action"] === "call" && typeof query["tool"] === "string"
              ? queryServer === DEFAULT_OCTOCODE_MCP_SERVER_NAME
                ? query["tool"]
                : `${queryServer}.${query["tool"]}`
              : "MCPTool";
          return buildToolView(
            {
              name: toolName,
              state:
                row.status === "success"
                  ? "success"
                  : row.status === "failed"
                    ? "error"
                    : "neutral",
              segments: row.summary
                ? [{
                    text: row.summary,
                    token: row.status === "success" ? "dim" : "error",
                  }]
                : [],
            },
            theme,
          ).render(width);
        }),
      );
    }
  }

  const args = extractQueryParams(context?.args);
  const server =
    typeof args["server"] === "string"
      ? args["server"]
      : DEFAULT_OCTOCODE_MCP_SERVER_NAME;
  if (
    args["action"] === "call" &&
    server === DEFAULT_OCTOCODE_MCP_SERVER_NAME &&
    typeof args["tool"] === "string"
  ) {
    return buildOctocodeRenderResult(
      args["tool"],
      resultValue,
      opts,
      theme,
      context,
    );
  }

  const { action, target } = formatMcpTarget(args);
  if (opts.isPartial) {
    return makeComponentRenderer((_props, { width }) => {
      const line = `mcp ${action} · ${target} · running…`;
      return [
        theme?.fg ? theme.fg("accent", clip(line, width)) : clip(line, width),
      ];
    }, undefined);
  }
  const lines = (resultValue.content[0] as { text?: string } | undefined)?.text
    ?.split("\n")
    .filter(Boolean) ?? ["MCP result"];
  const head = lines[0] ?? "MCP result";
  const second = lines.find((line) => /^[-•]\s+|\w+:\s/.test(line));
  const isError = Boolean(resultValue.isError) || Boolean(context?.isError);
  const prefix = isError ? "mcp error" : `mcp ${action}`;
  return makeComponentRenderer((_props, { width }) => {
    const color = isError ? "error" : "dim";
    const rendered = [`${prefix} · ${target} · ${head}`];
    if (second && second !== head) rendered.push(`  ${second}`);
    return rendered.map((line) =>
      theme?.fg ? theme.fg(color, clip(line, width)) : clip(line, width),
    );
  }, undefined);
}

(renderMcpResult as { multiQueryAware?: boolean }).multiQueryAware = true;
