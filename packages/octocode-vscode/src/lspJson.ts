import type { Json, Obj } from './ideBridgeTypes';

/**
 * VS Code objects -> LSP JSON. VS Code symbol kinds are 0-based and LSP's are
 * 1-based; diagnostic severities likewise. Inputs come from the editor's own
 * providers, so they are read structurally (extensions may return plain
 * objects) rather than through `instanceof`.
 */

type LspPosition = { line: number; character: number };
export type LspRange = { start: LspPosition; end: LspPosition };

export function isObject(value: unknown): value is Obj {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function text(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback;
}

export function toRange(value: unknown): LspRange {
  const range = value as { start: LspPosition; end: LspPosition };
  return {
    start: { line: range.start.line, character: range.start.character },
    end: { line: range.end.line, character: range.end.character },
  };
}

function optionalRange(value: unknown): LspRange | undefined {
  return isObject(value) ? toRange(value) : undefined;
}

/** `vscode.Uri` -> its string form (`file:///...`). */
export function toUri(value: unknown): string {
  return String(value);
}

function toTags(value: unknown): number[] | undefined {
  return Array.isArray(value) && value.length ? (value as number[]) : undefined;
}

/** Location -> {uri, range}; LocationLink -> LSP LocationLink. */
export function toLocation(value: unknown): Json {
  const item = value as Obj;
  if ('targetUri' in item) {
    const originSelectionRange = optionalRange(item.originSelectionRange);
    return {
      ...(originSelectionRange ? { originSelectionRange } : {}),
      targetUri: toUri(item.targetUri),
      targetRange: toRange(item.targetRange),
      targetSelectionRange: toRange(
        item.targetSelectionRange ?? item.targetRange
      ),
    };
  }
  return { uri: toUri(item.uri), range: toRange(item.range) };
}

/** DocumentSymbol -> LSP DocumentSymbol; SymbolInformation -> LSP SymbolInformation. */
export function toSymbol(value: unknown): Json {
  const symbol = value as Obj;
  const tags = toTags(symbol.tags);
  const base = {
    name: text(symbol.name),
    kind: (symbol.kind as number) + 1,
    ...(tags ? { tags } : {}),
  };
  if (isObject(symbol.location)) {
    const containerName = text(symbol.containerName);
    return {
      ...base,
      ...(containerName ? { containerName } : {}),
      location: toLocation(symbol.location),
    };
  }
  return {
    ...base,
    detail: text(symbol.detail),
    range: toRange(symbol.range),
    selectionRange: toRange(symbol.selectionRange),
    children: Array.isArray(symbol.children)
      ? symbol.children.map(toSymbol)
      : [],
  };
}

function markdown(content: unknown): string {
  if (typeof content === 'string') return content;
  if (!isObject(content)) return '';
  const value = text(content.value);
  return typeof content.language === 'string'
    ? `\`\`\`${content.language}\n${value}\n\`\`\``
    : value;
}

/** All hovers of the position merged into one markdown string; null when empty. */
export function toHover(values: unknown[]): Json {
  const hovers = values as Obj[];
  const contents = hovers
    .flatMap(hover =>
      (Array.isArray(hover.contents) ? hover.contents : [hover.contents]).map(
        markdown
      )
    )
    .filter(part => part.length > 0);
  if (contents.length === 0) return null;
  const range = optionalRange(hovers.find(hover => hover.range)?.range);
  return { contents: contents.join('\n\n'), ...(range ? { range } : {}) };
}

export function toDiagnostic(value: unknown): Json {
  const diagnostic = value as Obj;
  const code = isObject(diagnostic.code)
    ? diagnostic.code.value
    : diagnostic.code;
  const tags = toTags(diagnostic.tags);
  const source = text(diagnostic.source);
  const related = Array.isArray(diagnostic.relatedInformation)
    ? (diagnostic.relatedInformation as Obj[]).map(info => ({
        location: toLocation(info.location),
        message: text(info.message),
      }))
    : [];
  return {
    range: toRange(diagnostic.range),
    severity: (diagnostic.severity as number) + 1,
    message: text(diagnostic.message),
    ...(code === undefined ? {} : { code: code as Json }),
    ...(source ? { source } : {}),
    ...(tags ? { tags } : {}),
    ...(related.length ? { relatedInformation: related } : {}),
  };
}

/** CallHierarchyItem / TypeHierarchyItem -> LSP item carrying an opaque `data` handle. */
export function toHierarchyItem(value: unknown, handle: string): Json {
  const item = value as Obj;
  const tags = toTags(item.tags);
  const detail = text(item.detail);
  return {
    name: text(item.name),
    kind: (item.kind as number) + 1,
    ...(tags ? { tags } : {}),
    ...(detail ? { detail } : {}),
    uri: toUri(item.uri),
    range: toRange(item.range),
    selectionRange: toRange(item.selectionRange),
    data: handle,
  };
}
