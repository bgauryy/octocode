import { posix, win32 } from 'node:path';
import type { Relation } from './relations.ts';

export interface Observation extends Relation {
  artifact: string;
  details?: unknown;
}
export interface ProviderOptions {
  artifact: string;
  sourceRoot: string;
  workload?: string;
  testId?: string;
  moduleSha256?: string;
}

export function record(value: unknown): Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value))
    throw new Error('Expected a JSON object');
  return value as Record<string, unknown>;
}
export function list(value: unknown, field: string): unknown[] {
  if (!Array.isArray(value)) throw new Error(`Expected an array for ${field}`);
  return value;
}
export function string(value: unknown, field: string): string {
  if (typeof value !== 'string' || !value.length)
    throw new Error(`Expected a nonempty string for ${field}`);
  return value;
}
function natural(value: unknown, field: string): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0)
    throw new Error(`Expected a nonnegative integer for ${field}`);
  return value;
}
function sourcePath(value: unknown, root: string) {
  const path = string(value, 'source path');
  const api = /^[A-Za-z]:[\\/]|^\\\\/.test(root) ? win32 : posix;
  return (api.isAbsolute(path) ? api.relative(root, path) : api.normalize(path))
    .split(api.sep)
    .join('/');
}

const scipId = (path: string, symbol: string) =>
  `scip:${JSON.stringify(symbol.startsWith('local ') ? [path, symbol] : [symbol])}`;

function occurrenceLine(occ: Record<string, unknown>) {
  const single = occ.singleLineRange ?? occ.single_line_range;
  if (single != null) return natural(record(single).line ?? 0, 'line') + 1;
  const multi = occ.multiLineRange ?? occ.multi_line_range;
  if (multi != null) {
    const range = record(multi);
    return natural(range.startLine ?? range.start_line ?? 0, 'start line') + 1;
  }
  const range = list(occ.range, 'range');
  if (![3, 4].includes(range.length))
    throw new Error('Invalid SCIP occurrence range');
  return natural(range[0], 'start line') + 1;
}

function scip(data: Record<string, unknown>, opts: ProviderOptions) {
  const edges: Observation[] = [];
  for (const entry of list(data.documents, 'documents')) {
    const doc = record(entry);
    const path = sourcePath(
      doc.relativePath ?? doc.relative_path,
      opts.sourceRoot
    );
    for (const raw of list(doc.occurrences ?? [], 'occurrences')) {
      const occ = record(raw);
      if (!occ.symbol) continue; // Pure syntax highlighting has no symbol identity.
      const target = scipId(path, string(occ.symbol, 'symbol'));
      const roles = natural(
        occ.symbolRoles ?? occ.symbol_roles ?? 0,
        'symbol roles'
      );
      const line = occurrenceLine(occ);
      const kinds = [roles & 1 ? 'DEFINES' : 'REFERENCES'];
      if (roles & 4) kinds.push('WRITES');
      if (roles & 8) kinds.push('READS');
      for (const relation of kinds)
        edges.push({
          relation,
          source: path,
          target,
          path,
          lines: [line],
          test: Boolean(roles & 32),
          artifact: opts.artifact,
          details: {
            roles,
            range:
              occ.singleLineRange ??
              occ.single_line_range ??
              occ.multiLineRange ??
              occ.multi_line_range ??
              occ.range,
            encoding: doc.positionEncoding ?? doc.position_encoding,
          },
        });
    }
    for (const symbolInfo of list(doc.symbols ?? [], 'symbols')) {
      const symbol = record(symbolInfo);
      for (const item of list(symbol.relationships ?? [], 'relationships')) {
        const rel = record(item);
        const flags = [
          ['IMPLEMENTS', rel.isImplementation ?? rel.is_implementation],
          ['REFERENCES', rel.isReference ?? rel.is_reference],
          ['TYPE_DEFINITION', rel.isTypeDefinition ?? rel.is_type_definition],
          ['DEFINITION', rel.isDefinition ?? rel.is_definition],
        ] as const;
        for (const [relation, enabled] of flags)
          if (enabled === true)
            edges.push({
              relation,
              source: scipId(path, string(symbol.symbol, 'symbol')),
              target: scipId(path, string(rel.symbol, 'symbol')),
              path,
              lines: [],
              artifact: opts.artifact,
            });
      }
    }
  }
  return edges;
}

function coverage(data: Record<string, unknown>, opts: ProviderOptions) {
  if (data.type !== 'llvm.coverage.json.export')
    throw new Error('Expected llvm-cov export JSON');
  const source = `test:${string(opts.testId, 'testId')}`;
  const edges: Observation[] = [];
  for (const unit of list(data.data, 'coverage data'))
    for (const entry of list(record(unit).files, 'coverage files')) {
      const file = record(entry);
      const target = sourcePath(file.filename, opts.sourceRoot);
      const segments = list(file.segments, 'coverage segments').map(row =>
        list(row, 'segment')
      );
      const active = segments.filter(
        row =>
          row[3] === true &&
          row[5] !== true &&
          natural(row[2], 'execution count') > 0
      );
      if (active.length)
        edges.push({
          relation: 'COVERS',
          source,
          target,
          path: target,
          lines: [...new Set(active.map(row => natural(row[0], 'line')))],
          artifact: opts.artifact,
          details: { segments: active, summary: file.summary },
        });
    }
  return edges;
}

function nanoseconds(value: unknown) {
  if (typeof value !== 'string' || !/^\d+$/.test(value))
    throw new Error('Expected a decimal nanosecond timestamp');
  return BigInt(value);
}
function spanId(trace: string, id: unknown) {
  const value = string(id, 'spanId');
  if (!/^[\da-f]{16}$/i.test(value) || /^0+$/.test(value))
    throw new Error('Invalid OTLP span ID');
  return `span:${trace.toLowerCase()}/${value.toLowerCase()}`;
}
function spans(data: Record<string, unknown>, opts: ProviderOptions) {
  const edges: Observation[] = [];
  const workload = `workload:${string(opts.workload, 'workload')}`;
  for (const resource of list(data.resourceSpans, 'resourceSpans'))
    for (const scope of list(record(resource).scopeSpans ?? [], 'scopeSpans')) {
      for (const entry of list(record(scope).spans ?? [], 'spans')) {
        const span = record(entry);
        const trace = string(span.traceId, 'traceId');
        if (!/^[\da-f]{32}$/i.test(trace) || /^0+$/.test(trace))
          throw new Error('Invalid OTLP trace ID');
        const id = spanId(trace, span.spanId);
        const elapsed =
          nanoseconds(span.endTimeUnixNano) -
          nanoseconds(span.startTimeUnixNano);
        if (elapsed < 0n) throw new Error('Span ended before it started');
        const attrs = new Map(
          list(span.attributes ?? [], 'attributes').map(item => {
            const a = record(item);
            return [a.key, record(a.value)] as const;
          })
        );
        const file =
          attrs.get('code.file.path')?.stringValue ??
          attrs.get('code.filepath')?.stringValue;
        const path = file ? sourcePath(file, opts.sourceRoot) : opts.artifact;
        const loc =
          attrs.get('code.line.number')?.intValue ??
          attrs.get('code.lineno')?.intValue;
        const lines = loc == null ? [] : [natural(Number(loc), 'source line')];
        const details = {
          name: span.name,
          durationNs: elapsed.toString(),
          startTimeUnixNano: span.startTimeUnixNano,
          endTimeUnixNano: span.endTimeUnixNano,
        };
        const base = { path, lines, artifact: opts.artifact, details };
        edges.push({
          ...base,
          relation: 'OBSERVED',
          source: workload,
          target: id,
        });
        if (file)
          edges.push({
            ...base,
            relation: 'EXECUTES_FILE',
            source: id,
            target: path,
          });
        if (span.parentSpanId && span.parentSpanId !== '0000000000000000')
          edges.push({
            ...base,
            relation: 'PARENT_OF',
            source: spanId(trace, span.parentSpanId),
            target: id,
          });
      }
    }
  return edges;
}

function llvmHeader(data: Record<string, unknown>) {
  const title = string(data.name, 'LLVM graph title');
  const match = /^(CFG|Dominator tree) for '(.*)' function$/s.exec(title);
  if (!match)
    throw new Error('Expected LLVM dot-cfg-only or dot-dom-only output');
  const relation = match[1] === 'CFG' ? 'BRANCHES_TO' : 'IMMEDIATELY_DOMINATES';
  return { name: match[2], relation };
}

// LLVM's CFG-only and dominator-only printers use different process-local DOT node
// addresses. A block label is unique inside a function in the same exact IR module.
function llvmGraph(data: Record<string, unknown>, opts: ProviderOptions) {
  if (!opts.moduleSha256)
    throw new Error(
      'LLVM graphs require a verified moduleFile and moduleSha256'
    );
  if (data.directed !== true) throw new Error('Expected a directed LLVM graph');
  const { name, relation } = llvmHeader(data);
  const module = `llvm:${opts.moduleSha256}`,
    fn = `${module}/${encodeURIComponent(name)}`;
  const base = { path: opts.artifact, lines: [], artifact: opts.artifact };
  const edges: Observation[] = [
    {
      ...base,
      source: module,
      target: fn,
      relation: 'DEFINES',
      details: { function: name },
    },
  ];
  const ids = new Map<number, string>(),
    unique = new Set<string>();
  for (const obj of list(data.objects, 'LLVM blocks')) {
    const node = record(obj);
    const { _gvid: graphId } = node;
    const gvid = natural(graphId, 'graph node ID');
    const label = string(node.label, 'LLVM block label');
    // The first field is the IR block name. Reject unsupported record syntax and
    // instruction listings instead of conflating two block names during a join.
    const field = /^\{((?:\\[{}|<>\\ ]|[^{}|\\])+)(?:\|\{.*\})?\}$/s.exec(
      label
    );
    if (!field || node.shape !== 'record')
      throw new Error(`Unsupported LLVM block label: ${label}`);
    const block = field[1].replace(/\\([{}|<>\\ ])/g, '$1');
    const id = `${fn}/${encodeURIComponent(block)}`;
    if (unique.has(id) || ids.has(gvid))
      throw new Error(`Duplicate LLVM block: ${block}`);
    ids.set(gvid, id);
    unique.add(id);
    edges.push({
      ...base,
      relation: 'CONTAINS',
      source: fn,
      target: id,
      details: { block, label },
    });
  }
  for (const obj of list(data.edges ?? [], 'LLVM edges')) {
    const edge = record(obj);
    const { _gvid: edgeId } = edge;
    const source = ids.get(natural(edge.tail, 'tail')),
      target = ids.get(natural(edge.head, 'head'));
    if (!source || !target)
      throw new Error('LLVM edge references an unknown block');
    edges.push({
      ...base,
      source,
      target,
      relation,
      details: {
        edge: edgeId,
        tailport: edge.tailport,
        headport: edge.headport,
      },
    });
  }
  return edges;
}

export const adapters = {
  'llvm-graphviz': {
    read: llvmGraph,
    kindsFor: (data: Record<string, unknown>) => [
      'DEFINES',
      'CONTAINS',
      llvmHeader(data).relation,
    ],
  },
  'scip-json': {
    read: scip,
    kinds: [
      'DEFINES',
      'REFERENCES',
      'READS',
      'WRITES',
      'IMPLEMENTS',
      'TYPE_DEFINITION',
      'DEFINITION',
    ],
  },
  'llvm-cov': { read: coverage, kinds: ['COVERS'] },
  'otlp-json': {
    read: spans,
    kinds: ['OBSERVED', 'EXECUTES_FILE', 'PARENT_OF'],
  },
};
