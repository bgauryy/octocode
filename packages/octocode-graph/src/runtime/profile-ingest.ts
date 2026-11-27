// V8 CPU (.cpuprofile) and sampling heap (.heapprofile) profiles as provider relations (`octocode-graph enrich`).
// Each frame maps to a graph file and line: directly for a graph file, through the bundle's source map
// (sourcemap.ts) for a bundled one; then to the innermost graph symbol whose lines hold it. Cost is microseconds
// (each sample lasts until the next one) or sampled bytes:
//   SAMPLED  profile:<artifact> → <file>          { level: file, self, total, unit }        lines: hottest function
//   SAMPLED  profile:<artifact> → <file#symbol>   { level: function, self, total, unit, name, symbol, kind }
// A frame outside any symbol keys as `<file>:<line>`, a file's module scope as `<file>:(module)`. Total counts a file
// or function once per stack, so recursion is not counted twice. Cost no graph file holds stays in the provider
// record by reason, package and file.
import { readFileSync } from 'node:fs';
import { posix, resolve, win32 } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Observation, ProviderOptions } from '../providers.ts';
import { list, record, string } from '../json-fields.ts';
import {
  enclosingSymbol,
  graphPathOf,
  originalPosition,
  readSourceMap,
  type FoundMap,
  type SymbolLike,
} from './sourcemap.ts';

interface Frame {
  functionName: string;
  url: string;
  lineNumber: number;
  columnNumber: number;
}
interface ProfileNode {
  id: number;
  frame: Frame;
  self: number;
  children: number[];
}
interface Place {
  file: string;
  line: number;
  key: string;
  name: string;
  symbol: SymbolLike | null;
}
type Where = { place: Place } | { reason: string; file?: string; pkg?: string };

// V8's own entries: no script, no function of the program.
const META = new Set(['(root)', '(program)', '(idle)', '(garbage collector)']);

const number = (value: unknown, field: string) => {
  if (typeof value !== 'number' || !Number.isFinite(value))
    throw new Error(`Expected a number for ${field}`);
  return value;
};

function frameOf(value: unknown): Frame {
  const f = record(value);
  return {
    functionName: typeof f.functionName === 'string' ? f.functionName : '',
    url: typeof f.url === 'string' ? f.url : '',
    lineNumber: typeof f.lineNumber === 'number' ? f.lineNumber : -1,
    columnNumber: typeof f.columnNumber === 'number' ? f.columnNumber : -1,
  };
}

const packageName = (path: string) => {
  const parts = path
    .slice(path.lastIndexOf('node_modules/') + 'node_modules/'.length)
    .split('/');
  return `pkg:${parts[0].startsWith('@') ? parts.slice(0, 2).join('/') : parts[0]}`;
};

// Frame → graph place, or why none: one answer per script position, source maps read once per bundle.
function placer(opts: ProviderOptions) {
  const graph = opts.graph ?? {};
  const files = new Set(
    list(graph.files ?? [], 'graph files').map(f =>
      string(record(f).id, 'file id')
    )
  );
  const byFile = new Map<string, SymbolLike[]>();
  for (const item of list(graph.symbols ?? [], 'graph symbols')) {
    const s = record(item) as unknown as SymbolLike;
    const file = s.file ?? s.id.split('#')[0];
    byFile.set(file, [...(byFile.get(file) ?? []), s]);
  }
  const root = opts.sourceRoot;
  const local = opts.localRoot ?? root;
  const api = /^[A-Za-z]:[\\/]|^\\\\/.test(root) ? win32 : posix;
  const maps = new Map<string, FoundMap>();
  const used = new Map<
    string,
    { bundle: string; map: string; sha256: string; frames: number }
  >();
  const answers = new Map<string, Where>();

  const place = (file: string, line: number, frameName: string): Where => {
    const symbol = enclosingSymbol(byFile.get(file) ?? [], file, line);
    return {
      place: {
        file,
        line: symbol?.line ?? line,
        key: symbol?.id ?? `${file}:${line}`,
        name: symbol?.name ?? (frameName || '(anonymous)'),
        symbol,
      },
    };
  };
  const classify = (rel: string, display: string): Where | null => {
    if (rel.includes('node_modules/'))
      return { reason: 'node_modules', pkg: packageName(rel) };
    if (rel === '..' || rel.startsWith('../'))
      return { reason: 'outside root', file: display };
    return null;
  };
  const resolveFrame = (f: Frame): Where => {
    if (!f.url)
      return { reason: META.has(f.functionName) ? f.functionName : 'native' };
    if (f.url.startsWith('node:')) return { reason: 'node internals' };
    let abs: string;
    if (f.url.startsWith('file:')) abs = fileURLToPath(f.url);
    else if (api.isAbsolute(f.url)) abs = f.url;
    else return { reason: 'other url', file: f.url };
    const rel = api.relative(root, abs).split(api.sep).join('/');
    const outside = api.isAbsolute(rel) ? `../${rel}` : rel;
    const early = classify(outside, abs);
    if (early) return early;
    const line = f.lineNumber + 1;
    // V8 reports a script's module scope as a nameless function at 0:0: code of the file, of no function in it.
    const scope = !f.functionName && f.lineNumber === 0 && f.columnNumber === 0;
    if (files.has(rel))
      return scope
        ? {
            place: {
              file: rel,
              line: 1,
              key: `${rel}:(module)`,
              name: '(module scope)',
              symbol: null,
            },
          }
        : place(rel, line, f.functionName);
    // A file the graph does not hold may be a bundle of files it does: map the frame through its source map.
    let found = maps.get(rel);
    if (!found) {
      const path = resolve(local, rel);
      let text: string | null = null;
      try {
        text = readFileSync(path, 'utf8');
      } catch {
        // Not on this machine: nothing to map through.
      }
      found =
        text === null
          ? { ok: false, reason: 'unreadable' }
          : readSourceMap(path, text);
      maps.set(rel, found);
    }
    if (!found.ok) return { reason: 'not in graph', file: rel };
    // A bundle's module scope runs the top-level code of every source in it: no one source to charge.
    if (scope) return { reason: 'bundle module scope', file: rel };
    const o = originalPosition(found.map, line, f.columnNumber + 1);
    if (!o?.source) return { reason: 'unmapped position', file: rel };
    const map = used.get(rel) ?? {
      bundle: rel,
      map: found.mapFile,
      sha256: found.sha256,
      frames: 0,
    };
    map.frames++;
    used.set(rel, map);
    const src = graphPathOf(local, o.source);
    if (src === null) return { reason: 'outside root', file: o.source };
    const mapped = classify(src, o.source);
    if (mapped) return mapped;
    if (!files.has(src)) return { reason: 'not in graph', file: src };
    return place(src, o.line, f.functionName || (o.name ?? ''));
  };
  return {
    where(f: Frame) {
      const key = `${f.url}\0${f.lineNumber}\0${f.columnNumber}\0${f.functionName}`;
      let answer = answers.get(key);
      if (!answer) answers.set(key, (answer = resolveFrame(f)));
      return answer;
    },
    sourceMaps: () => [...used.values()],
  };
}

const add = (map: Map<string, number>, key: string, value: number) =>
  map.set(key, (map.get(key) ?? 0) + value);

// Self cost per node → self and once-per-stack total per file and function, plus every unmapped cost by reason.
function attribute(
  nodes: Map<number, ProfileNode>,
  unit: 'us' | 'bytes',
  opts: ProviderOptions,
  extra: Record<string, unknown>
) {
  const places = placer(opts);
  const children = new Set([...nodes.values()].flatMap(n => n.children));
  const roots = [...nodes.keys()].filter(id => !children.has(id));
  // Pre-order, refusing a child listed twice: the call tree must be a tree.
  const order: ProfileNode[] = [];
  const seen = new Set<number>();
  const stack = roots.toReversed();
  while (stack.length) {
    const id = stack.pop()!;
    const node = nodes.get(id);
    if (!node) throw new Error(`Profile node ${id} is listed but missing`);
    if (seen.has(id)) throw new Error(`Profile node ${id} has two parents`);
    seen.add(id);
    order.push(node);
    for (let k = node.children.length - 1; k >= 0; k--)
      stack.push(node.children[k]);
  }
  if (order.length !== nodes.size)
    throw new Error('Profile nodes form a cycle');
  const subtree = new Map<number, number>();
  for (const node of order.toReversed())
    subtree.set(
      node.id,
      node.children.reduce((a, c) => a + subtree.get(c)!, node.self)
    );

  const self = new Map<string, number>();
  const total = new Map<string, number>();
  const placed = new Map<string, Place>();
  const unmapped = new Map<string, number>();
  const packages = new Map<string, number>();
  const unmappedFiles = new Map<string, { reason: string; self: number }>();
  const keysOf = new Map<number, string[]>();
  for (const node of order) {
    const w = places.where(node.frame);
    if ('place' in w) {
      const fileKey = `file\0${w.place.file}`;
      const fnKey = `fn\0${w.place.key}`;
      placed.set(fnKey, w.place);
      keysOf.set(node.id, [fileKey, fnKey]);
      add(self, fileKey, node.self);
      add(self, fnKey, node.self);
    } else if (node.self) {
      add(unmapped, w.reason, node.self);
      if (w.pkg) add(packages, w.pkg, node.self);
      if (w.file) {
        const f = unmappedFiles.get(w.file) ?? { reason: w.reason, self: 0 };
        f.self += node.self;
        unmappedFiles.set(w.file, f);
      }
    }
  }
  // Total: the subtree under the outermost frame of each file or function on a stack.
  const active = new Map<string, number>();
  const walk: [ProfileNode, boolean][] = roots
    .toReversed()
    .map(id => [nodes.get(id)!, true]);
  while (walk.length) {
    const [node, entering] = walk.pop()!;
    const keys = keysOf.get(node.id) ?? [];
    if (!entering) {
      for (const k of keys) active.set(k, active.get(k)! - 1);
      continue;
    }
    for (const k of keys) {
      if (!active.get(k)) add(total, k, subtree.get(node.id)!);
      active.set(k, (active.get(k) ?? 0) + 1);
    }
    walk.push([node, false]);
    for (let k = node.children.length - 1; k >= 0; k--)
      walk.push([nodes.get(node.children[k])!, true]);
  }

  const relations: Observation[] = [];
  const source = `profile:${opts.artifact}`;
  const hottest = new Map<string, Place & { self: number }>();
  for (const [key, p] of placed) {
    const s = self.get(key) ?? 0;
    const best = hottest.get(p.file);
    if (!best || s > best.self) hottest.set(p.file, { ...p, self: s });
    if (!s && !total.get(key)) continue;
    relations.push({
      relation: 'SAMPLED',
      source,
      target: p.key,
      path: p.file,
      lines: [p.line],
      artifact: opts.artifact,
      details: {
        level: 'function',
        self: s,
        total: total.get(key) ?? 0,
        unit,
        name: p.name,
        symbol: p.symbol?.id ?? null,
        ...(p.symbol?.kind && { kind: p.symbol.kind }),
      },
    });
  }
  for (const [file, best] of hottest) {
    const key = `file\0${file}`;
    if (!self.get(key) && !total.get(key)) continue;
    relations.push({
      relation: 'SAMPLED',
      source,
      target: file,
      path: file,
      lines: [best.line],
      artifact: opts.artifact,
      details: {
        level: 'file',
        self: self.get(key) ?? 0,
        total: total.get(key) ?? 0,
        unit,
      },
    });
  }
  const sampled = order.reduce((a, n) => a + n.self, 0);
  const idle = unmapped.get('(idle)') ?? 0;
  const sorted = <V>(m: Map<string, V>) =>
    Object.fromEntries([...m].sort(([a], [b]) => (a < b ? -1 : 1)));
  return {
    relations,
    record: {
      unit,
      ...extra,
      sampled,
      idle,
      mapped: sampled - [...unmapped.values()].reduce((a, b) => a + b, 0),
      unmapped: sorted(unmapped),
      unmappedPackages: sorted(packages),
      unmappedFiles: sorted(unmappedFiles),
      sourceMaps: places.sourceMaps(),
    },
  };
}

/** A `.cpuprofile`: the call tree, and each sample's node and the time to the next sample (µs). */
export function readCpuProfile(
  data: Record<string, unknown>,
  opts: ProviderOptions
) {
  const nodes = new Map<number, ProfileNode>();
  for (const item of list(data.nodes, 'profile nodes')) {
    const n = record(item);
    const id = number(n.id, 'node id');
    nodes.set(id, {
      id,
      frame: frameOf(n.callFrame),
      self: 0,
      children: list(n.children ?? [], 'children').map(c =>
        number(c, 'child id')
      ),
    });
  }
  const start = number(data.startTime, 'startTime');
  const end = number(data.endTime, 'endTime');
  const samples = list(data.samples ?? [], 'samples');
  const deltas = list(data.timeDeltas ?? [], 'timeDeltas');
  let timing = 'timeDeltas';
  let clamped = 0;
  if (samples.length) {
    if (deltas.length !== samples.length)
      throw new Error('samples and timeDeltas differ in length');
    let at = start;
    const stamps = deltas.map(d => (at += number(d, 'timeDelta')));
    samples.forEach((s, i) => {
      const node = nodes.get(number(s, 'sample'));
      if (!node) throw new Error(`A sample names no node: ${String(s)}`);
      // A sample lasts until the next one; the last until the profile ends.
      let span =
        i + 1 < stamps.length ? stamps[i + 1] - stamps[i] : end - stamps[i];
      if (span < 0) {
        clamped++;
        span = 0;
      }
      node.self += span;
    });
  } else {
    // Older profiles carry hit counts only: the profile's span shared evenly among the hits.
    timing = 'hitCount';
    const raw = list(data.nodes, 'profile nodes').map(record);
    const hitsOf = (n: Record<string, unknown>) =>
      typeof n.hitCount === 'number' ? n.hitCount : 0;
    const hits = raw.reduce((a, n) => a + hitsOf(n), 0);
    for (const n of raw)
      nodes.get(number(n.id, 'node id'))!.self = hits
        ? (hitsOf(n) * (end - start)) / hits
        : 0;
  }
  return attribute(nodes, 'us', opts, {
    samples: samples.length,
    wall: end - start,
    timing,
    ...(clamped && { negativeIntervals: clamped }),
  });
}

/** A sampling heap profile: the allocation tree under `head` with each frame's sampled bytes. */
export function readHeapProfile(
  data: Record<string, unknown>,
  opts: ProviderOptions
) {
  const nodes = new Map<number, ProfileNode>();
  let next = 0;
  const visit = (value: unknown): number => {
    const n = record(value);
    const id = typeof n.id === 'number' ? n.id : --next;
    if (nodes.has(id)) throw new Error(`Heap profile node ${id} repeats`);
    const node: ProfileNode = {
      id,
      frame: frameOf(n.callFrame),
      self: number(n.selfSize ?? 0, 'selfSize'),
      children: [],
    };
    nodes.set(id, node);
    node.children = list(n.children ?? [], 'children').map(visit);
    return id;
  };
  visit(data.head);
  return attribute(nodes, 'bytes', opts, {
    samples: list(data.samples ?? [], 'samples').length,
  });
}
