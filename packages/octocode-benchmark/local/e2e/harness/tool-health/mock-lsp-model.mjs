// A finite protocol fixture, not a substitute compiler. Native owns tool output,
// source reads, paths, graph traversal, context overlays and pagination.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const range = (line, character, length = 1) => ({ start: { line, character }, end: { line, character: character + length } });
const text = uri => fs.readFileSync(fileURLToPath(uri), 'utf8').split('\n');
const sibling = (uri, name) => new URL(name, uri).href;
const location = (uri, line, name, occurrence = 0) => {
  const source = text(uri)[line] ?? '';
  let character = -1;
  for (let i = 0; i <= occurrence; i++) character = source.indexOf(name, character + 1);
  if (character < 0) throw new Error(`Fixture anchor missing: ${uri}:${line + 1}:${name}`);
  return { uri, range: range(line, character, name.length) };
};
const item = (uri, line, name, kind = 12) => ({ name, kind, uri, range: range(line, 0, (text(uri)[line] ?? '').length), selectionRange: location(uri, line, name).range });
function files(root) {
  return fs.readdirSync(root, { withFileTypes: true }).flatMap(entry => {
    const target = path.join(root, entry.name);
    if (entry.name.startsWith('.') || entry.name === 'node_modules' || entry.name === 'target') return [];
    return entry.isDirectory() ? files(target) : entry.isFile() && /\.(ts|rs)$/.test(entry.name) ? [pathToFileURL(target).href] : [];
  }).toSorted((a, b) => a.localeCompare(b));
}
function wordAt(uri, position) {
  const line = text(uri)[position.line] ?? '';
  return [...line.matchAll(/[A-Za-z_$][\w$]*/g)].find(match => match.index <= position.character && position.character < match.index + match[0].length)?.[0];
}
function runDefinition(uri, position) {
  const line = position.line === 24 ? (position.character < text(uri)[24].indexOf('right.run') ? 22 : 23) : position.line;
  return [location(uri, line, 'run')];
}

function rustReferences(uri, params, options) {
      const cargo = options.cargo ?? {}, features = cargo.features ?? [], cfgs = cargo.cfgs ?? [];
      const modules = ['always'];
      if (!cargo.noDefaultFeatures || features === 'all' || features.includes('base')) modules.push('base_feature');
      for (const feature of ['bright', 'dim']) if (features === 'all' || features.includes(feature)) modules.push(`${feature}_feature`);
      if (cfgs.includes('transport_cfg') && !cfgs.includes('!transport_cfg')) modules.push('custom_cfg');
      return [...(params.context?.includeDeclaration ? [location(uri, 0, 'target')] : []), ...modules.map(name => location(sibling(uri, `${name}.rs`), 0, 'target'))];
}
function references(rootPath, options, params) {
    const uri = params.textDocument.uri, word = wordAt(uri, params.position);
    if (!word) return [];
    if (uri.endsWith('/lib.rs') && word === 'target') return rustReferences(uri, params, options);
    return files(rootPath).flatMap(file => text(file).flatMap((line, index) => {
      if (!params.context?.includeDeclaration && /\b(?:function|import)\b/.test(line)) return [];
      return [...line.matchAll(new RegExp(`\\b${word}\\b`, 'g'))].map(match => ({ uri: file, range: range(index, match.index, word.length) }));
    }));
  }
  function modelItem(uri, name) {
    const rows = { add: [13, 12], calculate: [17, 12], Shape: [0, 11], Square: [4, 5], LargeSquare: [11, 5] };
    const [line, kind] = rows[name];
    return item(sibling(uri, 'model.ts'), line, name, kind);
  }
  function callItem(uri, name) {
    if (uri.endsWith('/cycle.ts')) return item(uri, { alpha: 0, beta: 5, gamma: 9 }[name], name);
    return modelItem(uri, name);
  }
  function calls(method, params) {
    const target = params.item, incoming = method === 'callHierarchy/incomingCalls';
    if (target.uri.endsWith('/cycle.ts')) {
      const edges = [{ from: 'alpha', to: 'beta', line: 2 }, { from: 'alpha', to: 'gamma', line: 2 }, { from: 'beta', to: 'alpha', line: 6 }, { from: 'gamma', to: 'beta', line: 10 }];
      return edges.filter(edge => edge[incoming ? 'to' : 'from'] === target.name).map(edge => ({
        [incoming ? 'from' : 'to']: callItem(target.uri, edge[incoming ? 'from' : 'to']),
        fromRanges: [location(target.uri, edge.line, edge.to).range],
      }));
    }
    if (incoming && target.name === 'add') return [
      { from: modelItem(target.uri, 'calculate'), fromRanges: [18, 19].map(line => location(target.uri, line, 'add').range) },
      { from: { name: '<module>', kind: 2, uri: sibling(target.uri, 'use.ts'), range: range(0, 0), selectionRange: range(0, 0) }, fromRanges: [location(sibling(target.uri, 'use.ts'), 3, 'add').range] },
    ];
    if (!incoming && target.name === 'calculate') return [{ to: modelItem(target.uri, 'add'), fromRanges: [18, 19].map(line => location(target.uri, line, 'add').range) }];
    return [];
  }
export function fixtureModel(root, options = {}) {
  const rootPath = fileURLToPath(root);
  return (method, params) => {
    const uri = params.textDocument?.uri, word = params.position ? wordAt(uri, params.position) : undefined;
    switch (method) {
      case 'textDocument/references': return references(rootPath, options, params);
      case 'textDocument/definition':
        if (word === 'run') return runDefinition(uri, params.position);
        if (word === 'add') return [location(sibling(uri, 'model.ts'), 13, 'add')];
        return references(rootPath, options, { ...params, context: { includeDeclaration: true } }).filter(row => /\bfunction\b/.test(text(row.uri)[row.range.start.line]));
      case 'textDocument/typeDefinition': return [location(sibling(uri, 'model.ts'), 4, 'Square')];
      case 'textDocument/implementation': return ['Square', 'LargeSquare'].map(name => { const value = modelItem(uri, name); return { uri: value.uri, range: value.selectionRange }; });
      case 'textDocument/hover': return { contents: { kind: 'markdown', value: 'function add(a: number, b: number): number' } };
      case 'textDocument/documentSymbol': return ['Shape', 'Square', 'LargeSquare', 'add', 'calculate'].map(name => { const { uri: _uri, ...value } = modelItem(uri, name); return value; });
      case 'workspace/symbol': return params.query === 'calculate' ? [{ name: 'calculate', kind: 12, location: location(pathToFileURL(path.join(rootPath, 'model.ts')).href, 17, 'calculate') }] : [];
      case 'textDocument/diagnostic': return { kind: 'full', items: [{ range: range(0, 0, 1), severity: 1, code: 2322, source: 'octocode-protocol-fixture', message: 'Fixture type mismatch' }] };
      default: return hierarchy(method, params, uri, word);
    }
  };
}
function hierarchy(method, params, uri, word) {
    switch (method) {
      case 'textDocument/prepareCallHierarchy': return word === 'greet' ? [] : [callItem(uri, word)];
      case 'callHierarchy/incomingCalls':
      case 'callHierarchy/outgoingCalls': return calls(method, params);
      case 'textDocument/prepareTypeHierarchy': return [modelItem(uri, word)];
      case 'typeHierarchy/supertypes': return { LargeSquare: ['Square'], Square: ['Shape'], Shape: [] }[params.item.name].map(name => modelItem(params.item.uri, name));
      case 'typeHierarchy/subtypes': return { Shape: ['Square'], Square: ['LargeSquare'], LargeSquare: [] }[params.item.name].map(name => modelItem(params.item.uri, name));
      default: throw Object.assign(new Error(`Unsupported fixture method: ${method}`), { code: -32601 });
    }
}
