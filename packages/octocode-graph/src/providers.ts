import { posix, win32 } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Relation } from './relations.ts';
import { list, record, string } from './json-fields.ts';
import {
  BUNDLE_KINDS,
  readEsbuildMetafile,
  readRolldownAnalyze,
  readRollupBundle,
  readWebpackStats,
} from './runtime/bundle-ingest.ts';
import { readCpuProfile, readHeapProfile } from './runtime/profile-ingest.ts';

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
  /** Bundles: the directory the bundler resolved relative paths from, on the machine that ran it (default sourceRoot). */
  workingDirectory?: string;
  /** Profiles: the export being enriched (its files and symbols) and its root on this machine, where source maps of
   * bundled frames are read. */
  graph?: Record<string, unknown>;
  localRoot?: string;
  /** The revision the manifest attests: a provider that names its own revision must name this one. */
  revision?: string;
}

/** What an adapter returns: relations, plus fields for its provider record (counts, what could not be mapped). */
export type Parsed =
  Observation[] | { relations: Observation[]; record: Record<string, unknown> };

export interface Adapter {
  read(data: unknown, opts: ProviderOptions): Parsed;
  kinds?: string[];
  kindsFor?(data: unknown): string[];
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

// ---- data flow: CodeQL path results and Joern flows as FLOWS_TO relations ----

/** One step of a flow: where it is (a graph path, 1-based line and column) and what the analyzer said of it. */
interface FlowStep {
  path: string;
  line: number | null;
  column: number | null;
  endLine?: number | null;
  endColumn?: number | null;
  message?: string;
  [field: string]: unknown;
}

// A step's node: its file, line and column (`src/db.js#L9:15`), so two flows meet only at the same program point; a
// step with no region is its file.
const stepId = (s: FlowStep) =>
  s.line === null
    ? s.path
    : `${s.path}#L${s.line}${s.column === null ? '' : `:${s.column}`}`;

// One flow as relations: the file holding the source starts it (so `trace <file> FLOWS_TO+` begins there), then each
// step flows to the next. A relation cites the step it arrives at.
function flowRelations(
  steps: FlowStep[],
  details: Record<string, unknown>,
  artifact: string
): Observation[] {
  const out: Observation[] = [];
  const at = (s: FlowStep) => ({
    path: s.path,
    lines: s.line === null ? [] : [s.line],
  });
  steps.forEach((step, k) => {
    const role = k === 0 ? 'source' : k === steps.length - 1 ? 'sink' : 'step';
    const from = k === 0 ? null : steps[k - 1];
    if (k === 0 && stepId(step) === step.path) return;
    out.push({
      relation: 'FLOWS_TO',
      source: from ? stepId(from) : step.path,
      target: stepId(step),
      ...at(step),
      artifact,
      details: {
        ...details,
        step: k,
        steps: steps.length,
        role,
        ...(from && { from }),
        to: step,
      },
    });
  });
  return out;
}

const isObject = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const positive = (value: unknown, field: string) => {
  if (value === undefined || value === null) return null;
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 1)
    throw new Error(`Expected a positive integer for ${field}`);
  return value;
};

// The revision a SARIF run analyzed must be the one the manifest attests (a commit and its abbreviation agree).
function sameRevision(declared: string, attested: string) {
  if (declared === attested) return true;
  const sha = /^[0-9a-f]{7,64}$/i;
  return (
    sha.test(declared) &&
    sha.test(attested) &&
    (declared.toLowerCase().startsWith(attested.toLowerCase()) ||
      attested.toLowerCase().startsWith(declared.toLowerCase()))
  );
}

// SARIF artifact locations → graph paths. A relative `uri` is relative to its `uriBaseId` (CodeQL: %SRCROOT%, the
// analyzed checkout), which `sourceRoot` names on this machine; an absolute file URI under a base the run declares in
// `originalUriBaseIds` is made relative to that base first. Paths are graph ids: relative to the graph's root.
function sarifPaths(run: Record<string, unknown>, opts: ProviderOptions) {
  const api = /^[A-Za-z]:[\\/]|^\\\\/.test(opts.sourceRoot) ? win32 : posix;
  const graphRoot = opts.localRoot ?? opts.sourceRoot;
  const bases = new Map<string, string>();
  for (const [id, value] of Object.entries(
    record(run.originalUriBaseIds ?? {})
  )) {
    const uri = record(value).uri;
    if (typeof uri === 'string' && uri.startsWith('file:'))
      bases.set(id, fileURLToPath(uri));
  }
  const artifacts = list(run.artifacts ?? [], 'artifacts').map(record);
  const toPath = (raw: string) => {
    const path = raw.startsWith('file:')
      ? fileURLToPath(raw)
      : decodeURIComponent(raw);
    if (api.isAbsolute(path)) {
      for (const base of bases.values()) {
        const rel = api.relative(base, path);
        if (!rel.startsWith('..') && !api.isAbsolute(rel))
          return api.resolve(opts.sourceRoot, rel);
      }
      return path;
    }
    return api.resolve(opts.sourceRoot, path);
  };
  return (location: unknown, field: string) => {
    let artifact = record(location);
    if (artifact.uri === undefined && artifact.index !== undefined) {
      const known = artifacts[natural(artifact.index, `${field} index`)];
      if (!known)
        throw new Error(`No artifact ${JSON.stringify(artifact.index)}`);
      artifact = record(known.location);
    }
    const abs = toPath(string(artifact.uri, `${field} uri`));
    return api.relative(graphRoot, abs).split(api.sep).join('/') || '.';
  };
}

function sarifStep(
  location: unknown,
  pathOf: ReturnType<typeof sarifPaths>,
  field: string
): FlowStep {
  const loc = record(location);
  const physical = record(loc.physicalLocation);
  const region = record(physical.region ?? {});
  const message =
    loc.message === undefined ? undefined : record(loc.message).text;
  return {
    path: pathOf(physical.artifactLocation, `${field} artifactLocation`),
    line: positive(region.startLine, `${field} startLine`),
    column: positive(region.startColumn, `${field} startColumn`),
    endLine: positive(region.endLine, `${field} endLine`),
    endColumn: positive(region.endColumn, `${field} endColumn`),
    ...(typeof message === 'string' && { message }),
  };
}

/**
 * A SARIF 2.1.0 log with path results (CodeQL `@kind path-problem`): every thread flow of every result's
 * `codeFlows` becomes FLOWS_TO relations, step to step, carrying the rule id, the result's message and each step's
 * location and message. Results without a code flow (plain problems) have no path and are only counted.
 */
function codeqlSarif(data: Record<string, unknown>, opts: ProviderOptions) {
  if (data.version !== '2.1.0') throw new Error('Expected SARIF version 2.1.0');
  const relations: Observation[] = [];
  let results = 0,
    flows = 0,
    withoutFlow = 0;
  const tools = new Set<string>();
  list(data.runs, 'runs').forEach((value, r) => {
    const run = record(value);
    for (const item of list(
      run.versionControlProvenance ?? [],
      'versionControlProvenance'
    )) {
      const declared = record(item).revisionId;
      if (
        typeof declared === 'string' &&
        opts.revision &&
        !sameRevision(declared, opts.revision)
      )
        throw new Error(
          `SARIF run ${r} analyzed revision ${declared}, the manifest attests ${opts.revision}`
        );
    }
    const driver = record(record(run.tool).driver);
    const tool = string(driver.name, 'tool.driver.name');
    tools.add(
      typeof driver.semanticVersion === 'string'
        ? `${tool} ${driver.semanticVersion}`
        : tool
    );
    const rules = list(driver.rules ?? [], 'rules').map(record);
    const pathOf = sarifPaths(run, opts);
    list(run.results ?? [], 'results').forEach((item, i) => {
      const result = record(item);
      results++;
      // CodeQL writes `ruleId`; SARIF also allows only a `rule` reference or an index into the driver's rules.
      const rule = [
        result.ruleId,
        isObject(result.rule) ? result.rule.id : undefined,
        typeof result.ruleIndex === 'number'
          ? rules[result.ruleIndex]?.id
          : undefined,
      ].find((x): x is string => typeof x === 'string');
      const codeFlows = list(result.codeFlows ?? [], 'codeFlows');
      if (!codeFlows.length) {
        withoutFlow++;
        return;
      }
      const message =
        result.message === undefined ? undefined : record(result.message).text;
      codeFlows.forEach((cf, c) =>
        list(record(cf).threadFlows, 'threadFlows').forEach((tf, t) => {
          const field = `run ${r} result ${i} codeFlow ${c} threadFlow ${t}`;
          const steps = list(record(tf).locations, `${field} locations`).map(
            (step, k) =>
              sarifStep(record(step).location, pathOf, `${field} step ${k}`)
          );
          if (!steps.length) return;
          flows++;
          relations.push(
            ...flowRelations(
              steps,
              {
                tool,
                ...(rule && { ruleId: rule }),
                ...(typeof result.level === 'string' && {
                  level: result.level,
                }),
                ...(typeof message === 'string' && { message }),
                flow: `${r}/${i}/${c}/${t}`,
              },
              opts.artifact
            )
          );
        })
      );
    });
  });
  return {
    relations,
    record: { tools: [...tools], results, flows, withoutFlow },
  };
}

/**
 * Joern data flows (`reachableByFlows`) exported as JSON: a list of flows (or `{ flows }`), each `{ elements: [...] }`
 * or a bare list of elements, an element carrying the columns Joern's flow table prints: `nodeType`, `tracked` (or
 * `code`), `lineNumber`, `method` and `file` (or `filename`), with an optional `columnNumber`. A flow may name its
 * query as `name` or `rule`.
 */
function joernFlows(data: unknown, opts: ProviderOptions) {
  const flowsIn = Array.isArray(data)
    ? data
    : list(record(data).flows, 'flows');
  const graphRoot = opts.localRoot ?? opts.sourceRoot;
  const api = /^[A-Za-z]:[\\/]|^\\\\/.test(opts.sourceRoot) ? win32 : posix;
  const relations: Observation[] = [];
  flowsIn.forEach((value, f) => {
    const flow = Array.isArray(value) ? { elements: value } : record(value);
    const rule = [flow.rule, flow.name].find(x => typeof x === 'string');
    const steps = list(flow.elements, `flow ${f} elements`).map(
      (item, k): FlowStep => {
        const e = record(item);
        const field = `flow ${f} element ${k}`;
        const file = string(e.file ?? e.filename, `${field} file`);
        const abs = api.resolve(opts.sourceRoot, file);
        const code = e.tracked ?? e.code;
        const nodeType = e.nodeType ?? e['_label'] ?? e.label;
        const method = e.method ?? e.methodFullName;
        return {
          path: api.relative(graphRoot, abs).split(api.sep).join('/') || '.',
          line: positive(e.lineNumber, `${field} lineNumber`),
          column: positive(e.columnNumber, `${field} columnNumber`),
          ...(typeof code === 'string' && { message: code }),
          ...(typeof nodeType === 'string' && { nodeType }),
          ...(typeof method === 'string' && { method }),
        };
      }
    );
    if (steps.length)
      relations.push(
        ...flowRelations(
          steps,
          { tool: 'joern', ...(rule && { ruleId: rule }), flow: String(f) },
          opts.artifact
        )
      );
  });
  return { relations, record: { flows: flowsIn.length } };
}

export const adapters: Record<string, Adapter> = {
  'llvm-graphviz': {
    read: (data, opts) => llvmGraph(record(data), opts),
    kindsFor: data => [
      'DEFINES',
      'CONTAINS',
      llvmHeader(record(data)).relation,
    ],
  },
  'scip-json': {
    read: (data, opts) => scip(record(data), opts),
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
  'llvm-cov': {
    read: (data, opts) => coverage(record(data), opts),
    kinds: ['COVERS'],
  },
  'otlp-json': {
    read: (data, opts) => spans(record(data), opts),
    kinds: ['OBSERVED', 'EXECUTES_FILE', 'PARENT_OF'],
  },
  'esbuild-metafile': {
    read: (data, opts) => readEsbuildMetafile(record(data), opts),
    kinds: BUNDLE_KINDS,
  },
  'rollup-bundle': {
    read: (data, opts) =>
      readRollupBundle(Array.isArray(data) ? data : record(data), opts),
    kinds: BUNDLE_KINDS,
  },
  'rolldown-analyze': {
    read: (data, opts) => readRolldownAnalyze(record(data), opts),
    kinds: BUNDLE_KINDS,
  },
  'webpack-stats': {
    read: (data, opts) => readWebpackStats(record(data), opts),
    kinds: BUNDLE_KINDS,
  },
  cpuprofile: {
    read: (data, opts) => readCpuProfile(record(data), opts),
    kinds: ['SAMPLED'],
  },
  heapprofile: {
    read: (data, opts) => readHeapProfile(record(data), opts),
    kinds: ['SAMPLED'],
  },
  'codeql-sarif': {
    read: (data, opts) => codeqlSarif(record(data), opts),
    kinds: ['FLOWS_TO'],
  },
  'joern-flows': {
    read: (data, opts) => joernFlows(data, opts),
    kinds: ['FLOWS_TO'],
  },
};
