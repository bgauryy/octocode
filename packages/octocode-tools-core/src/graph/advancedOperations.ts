import type { FileGraphEdgeKind, FileNode } from './types.js';

const RUNTIME_IMPORT_KINDS = new Set<FileGraphEdgeKind>([
  'static-import',
  'dynamic-import',
  'named-reexport',
  'star-reexport',
  'commonjs-require',
  'create-require',
  'python-import',
]);

/** Runtime import candidates; Rust names/modules and C preprocessing are topology only. */
export function runtimeImportGraph(
  graph: ReadonlyMap<string, FileNode>
): Map<string, FileNode> {
  return new Map(
    [...graph].map(([file, node]) => {
      const retained = [...node.importsFiles].filter(target => {
        const kinds = node.edgeKinds.get(target);
        return [...(kinds ?? [])].some(kind => RUNTIME_IMPORT_KINDS.has(kind));
      });
      return [
        file,
        {
          relativePath: file,
          importsFiles: new Set(retained),
          dynamicImportsFiles: new Set(
            retained.filter(target => node.dynamicImportsFiles.has(target))
          ),
          edgeKinds: new Map(
            retained.map(target => [
              target,
              new Set(node.edgeKinds.get(target) ?? ['static-import']),
            ])
          ),
        },
      ];
    })
  );
}

export interface CondensedGraph {
  components: string[][];
  componentOf: Map<string, number>;
  edges: Map<number, Set<number>>;
  reverseEdges: Map<number, Set<number>>;
  layers: number[][];
}

/** Condense every SCC (including acyclic singletons) into a deterministic DAG. */
export function condenseGraph(
  graph: ReadonlyMap<string, FileNode>
): CondensedGraph {
  let index = 0;
  const indices = new Map<string, number>();
  const lowlinks = new Map<string, number>();
  const stack: string[] = [];
  const onStack = new Set<string>();
  const components: string[][] = [];

  type Frame = { node: string; successors: string[]; offset: number };
  for (const root of [...graph.keys()].sort()) {
    if (indices.has(root)) continue;
    const frames: Frame[] = [
      {
        node: root,
        successors: [...(graph.get(root)?.importsFiles ?? [])].sort(),
        offset: 0,
      },
    ];
    indices.set(root, index);
    lowlinks.set(root, index++);
    stack.push(root);
    onStack.add(root);

    while (frames.length > 0) {
      const frame = frames[frames.length - 1] as Frame;
      const successor = frame.successors[frame.offset++];
      if (successor !== undefined) {
        if (!indices.has(successor)) {
          indices.set(successor, index);
          lowlinks.set(successor, index++);
          stack.push(successor);
          onStack.add(successor);
          frames.push({
            node: successor,
            successors: [...(graph.get(successor)?.importsFiles ?? [])].sort(),
            offset: 0,
          });
        } else if (onStack.has(successor)) {
          lowlinks.set(
            frame.node,
            Math.min(
              lowlinks.get(frame.node) as number,
              indices.get(successor) as number
            )
          );
        }
        continue;
      }

      frames.pop();
      const parent = frames[frames.length - 1];
      if (parent) {
        lowlinks.set(
          parent.node,
          Math.min(
            lowlinks.get(parent.node) as number,
            lowlinks.get(frame.node) as number
          )
        );
      }
      if (lowlinks.get(frame.node) !== indices.get(frame.node)) continue;
      const component: string[] = [];
      let popped: string;
      do {
        popped = stack.pop() as string;
        onStack.delete(popped);
        component.push(popped);
      } while (popped !== frame.node);
      components.push(component.sort());
    }
  }

  components.sort((a, b) => (a[0] ?? '').localeCompare(b[0] ?? ''));
  const componentOf = new Map<string, number>();
  components.forEach((component, id) => {
    for (const file of component) componentOf.set(file, id);
  });
  const edges = new Map<number, Set<number>>();
  const reverseEdges = new Map<number, Set<number>>();
  components.forEach((_, id) => {
    edges.set(id, new Set());
    reverseEdges.set(id, new Set());
  });
  for (const [source, node] of graph) {
    const from = componentOf.get(source) as number;
    for (const target of node.importsFiles) {
      const to = componentOf.get(target);
      if (to === undefined || to === from) continue;
      edges.get(from)?.add(to);
      reverseEdges.get(to)?.add(from);
    }
  }

  const indegree = new Map(
    components.map((_, id) => [id, reverseEdges.get(id)?.size ?? 0])
  );
  const layers: number[][] = [];
  let frontier = [...indegree]
    .filter(([, degree]) => degree === 0)
    .map(([id]) => id)
    .sort((a, b) => a - b);
  while (frontier.length > 0) {
    layers.push(frontier);
    const next: number[] = [];
    for (const from of frontier) {
      for (const to of [...(edges.get(from) ?? [])].sort((a, b) => a - b)) {
        const degree = (indegree.get(to) as number) - 1;
        indegree.set(to, degree);
        if (degree === 0) next.push(to);
      }
    }
    frontier = next.sort((a, b) => a - b);
  }
  return { components, componentOf, edges, reverseEdges, layers };
}

/** Edges removable from a DAG without changing reachability. */
export function findTransitiveEdges(
  edges: ReadonlyMap<number, ReadonlySet<number>>
): Set<string> {
  const redundant = new Set<string>();
  for (const [source, targets] of edges) {
    for (const target of targets) {
      const visited = new Set<number>([source]);
      const stack = [...targets].filter(candidate => candidate !== target);
      while (stack.length > 0) {
        const current = stack.pop() as number;
        if (current === target) {
          redundant.add(`${source}:${target}`);
          break;
        }
        if (visited.has(current)) continue;
        visited.add(current);
        stack.push(...(edges.get(current) ?? []));
      }
    }
  }
  return redundant;
}

/** Classic iterative dominator sets for nodes reachable from one source. */
export function computeImmediateDominators(
  graph: ReadonlyMap<string, FileNode>,
  source: string
): Map<string, string | null> {
  const seen = new Set<string>();
  const postorder: string[] = [];
  type Frame = { node: string; successors: string[]; offset: number };
  const frames: Frame[] = [
    {
      node: source,
      successors: [...(graph.get(source)?.importsFiles ?? [])].sort(),
      offset: 0,
    },
  ];
  seen.add(source);
  while (frames.length > 0) {
    const frame = frames[frames.length - 1] as Frame;
    const successor = frame.successors[frame.offset++];
    if (successor !== undefined) {
      if (seen.has(successor)) continue;
      seen.add(successor);
      frames.push({
        node: successor,
        successors: [...(graph.get(successor)?.importsFiles ?? [])].sort(),
        offset: 0,
      });
      continue;
    }
    frames.pop();
    postorder.push(frame.node);
  }
  const reversePostorder = postorder.reverse();
  const order = new Map(
    reversePostorder.map((node, position) => [node, position])
  );
  const predecessors = new Map(
    reversePostorder.map(node => [node, new Set<string>()])
  );
  for (const node of reversePostorder) {
    for (const target of graph.get(node)?.importsFiles ?? []) {
      predecessors.get(target)?.add(node);
    }
  }
  const idom = new Map<string, string>([[source, source]]);
  const intersect = (left: string, right: string): string => {
    let first = left;
    let second = right;
    while (first !== second) {
      while ((order.get(first) as number) > (order.get(second) as number)) {
        first = idom.get(first) as string;
      }
      while ((order.get(second) as number) > (order.get(first) as number)) {
        second = idom.get(second) as string;
      }
    }
    return first;
  };
  let changed = true;
  while (changed) {
    changed = false;
    for (const node of reversePostorder.slice(1)) {
      const preds = [...(predecessors.get(node) ?? [])].filter(pred =>
        idom.has(pred)
      );
      if (preds.length === 0) continue;
      let next = preds[0] as string;
      for (const pred of preds.slice(1)) next = intersect(pred, next);
      if (idom.get(node) !== next) {
        idom.set(node, next);
        changed = true;
      }
    }
  }
  const immediate = new Map<string, string | null>([[source, null]]);
  for (const node of reversePostorder.slice(1))
    immediate.set(node, idom.get(node) ?? null);
  return immediate;
}

/** Dijkstra primitive for future callers that can supply defensible costs. */
export function findWeightedShortestPath(
  graph: ReadonlyMap<string, FileNode>,
  source: string,
  target: string,
  edgeCost: (
    from: string,
    to: string,
    kinds: ReadonlySet<FileGraphEdgeKind>
  ) => number
): { found: boolean; files: string[]; cost?: number } {
  const distance = new Map<string, number>([[source, 0]]);
  const previous = new Map<string, string>();
  const heap: Array<{ node: string; cost: number }> = [
    { node: source, cost: 0 },
  ];
  const push = (item: { node: string; cost: number }): void => {
    heap.push(item);
    let child = heap.length - 1;
    while (child > 0) {
      const parent = Math.floor((child - 1) / 2);
      const parentItem = heap[parent] as { node: string; cost: number };
      if (
        parentItem.cost < item.cost ||
        (parentItem.cost === item.cost && parentItem.node <= item.node)
      )
        break;
      heap[child] = parentItem;
      child = parent;
    }
    heap[child] = item;
  };
  const pop = (): { node: string; cost: number } | undefined => {
    const first = heap[0];
    const last = heap.pop();
    if (!first || !last || heap.length === 0) return first;
    let parent = 0;
    while (true) {
      const left = parent * 2 + 1;
      if (left >= heap.length) break;
      const right = left + 1;
      let child = left;
      if (right < heap.length) {
        const leftItem = heap[left] as { node: string; cost: number };
        const rightItem = heap[right] as { node: string; cost: number };
        if (
          rightItem.cost < leftItem.cost ||
          (rightItem.cost === leftItem.cost && rightItem.node < leftItem.node)
        )
          child = right;
      }
      const childItem = heap[child] as { node: string; cost: number };
      if (
        last.cost < childItem.cost ||
        (last.cost === childItem.cost && last.node <= childItem.node)
      )
        break;
      heap[parent] = childItem;
      parent = child;
    }
    heap[parent] = last;
    return first;
  };
  while (heap.length > 0) {
    const currentItem = pop() as { node: string; cost: number };
    const current = currentItem.node;
    const best = currentItem.cost;
    if (best !== distance.get(current)) continue;
    if (current === target) break;
    const node = graph.get(current);
    for (const next of node?.importsFiles ?? []) {
      const cost = edgeCost(
        current,
        next,
        node?.edgeKinds.get(next) ?? new Set(['static-import'])
      );
      if (!Number.isFinite(cost) || cost < 0) {
        throw new Error('edgeCost must return a finite non-negative number');
      }
      const candidate = best + cost;
      if (candidate < (distance.get(next) ?? Number.POSITIVE_INFINITY)) {
        distance.set(next, candidate);
        previous.set(next, current);
        push({ node: next, cost: candidate });
      }
    }
  }
  if (!distance.has(target)) return { found: false, files: [] };
  const files = [target];
  while (files[0] !== source) {
    files.unshift(previous.get(files[0] as string) as string);
  }
  return { found: true, files, cost: distance.get(target) };
}
