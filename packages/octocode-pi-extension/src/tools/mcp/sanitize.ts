import type { ContentPart } from '../../types.js';
import { isDeepStrictEqual } from 'node:util';
import { isPlainRecord } from './config.js';

export function assistantText(message: unknown): string | undefined {
  if (!isPlainRecord(message) || !Array.isArray(message["content"]))
    return undefined;
  const text = message["content"]
    .filter(isPlainRecord)
    .filter(
      (part) => part["type"] === "text" && typeof part["text"] === "string",
    )
    .map((part) => String(part["text"]))
    .join("\n")
    .trim();
  return text || undefined;
}

/**
 * Interop fallback for MCP call results: octocode-mcp (without
 * OCTOCODE_MCP_FULL_TEXT) replaces text content with a compact
 * "structuredContent available …" stub while the real data lives in
 * structuredContent. When the stub sentinel is
 * detected (or content is empty) and structuredContent exists, surface the
 * structured payload instead — otherwise the model researches blind.
 */
export function resolveMcpCallText(payload: unknown): string {
  return resolveMcpCallContent(payload)
    .map((part) => (part.type === "text" ? part.text : stringify(part)))
    .join("\n");
}

/** Preserve both MCP content channels; expand compact stubs instead of duplicating them. */
export function resolveMcpCallContent(payload: unknown): ContentPart[] {
  if (!isPlainRecord(payload))
    return [{ type: "text", text: stringify(payload) }];
  const content = Array.isArray(payload["content"]) ? payload["content"] : [];
  const textBlocks = content.filter(
    (item): item is Record<string, unknown> =>
      isPlainRecord(item) &&
      item["type"] === "text" &&
      typeof item["text"] === "string",
  );
  const structured = payload["structuredContent"];
  const hasStructured = structured !== undefined && structured !== null;
  const onlyStub =
    textBlocks.length > 0 &&
    textBlocks.every((item) =>
      String(item["text"]).startsWith("structuredContent available"),
    );
  if (hasStructured && (textBlocks.length === 0 || onlyStub)) {
    const nonText = content.filter(
      (item) => !(isPlainRecord(item) && item["type"] === "text"),
    );
    return [
      { type: "text", text: stringify(structured) },
      ...nonText.map(toPiContent),
    ];
  }
  if (content.length > 0) {
    const parts = content.map(toPiContent);
    if (hasStructured && !textBlocks.some(item => containsStructuredContent(String(item['text']), structured))) {
      parts.push({ type: 'text', text: stringify(structured) });
    }
    return parts;
  }
  return [{ type: "text", text: stringify(payload) }];
}

function toPiContent(item: unknown): ContentPart {
  if (isPlainRecord(item)) {
    if (item['type'] === 'text' && typeof item['text'] === 'string') {
      return { type: 'text', text: item['text'] };
    }
    if (item['type'] === 'image' && typeof item['data'] === 'string' && typeof item['mimeType'] === 'string') {
      return { type: 'image', data: item['data'], mimeType: item['mimeType'] };
    }
  }
  // Pi accepts text/image blocks; retain other MCP blocks as JSON text.
  return { type: 'text', text: stringify(item) };
}

function containsStructuredContent(text: string, structured: unknown): boolean {
  if (text === stringify(structured)) return true;
  try {
    return isDeepStrictEqual(JSON.parse(text), structured);
  } catch {
    // Ordinary prose is a separate content channel, not a JSON copy.
    return false;
  }
}

function mcpStructuredRows(payload: unknown): Record<string, unknown>[] {
  if (!isPlainRecord(payload) || !isPlainRecord(payload["structuredContent"])) {
    return [];
  }
  const rows = payload["structuredContent"]["results"];
  return Array.isArray(rows)
    ? rows.filter((row): row is Record<string, unknown> => isPlainRecord(row))
    : [];
}

function numericField(record: Record<string, unknown>, key: string): number {
  const value = record[key];
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function structuredRowMetrics(row: Record<string, unknown>): {
  data: Record<string, unknown>;
  matches: number;
  files: number;
  references: number;
  chars: number;
} {
  const data = isPlainRecord(row["data"]) ? row["data"] : {};
  const stats = isPlainRecord(data["stats"]) ? data["stats"] : {};
  return {
    data,
    matches: numericField(stats, "totalOccurrences"),
    files: numericField(stats, "filesMatched") || numericField(data, "totalFiles"),
    references: numericField(data, "totalReferences"),
    chars: numericField(data, "returnedChars"),
  };
}

function plural(value: number, singular: string): string {
  const pluralWord = singular === "match" ? "matches" : `${singular}s`;
  return `${value} ${value === 1 ? singular : pluralWord}`;
}

/** Aggregate every structured MCP batch row so receipts never report only row zero. */
export function summarizeMcpStructuredResults(payload: unknown): string | undefined {
  const rows = mcpStructuredRows(payload);
  if (rows.length === 0) return undefined;
  const totals = rows.reduce<{ matches: number; files: number; references: number; chars: number }>(
    (sum, row) => {
      const metric = structuredRowMetrics(row);
      sum.matches += metric.matches;
      sum.files += metric.files;
      sum.references += metric.references;
      sum.chars += metric.chars;
      return sum;
    },
    { matches: 0, files: 0, references: 0, chars: 0 },
  );
  return [
    plural(rows.length, "result"),
    totals.matches > 0 ? plural(totals.matches, "match") : undefined,
    totals.files > 0 ? plural(totals.files, "file") : undefined,
    totals.references > 0 ? plural(totals.references, "reference") : undefined,
    totals.chars > 0 ? `${totals.chars} chars` : undefined,
  ]
    .filter((part): part is string => Boolean(part))
    .join(" · ");
}

function tableCell(value: unknown): string {
  return String(value ?? "").replace(/[|\r\n]+/g, " ").replace(/\s+/g, " ").trim();
}

function summarizeStructuredRow(row: Record<string, unknown>): string {
  const { data, matches, files, references, chars } = structuredRowMetrics(row);
  if (typeof data["error"] === "string") return tableCell(data["error"]);
  const explicit = typeof data["summary"] === "string" ? data["summary"] : undefined;
  if (explicit) return tableCell(explicit);
  const metrics = [
    matches > 0 ? plural(matches, "match") : undefined,
    files > 0 ? plural(files, "file") : undefined,
    references > 0 ? plural(references, "reference") : undefined,
    chars > 0 ? `${chars} chars` : undefined,
    numericField(data, "totalLines") > 0
      ? plural(numericField(data, "totalLines"), "line")
      : undefined,
  ].filter((part): part is string => Boolean(part));
  return metrics.join(" · ") || "ok";
}

/** Optional summary followed by complete evidence, including continuations. */
export function resolveMcpCallTable(payload: unknown): ContentPart[] | undefined {
  const rows = mcpStructuredRows(payload);
  const aggregate = summarizeMcpStructuredResults(payload);
  if (rows.length === 0 || !aggregate) return undefined;
  const lines = rows.map((row, position) => {
    const { data } = structuredRowMetrics(row);
    const index = typeof row["index"] === "number" ? row["index"] : position;
    const status = tableCell(row["status"] ?? (data["error"] ? "error" : "ok"));
    const item = tableCell(
      data["path"] ?? data["file"] ?? data["uri"] ?? data["name"] ?? `[${index}]`,
    );
    return `${index} | ${status} | ${item} | ${summarizeStructuredRow(row)}`;
  });
  return [
    {
      type: "text",
      text: `${aggregate}\nindex | status | item | summary\n${lines.join("\n")}`,
    },
    ...resolveMcpCallContent(payload),
  ];
}

/** Session/UI metadata only; provider-visible MCP bytes live exclusively in content. */
export function summarizeMcpCallDetails(payload: unknown): Record<string, unknown> {
  const record = isPlainRecord(payload) ? payload : {};
  const content = Array.isArray(record['content']) ? record['content'] : [];
  const summary = summarizeMcpStructuredResults(payload);
  return {
    isError: record['isError'] === true,
    contentBlocks: content.length,
    textBlocks: content.filter((item) => isPlainRecord(item) && item['type'] === 'text').length,
    imageBlocks: content.filter((item) => isPlainRecord(item) && item['type'] === 'image').length,
    hasStructuredContent: record['structuredContent'] !== undefined && record['structuredContent'] !== null,
    ...(summary ? { summary } : {}),
  };
}

export function stringify(value: unknown): string {
  return typeof value === "string"
    ? value
    : (JSON.stringify(value, null, 2) ?? String(value));
}
