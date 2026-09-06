import { createHash } from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import type {
  LspGetSemanticsQuery,
  LspSemanticEnvelope,
} from '../shared/semanticTypes.js';

function canonical(value: unknown): string {
  return JSON.stringify(value, (_key, item: unknown) => {
    if (!item || typeof item !== 'object' || Array.isArray(item)) return item;
    return Object.fromEntries(
      Object.entries(item).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    );
  });
}

function localPath(value: string | undefined): string | undefined {
  if (!value) return undefined;
  return path.resolve(value.startsWith('file:') ? fileURLToPath(value) : value);
}

function queryIdentity(query: LspGetSemanticsQuery) {
  const anchored = query as LspGetSemanticsQuery & {
    symbolName?: string;
    lineHint?: number;
    orderHint?: number;
    depth?: number;
    includeDeclaration?: boolean;
    groupByFile?: boolean;
  };
  // Page, pageSize, presentation, and caller metadata do not change the set.
  return {
    type: query.type,
    uri: localPath(query.uri),
    workspaceRoot: localPath(query.workspaceRoot) ?? process.cwd(),
    symbolName: anchored.symbolName,
    lineHint: anchored.lineHint,
    orderHint: anchored.orderHint ?? 0,
    depth: anchored.depth ?? 1,
    includeDeclaration: anchored.includeDeclaration ?? true,
    groupByFile: anchored.groupByFile ?? false,
    contextLines: query.contextLines ?? 0,
  };
}

function ordering(value: unknown): [string, number, number, string] {
  type OrderedItem = {
    uri?: string;
    path?: string;
    name?: string;
    line?: number;
    character?: number;
    firstLine?: number;
    firstCharacter?: number;
    range?: { start?: { line?: number; character?: number } };
    displayRange?: { startLine?: number };
  };
  const row = value as
    | (OrderedItem & {
        item?: OrderedItem;
        from?: OrderedItem;
        to?: OrderedItem;
      })
    | null;
  const item = row?.item ?? row?.from ?? row?.to ?? row;
  return [
    String(item?.uri ?? item?.path ?? ''),
    Number(
      item?.range?.start?.line ??
        item?.displayRange?.startLine ??
        item?.firstLine ??
        item?.line ??
        0
    ),
    Number(
      item?.range?.start?.character ??
        item?.firstCharacter ??
        item?.character ??
        0
    ),
    String(item?.name ?? ''),
  ];
}

/** Content-addressed guard, independent of process-local caches or server order. */
export function semanticSnapshotItems<T>(
  items: readonly T[],
  query: LspGetSemanticsQuery,
  identities: readonly unknown[] = items
): { items: T[]; snapshot: string } {
  const rows = items.map((item, index) => ({
    item,
    identity: canonical(identities[index]),
    order: ordering(item),
  }));
  rows.sort((a, b) => {
    for (let index = 0; index < a.order.length; index++) {
      const left = a.order[index]!;
      const right = b.order[index]!;
      if (left < right) return -1;
      if (left > right) return 1;
    }
    return a.identity < b.identity ? -1 : a.identity > b.identity ? 1 : 0;
  });
  const digest = createHash('sha256')
    .update(canonical([queryIdentity(query), rows.map(row => row.identity)]))
    .digest('hex');
  return { items: rows.map(row => row.item), snapshot: `lsp-v1:${digest}` };
}

export function guardSemanticSnapshot(
  query: LspGetSemanticsQuery,
  result: LspSemanticEnvelope | Record<string, unknown>
): LspSemanticEnvelope | Record<string, unknown> {
  if (
    !result.payload ||
    typeof result.payload !== 'object' ||
    ('status' in result && result.status === 'error')
  )
    return result;
  const envelope = result as LspSemanticEnvelope;
  const pagination = envelope.pagination as { snapshot?: string } | undefined;
  const required =
    (query.page ?? 1) > 1 && !query.snapshot && Boolean(pagination);
  if (!required && (!query.snapshot || query.snapshot === pagination?.snapshot))
    return result;
  const { snapshot: _snapshot, ...restart } = query;
  return {
    type: envelope.type,
    uri: envelope.uri,
    lsp: envelope.lsp,
    ...(envelope.workspaceRoot
      ? { workspaceRoot: envelope.workspaceRoot }
      : {}),
    status: 'empty',
    confidence: 'low',
    payload: {
      kind: 'empty',
      category: required ? 'paginationSnapshotRequired' : 'paginationChanged',
      reason: required
        ? 'Later pages require the snapshot from next.nextPage. Restart at page 1.'
        : 'The semantic result set or query changed. Discard earlier pages and restart at page 1; no rows from a different snapshot were returned.',
    },
    snapshot: {
      ...(query.snapshot ? { expected: query.snapshot } : {}),
      ...(pagination?.snapshot ? { actual: pagination.snapshot } : {}),
    },
    next: {
      restartPagination: {
        tool: 'lspGetSemantics',
        query: { ...restart, page: 1 },
        why: 'Discard earlier pages and restart semantic pagination from the current result set.',
        confidence: 'exact',
      },
    },
  };
}
