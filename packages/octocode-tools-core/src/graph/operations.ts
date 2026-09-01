import { posix } from 'node:path';

import type { FileGraphEdgeKind, FileNode } from './types.js';

export function normalizeGraphFile(file: string): string {
  return posix.normalize(file.split('\\').join('/')).replace(/^\.\//, '');
}

export function getFileEdgeKinds(
  node: FileNode | undefined,
  target: string
): FileGraphEdgeKind[] {
  return [...(node?.edgeKinds.get(target) ?? ['static-import'])].sort();
}

export function collectGraphEdgeKinds(
  graph: ReadonlyMap<string, FileNode>,
  files: readonly string[]
): FileGraphEdgeKind[] {
  const members = new Set(files);
  const kinds = new Set<FileGraphEdgeKind>();
  for (const file of files) {
    const node = graph.get(file);
    for (const target of node?.importsFiles ?? []) {
      if (!members.has(target)) continue;
      for (const kind of getFileEdgeKinds(node, target)) kinds.add(kind);
    }
  }
  return [...kinds].sort();
}

export function traverseGraph(
  graph: ReadonlyMap<string, FileNode>,
  source: string,
  depth: number
): Array<Record<string, unknown>> {
  const results: Array<Record<string, unknown>> = [];
  const visited = new Set([source]);
  const queue: Array<{ file: string; distance: number }> = [
    { file: source, distance: 0 },
  ];

  for (let index = 0; index < queue.length; index++) {
    const current = queue[index] as { file: string; distance: number };
    if (current.distance >= depth) continue;
    const node = graph.get(current.file);
    const targets = [...(node?.importsFiles ?? [])].sort();
    for (const target of targets) {
      if (visited.has(target)) continue;
      visited.add(target);
      const distance = current.distance + 1;
      queue.push({ file: target, distance });
      results.push({
        file: target,
        distance,
        via: current.file,
        edgeKinds: getFileEdgeKinds(node, target),
        confidence: 'syntactic',
      });
    }
  }
  return results;
}

export function reverseGraph(
  graph: ReadonlyMap<string, FileNode>
): Map<string, FileNode> {
  const reversed = new Map<string, FileNode>();
  for (const file of graph.keys()) {
    reversed.set(file, {
      relativePath: file,
      importsFiles: new Set(),
      dynamicImportsFiles: new Set(),
      edgeKinds: new Map(),
    });
  }
  for (const [source, node] of graph) {
    for (const target of node.importsFiles) {
      const reverseNode = reversed.get(target);
      if (!reverseNode) continue;
      reverseNode.importsFiles.add(source);
      const kinds = new Set(getFileEdgeKinds(node, target));
      reverseNode.edgeKinds.set(source, kinds);
      if (kinds.size === 1 && kinds.has('dynamic-import')) {
        reverseNode.dynamicImportsFiles.add(source);
      }
    }
  }
  return reversed;
}

export function findShortestPath(
  graph: ReadonlyMap<string, FileNode>,
  source: string,
  target: string
): Record<string, unknown> {
  const previous = new Map<string, string>();
  const visited = new Set([source]);
  const queue = [source];

  for (let index = 0; index < queue.length; index++) {
    const current = queue[index] as string;
    if (current === target) break;
    for (const next of [...(graph.get(current)?.importsFiles ?? [])].sort()) {
      if (visited.has(next)) continue;
      visited.add(next);
      previous.set(next, current);
      queue.push(next);
    }
  }

  if (!visited.has(target)) {
    return { found: false, files: [], edges: [] };
  }

  const files = [target];
  while (files[0] !== source) {
    files.unshift(previous.get(files[0] as string) as string);
  }
  const edges = files.slice(0, -1).map((from, index) => {
    const to = files[index + 1] as string;
    return {
      from,
      to,
      edgeKinds: getFileEdgeKinds(graph.get(from), to),
      confidence: 'syntactic',
    };
  });
  return {
    found: true,
    files,
    edges,
    length: files.length,
    complete: true,
    confidence: 'syntactic',
  };
}
