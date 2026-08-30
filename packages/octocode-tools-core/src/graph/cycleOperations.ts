import type { FileNode } from './types.js';

export interface DirectedFileEdge {
  from: string;
  to: string;
}

export interface DetailedFileEdge extends DirectedFileEdge {
  edgeKinds: string[];
}

/** Return one deterministic directed cycle contained entirely in `members`. */
export function findCycleWitness(
  graph: ReadonlyMap<string, FileNode>,
  members: ReadonlySet<string>
): DirectedFileEdge[] {
  const state = new Map<string, 'visiting' | 'done'>();
  const parent = new Map<string, string>();
  type Frame = { node: string; successors: string[]; offset: number };

  for (const root of [...members].sort()) {
    if (!graph.has(root) || state.has(root)) continue;
    state.set(root, 'visiting');
    const frames: Frame[] = [
      {
        node: root,
        successors: [...(graph.get(root)?.importsFiles ?? [])]
          .filter(target => members.has(target))
          .sort(),
        offset: 0,
      },
    ];

    while (frames.length > 0) {
      const frame = frames[frames.length - 1] as Frame;
      const successor = frame.successors[frame.offset++];
      if (successor === undefined) {
        state.set(frame.node, 'done');
        frames.pop();
        continue;
      }

      const successorState = state.get(successor);
      if (!successorState) {
        parent.set(successor, frame.node);
        state.set(successor, 'visiting');
        frames.push({
          node: successor,
          successors: [...(graph.get(successor)?.importsFiles ?? [])]
            .filter(target => members.has(target))
            .sort(),
          offset: 0,
        });
        continue;
      }
      if (successorState !== 'visiting') continue;

      const cycleNodes = [frame.node];
      while (cycleNodes[cycleNodes.length - 1] !== successor) {
        const previous = parent.get(
          cycleNodes[cycleNodes.length - 1] as string
        );
        if (!previous) return [];
        cycleNodes.push(previous);
      }
      cycleNodes.reverse();
      const edges: DirectedFileEdge[] = [];
      for (let index = 0; index < cycleNodes.length - 1; index++) {
        edges.push({
          from: cycleNodes[index] as string,
          to: cycleNodes[index + 1] as string,
        });
      }
      edges.push({ from: frame.node, to: successor });
      return edges;
    }
  }
  return [];
}

export function describeCycleWitness(
  graph: ReadonlyMap<string, FileNode>,
  members: ReadonlySet<string>
): DetailedFileEdge[] {
  return findCycleWitness(graph, members).map(({ from, to }) => ({
    from,
    to,
    edgeKinds: [
      ...(graph.get(from)?.edgeKinds.get(to) ?? new Set(['static-import'])),
    ].sort(),
  }));
}

export function componentLayerMap(
  layers: readonly (readonly number[])[]
): Map<number, number> {
  const result = new Map<number, number>();
  layers.forEach((layer, layerIndex) => {
    for (const component of layer) result.set(component, layerIndex);
  });
  return result;
}
