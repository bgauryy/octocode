// Bundler output as provider relations (`octocode-graph enrich`): an esbuild metafile, a rollup or rolldown output
// bundle, rolldown's analyze-data.json and webpack's stats JSON each become the same relations, so `bundle`
// (bundle.ts) reads one shape. A build finds these files by itself (package-closure.ts, bundleFormat).
//   OUTPUTS         build:<artifact> → bundle:<output>   { bytes, entry, kind, measure }
//   BUNDLES         bundle:<output> → <input>            { bytes }   bytes of the input in that output
//   LOADS           bundle:<output> → bundle:<output>    { kind: static | dynamic }
//   INPUTS          build:<artifact> → <input>           { bytes }   the input's own size, as the bundler read it
//   BUNDLE_IMPORTS  <input> → <input>                    { kind, original? }   the bundler's module graph
//   EXTERNAL        bundle:<output> | <input> → external:<specifier>   left for the runtime to load
// Inputs are root-relative paths (graph file ids when the graph holds them), `node_modules/…` included; virtual
// modules keep their own name (`\0` written as `\\0`, `<stdin>`, `ns:path`).
import { posix, win32 } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Observation, ProviderOptions } from '../providers.ts';
import { list, record, string } from '../json-fields.ts';

export const BUNDLE_KINDS = [
  'OUTPUTS',
  'BUNDLES',
  'LOADS',
  'INPUTS',
  'BUNDLE_IMPORTS',
  'EXTERNAL',
];

type Measure = 'output' | 'module';

const count = (value: unknown, field: string) => {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0)
    throw new Error(`Expected a nonnegative number for ${field}`);
  return value;
};

// A module or output path the bundler wrote → root-relative, resolving relative paths from its working directory.
function idMaker(opts: ProviderOptions) {
  const root = opts.sourceRoot;
  const api = /^[A-Za-z]:[\\/]|^\\\\/.test(root) ? win32 : posix;
  const base = opts.workingDirectory ?? root;
  return (raw: string) => {
    if (raw.startsWith('\0')) return raw.replace(/\0/g, '\\0');
    if (/^<.*>$/s.test(raw) || raw.startsWith('(disabled):')) return raw;
    let path = raw;
    if (path.startsWith('file:')) path = fileURLToPath(path);
    else if (/^[a-z][\w+.-]*:/i.test(path) && !/^[a-z]:[\\/]/i.test(path))
      return raw; // A plugin namespace (`virtual:x`): no file to place.
    path = path.replace(/[?#].*$/s, ''); // `file.vue?type=script` is part of file.vue.
    const abs = api.isAbsolute(path) ? path : api.resolve(base, path);
    return api.relative(root, abs).split(api.sep).join('/') || '.';
  };
}

function builder(opts: ProviderOptions) {
  const relations: Observation[] = [];
  const add = (
    relation: string,
    source: string,
    target: string,
    details: Record<string, unknown>
  ) =>
    relations.push({
      relation,
      source,
      target,
      path: opts.artifact,
      lines: [],
      artifact: opts.artifact,
      details,
    });
  const build = `build:${opts.artifact}`;
  return {
    relations,
    output: (
      id: string,
      bytes: number | null,
      entry: string | null,
      kind: string,
      measure: Measure
    ) => add('OUTPUTS', build, `bundle:${id}`, { bytes, entry, kind, measure }),
    bundles: (output: string, input: string, bytes: number) =>
      add('BUNDLES', `bundle:${output}`, input, { bytes }),
    loads: (from: string, to: string, kind: 'static' | 'dynamic') =>
      add('LOADS', `bundle:${from}`, `bundle:${to}`, { kind }),
    input: (id: string, bytes: number) => add('INPUTS', build, id, { bytes }),
    imports: (from: string, to: string, kind: string, original?: unknown) =>
      add('BUNDLE_IMPORTS', from, to, {
        kind,
        ...(typeof original === 'string' && { original }),
      }),
    external: (from: string, specifier: string, kind: string) =>
      add('EXTERNAL', from, `external:${specifier}`, { kind }),
  };
}

const result = (
  b: ReturnType<typeof builder>,
  measure: Measure,
  outputs: number
) => ({
  relations: b.relations,
  record: { measure, outputs },
});

/** esbuild's `metafile`: inputs with their imports, outputs with bytes in output per input. */
export function readEsbuildMetafile(
  data: Record<string, unknown>,
  opts: ProviderOptions
) {
  const id = idMaker(opts);
  const b = builder(opts);
  for (const [path, value] of Object.entries(record(data.inputs))) {
    const input = record(value);
    const from = id(path);
    b.input(from, count(input.bytes, `bytes of ${path}`));
    for (const item of list(input.imports ?? [], `imports of ${path}`)) {
      const imp = record(item);
      const target = string(imp.path, 'import path');
      const kind = string(imp.kind, 'import kind');
      if (imp.external === true) b.external(from, target, kind);
      else b.imports(from, id(target), kind, imp.original);
    }
  }
  const outputs = Object.entries(record(data.outputs));
  const dynamic = new Set<string>();
  for (const [, value] of outputs)
    for (const item of list(record(value).imports ?? [], 'output imports')) {
      const imp = record(item);
      if (imp.kind === 'dynamic-import' && imp.external !== true)
        dynamic.add(id(string(imp.path, 'output import path')));
    }
  for (const [path, value] of outputs) {
    const output = record(value);
    const oid = id(path);
    const inputs = Object.entries(record(output.inputs ?? {}));
    const entry =
      typeof output.entryPoint === 'string' ? id(output.entryPoint) : null;
    const kind = entry
      ? dynamic.has(oid)
        ? 'dynamic-entry'
        : 'entry'
      : inputs.length
        ? 'chunk'
        : 'asset';
    b.output(
      oid,
      count(output.bytes, `bytes of ${path}`),
      entry,
      kind,
      'output'
    );
    for (const [input, x] of inputs)
      b.bundles(
        oid,
        id(input),
        count(record(x).bytesInOutput, `bytesInOutput of ${input}`)
      );
    for (const item of list(output.imports ?? [], `imports of ${path}`)) {
      const imp = record(item);
      const target = string(imp.path, 'output import path');
      if (imp.external === true)
        b.external(`bundle:${oid}`, target, string(imp.kind, 'import kind'));
      else
        b.loads(
          oid,
          id(target),
          imp.kind === 'dynamic-import' ? 'dynamic' : 'static'
        );
    }
  }
  return result(b, 'output', outputs.length);
}

// The size of a rollup output: what the dump recorded, else its code or asset source.
function outputBytes(o: Record<string, unknown>) {
  if (typeof o.bytes === 'number') return count(o.bytes, 'bytes');
  const body = o.type === 'asset' ? o.source : o.code;
  if (typeof body === 'string') return Buffer.byteLength(body);
  if (Array.isArray(body)) return body.length;
  return null;
}

/**
 * A rollup or rolldown output bundle (`generateBundle`'s `bundle`), as an object by file name or an array of
 * outputs: chunks with their modules' rendered lengths, static and dynamic imports of other chunks or externals.
 */
export function readRollupBundle(
  data: Record<string, unknown> | unknown[],
  opts: ProviderOptions
) {
  const id = idMaker(opts);
  const b = builder(opts);
  const outputs = (Array.isArray(data) ? data : Object.values(data)).map(
    record
  );
  const names = new Set(outputs.map(o => string(o.fileName, 'fileName')));
  for (const o of outputs) {
    const name = string(o.fileName, 'fileName');
    if (o.type === 'asset') {
      b.output(name, outputBytes(o), null, 'asset', 'output');
      continue;
    }
    if (o.type !== 'chunk')
      throw new Error(`Expected a chunk or an asset: ${name}`);
    const entry =
      typeof o.facadeModuleId === 'string' && (o.isEntry || o.isDynamicEntry)
        ? id(o.facadeModuleId)
        : null;
    const kind = o.isEntry
      ? 'entry'
      : o.isDynamicEntry
        ? 'dynamic-entry'
        : 'chunk';
    b.output(name, outputBytes(o), entry, kind, 'output');
    for (const [module, value] of Object.entries(record(o.modules ?? {}))) {
      const m = record(value);
      const input = id(module);
      b.bundles(name, input, count(m.renderedLength, 'renderedLength'));
      if (m.originalLength !== undefined)
        b.input(input, count(m.originalLength, 'originalLength'));
    }
    for (const [field, how] of [
      ['imports', 'static'],
      ['dynamicImports', 'dynamic'],
    ] as const)
      for (const item of list(o[field] ?? [], field)) {
        const target = string(item, field);
        if (names.has(target)) b.loads(name, target, how);
        else b.external(`bundle:${name}`, target, how);
      }
  }
  return result(b, 'output', outputs.length);
}

/**
 * rolldown's `bundleAnalyzerPlugin` analyze-data.json: chunks by index, modules relative to rolldown's working
 * directory. A module's `size` is its code before rendering, not its bytes in the output: measure `module`.
 */
export function readRolldownAnalyze(
  data: Record<string, unknown>,
  opts: ProviderOptions
) {
  const id = idMaker(opts);
  const b = builder(opts);
  const modules = list(data.modules, 'modules').map(record);
  const chunks = list(data.chunks, 'chunks').map(record);
  const moduleId = (index: unknown) => {
    const m = modules[count(index, 'module index')];
    if (!m) throw new Error(`No module ${String(index)}`);
    return id(string(m.path, 'module path'));
  };
  modules.forEach((m, i) => {
    b.input(moduleId(i), count(m.size, 'module size'));
    for (const importer of list(m.importers ?? [], 'importers'))
      b.imports(moduleId(importer), moduleId(i), 'import');
  });
  for (const c of chunks) {
    const name = string(c.name, 'chunk name');
    const kind =
      c.type === 'static-entry'
        ? 'entry'
        : c.type === 'dynamic-entry'
          ? 'dynamic-entry'
          : 'chunk';
    b.output(
      name,
      count(c.size, 'chunk size'),
      c.entryModule == null ? null : moduleId(c.entryModule),
      kind,
      'module'
    );
    for (const index of list(c.moduleIndices ?? [], 'moduleIndices'))
      b.bundles(
        name,
        moduleId(index),
        count(modules[index as number]?.size, 'module size')
      );
    for (const item of list(c.imports ?? [], 'chunk imports')) {
      const imp = record(item);
      const target = chunks[count(imp.targetChunkIndex, 'targetChunkIndex')];
      if (!target) throw new Error('A chunk import names no chunk');
      b.loads(
        name,
        string(target.name, 'chunk name'),
        imp.type === 'dynamic' ? 'dynamic' : 'static'
      );
    }
  }
  return result(b, 'module', chunks.length);
}

// A webpack module's path: its `name` relative to the compiler's context (`./src/a.js`), without the loaders a
// request names (`./x.js!=!./loader!./y.js`), the concatenation suffix (` + 3 modules`) or a query.
function webpackModulePath(m: Record<string, unknown>) {
  const name =
    typeof m.name === 'string'
      ? m.name
      : string(m.identifier, 'module identifier');
  return name
    .replace(/ \+ \d+ modules?$/, '')
    .split('!')
    .pop()!;
}

/**
 * webpack's stats JSON (`webpack --json`, `stats.toJson()`): chunks with their emitted files and modules with their
 * sizes. A module's `size` is its source after loaders, before minification: measure `module`. A concatenated module
 * counts as the modules inside it. A multi-compiler stats file (`children`) reads every child.
 */
export function readWebpackStats(
  data: Record<string, unknown>,
  opts: ProviderOptions
) {
  const id = idMaker(opts);
  const b = builder(opts);
  let outputs = 0;
  const one = (stats: Record<string, unknown>) => {
    const assets = new Map(
      list(stats.assets ?? [], 'assets').map(a => {
        const asset = record(a);
        return [string(asset.name, 'asset name'), asset] as const;
      })
    );
    const chunks = list(stats.chunks ?? [], 'chunks').map(record);
    const outputOf = new Map<unknown, string>();
    for (const c of chunks) {
      const files = list(c.files ?? [], 'chunk files').map(f =>
        string(f, 'chunk file')
      );
      const name =
        files.find(f => /\.[cm]?js$/.test(f)) ??
        files[0] ??
        `chunk:${String(c.id)}`;
      outputOf.set(c.id, name);
    }
    // The modules each chunk holds: the stats' top-level list (or each chunk's own), inner modules of a concatenation.
    const modules = (
      list(stats.modules ?? [], 'modules').length
        ? list(stats.modules, 'modules')
        : chunks.flatMap(c => list(c.modules ?? [], 'chunk modules'))
    ).map(record);
    const leaves = (m: Record<string, unknown>): Record<string, unknown>[] =>
      Array.isArray(m.modules) && m.modules.length
        ? m.modules.map(record).flatMap(leaves)
        : [m];
    const inChunk = new Map<string, Map<string, number>>();
    for (const m of modules) {
      const external =
        typeof m.identifier === 'string' &&
        m.identifier.startsWith('external ');
      for (const chunk of list(m.chunks ?? [], 'module chunks')) {
        const output = outputOf.get(chunk);
        if (output === undefined) continue;
        if (external) {
          const spec = /^external (?:"([^"]*)"|(\S+))/.exec(
            String(m.identifier)
          );
          b.external(
            `bundle:${output}`,
            spec?.[1] ?? spec?.[2] ?? String(m.identifier),
            'external'
          );
          continue;
        }
        const held = inChunk.get(output) ?? new Map<string, number>();
        for (const leaf of leaves(m)) {
          const input = id(webpackModulePath(leaf));
          held.set(
            input,
            (held.get(input) ?? 0) + count(leaf.size ?? 0, `size of ${input}`)
          );
        }
        inChunk.set(output, held);
      }
      if (external) continue;
      for (const leaf of leaves(m)) {
        const input = id(webpackModulePath(leaf));
        b.input(input, count(leaf.size ?? 0, `size of ${input}`));
        for (const r of list(leaf.reasons ?? [], 'module reasons')) {
          const reason = record(r);
          if (typeof reason.moduleName !== 'string') continue;
          b.imports(
            id(
              reason.moduleName
                .replace(/ \+ \d+ modules?$/, '')
                .split('!')
                .pop()!
            ),
            input,
            typeof reason.type === 'string' ? reason.type : 'import'
          );
        }
      }
    }
    // An entrypoint's first chunk loads the others of its group; a chunk's children load behind import().
    const entries = new Map<string, string>();
    for (const [entry, value] of Object.entries(
      record(stats.entrypoints ?? {})
    )) {
      const group = list(record(value).chunks ?? [], 'entrypoint chunks');
      const [first, ...rest] = group
        .map(c => outputOf.get(c))
        .filter((o): o is string => o !== undefined);
      if (!first) continue;
      entries.set(first, entry);
      for (const o of rest) b.loads(first, o, 'static');
    }
    for (const c of chunks) {
      const output = outputOf.get(c.id)!;
      const files = list(c.files ?? [], 'chunk files').map(f =>
        string(f, 'chunk file')
      );
      const sizes = files.map(f => assets.get(f)?.size);
      const bytes =
        sizes.length && sizes.every(x => typeof x === 'number')
          ? sizes.reduce((a: number, x) => a + count(x, 'asset size'), 0)
          : null;
      const origin = list(c.origins ?? [], 'chunk origins')
        .map(record)
        .find(
          o => typeof o.moduleName === 'string' || typeof o.request === 'string'
        );
      const entryModule =
        c.entry === true && origin
          ? id(
              String(origin.request ?? origin.moduleName)
                .split('!')
                .pop()!
            )
          : null;
      b.output(
        output,
        bytes,
        entryModule,
        c.entry === true || c.initial === true
          ? entries.has(output) || c.entry
            ? 'entry'
            : 'chunk'
          : 'dynamic-entry',
        'module'
      );
      outputs++;
      for (const [input, size] of inChunk.get(output) ?? [])
        b.bundles(output, input, size);
      for (const child of list(c.children ?? [], 'chunk children')) {
        const target = outputOf.get(child);
        if (target !== undefined) b.loads(output, target, 'dynamic');
      }
    }
  };
  one(data);
  for (const child of list(data.children ?? [], 'children')) one(record(child));
  return result(b, 'module', outputs);
}

/** Which bundle format a parsed file is, by its shape; null when it is none of them. */
export function bundleFormat(data: unknown): string | null {
  const obj = (v: unknown): v is Record<string, unknown> =>
    v !== null && typeof v === 'object' && !Array.isArray(v);
  if (Array.isArray(data))
    return data.length &&
      data.every(
        o =>
          obj(o) &&
          typeof o.fileName === 'string' &&
          (o.type === 'chunk' || o.type === 'asset')
      )
      ? 'rollup-bundle'
      : null;
  if (!obj(data)) return null;
  if (obj(data.inputs) && obj(data.outputs)) return 'esbuild-metafile';
  if (
    Array.isArray(data.modules) &&
    Array.isArray(data.chunks) &&
    data.chunks.some(c => obj(c) && Array.isArray(c.moduleIndices))
  )
    return 'rolldown-analyze';
  if (
    Array.isArray(data.modules) ||
    (Array.isArray(data.children) &&
      !Array.isArray(data.chunks) &&
      data.children.some(c => obj(c) && Array.isArray(c.modules))) ||
    (Array.isArray(data.chunks) && Array.isArray(data.assets))
  )
    return 'webpack-stats';
  const values = Object.values(data);
  if (
    values.length &&
    values.every(
      o =>
        obj(o) &&
        typeof o.fileName === 'string' &&
        (o.type === 'chunk' || o.type === 'asset')
    )
  )
    return 'rollup-bundle';
  return null;
}
