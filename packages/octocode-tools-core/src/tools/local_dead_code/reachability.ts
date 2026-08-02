import type { FileNode } from './types.js';

/** BFS from every entrypoint across the directed file-import graph. */
export function computeReachableFiles(
  fileGraph: ReadonlyMap<string, FileNode>,
  entrypoints: readonly string[]
): Set<string> {
  const reachable = new Set<string>();
  const queue: string[] = [];

  for (const entry of entrypoints) {
    if (!reachable.has(entry)) {
      reachable.add(entry);
      queue.push(entry);
    }
  }

  while (queue.length > 0) {
    const current = queue.pop() as string;
    const node = fileGraph.get(current);
    if (!node) continue;
    for (const target of node.importsFiles) {
      if (!reachable.has(target)) {
        reachable.add(target);
        queue.push(target);
      }
    }
  }

  return reachable;
}

export interface StronglyConnectedComponent {
  files: string[];
}

/**
 * Iterative Tarjan's SCC over the file-import graph (iterative, not
 * recursive, so a deep or cyclic graph in a large repo can't blow the call
 * stack). Only components with more than one file — or a single file that
 * imports itself — are real cycles; singleton components are dropped.
 *
 * This does NOT change what's reachable (BFS in `computeReachableFiles`
 * already resolves cycles correctly — a 2-file cycle with no path from any
 * entrypoint is simply never visited). What SCC adds is explanatory: instead
 * of reporting N separately "unreachable" files, a cyclic cluster is grouped
 * and named as one unit — the exact case a one-symbol-at-a-time reference
 * check (ask "does A call B" in isolation) gets wrong, since each file in the
 * cluster looks locally referenced by the other.
 */
export function findStronglyConnectedComponents(
  fileGraph: ReadonlyMap<string, FileNode>
): StronglyConnectedComponent[] {
  let nextIndex = 0;
  const indices = new Map<string, number>();
  const lowlinks = new Map<string, number>();
  const onStack = new Set<string>();
  const stack: string[] = [];
  const components: StronglyConnectedComponent[] = [];

  type Frame = { node: string; iterator: IterableIterator<string> };
  const callStack: Frame[] = [];

  for (const root of fileGraph.keys()) {
    if (indices.has(root)) continue;

    callStack.push({
      node: root,
      iterator: (fileGraph.get(root)?.importsFiles ?? new Set()).values(),
    });
    indices.set(root, nextIndex);
    lowlinks.set(root, nextIndex);
    nextIndex++;
    stack.push(root);
    onStack.add(root);

    while (callStack.length > 0) {
      const frame = callStack[callStack.length - 1] as Frame;
      const next = frame.iterator.next();

      if (!next.done) {
        const successor = next.value;
        if (!indices.has(successor)) {
          indices.set(successor, nextIndex);
          lowlinks.set(successor, nextIndex);
          nextIndex++;
          stack.push(successor);
          onStack.add(successor);
          callStack.push({
            node: successor,
            iterator: (
              fileGraph.get(successor)?.importsFiles ?? new Set()
            ).values(),
          });
        } else if (onStack.has(successor)) {
          const vLow = lowlinks.get(frame.node) as number;
          const wIndex = indices.get(successor) as number;
          lowlinks.set(frame.node, Math.min(vLow, wIndex));
        }
        continue;
      }

      // Finished exploring `frame.node`'s successors.
      callStack.pop();
      if (callStack.length > 0) {
        const parent = callStack[callStack.length - 1] as Frame;
        const parentLow = lowlinks.get(parent.node) as number;
        const childLow = lowlinks.get(frame.node) as number;
        lowlinks.set(parent.node, Math.min(parentLow, childLow));
      }

      if (lowlinks.get(frame.node) === indices.get(frame.node)) {
        const component: string[] = [];
        let popped: string;
        do {
          popped = stack.pop() as string;
          onStack.delete(popped);
          component.push(popped);
        } while (popped !== frame.node);

        const isRealCycle =
          component.length > 1 ||
          (fileGraph
            .get(component[0] as string)
            ?.importsFiles.has(component[0] as string) ??
            false);
        if (isRealCycle) components.push({ files: component });
      }
    }
  }

  return components;
}
