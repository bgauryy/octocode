import { CASE_FOLDING } from './casefold-table.generated.js';

/**
 * The reservation key rules, shared with the octocode-agents-communication package and pinned by the vectors in
 * `test-utils/agent-coordination/contract.json` (both test suites run them):
 *   - a path is compared component by component, each one folded: NFD, full Unicode case folding, NFD again, so
 *     `Straße.ts` and `STRASSE.ts`, or `ﬁle` and `FILE`, name one file on every filesystem;
 *   - the key is `/` plus the folded components joined by `/` (the root is `''`);
 *   - a tree reservation covers its key and everything below it, on component boundaries; a file covers only its key.
 * The folding table is the package's frozen Unicode 16 one (`casefold-table.generated.ts`, generated from it), so keys do not
 * depend on the Node or ICU version; only NFD uses the runtime's normalizer, and a normalization never changes for an
 * assigned character.
 */

export type LeaseKind = 'file' | 'tree';

let folding: ReadonlyMap<number, string> | undefined;

function foldingTable(): ReadonlyMap<number, string> {
  if (!folding) {
    folding = new Map(
      CASE_FOLDING.trim().split('\n').map((line) => {
        const [from = '', to = ''] = line.split(':');
        return [parseInt(from, 16), String.fromCodePoint(...to.split(' ').map((point) => parseInt(point, 16)))] as const;
      }),
    );
  }
  return folding;
}

/** One path component as it is compared: NFD, full case folding, NFD. */
export function foldComponent(component: string): string {
  // Same shortcut as the package: for ASCII, folding is lower-casing.
  if (/^[\u0000-\u007f]*$/.test(component)) return component.toLowerCase();
  const table = foldingTable();
  const decomposed = component.normalize('NFD');
  let folded = '';
  for (const point of decomposed) folded += table.get(point.codePointAt(0)!) ?? point;
  return folded.normalize('NFD');
}

/** The key of a path given as its components (empty and `.` ones are not components). */
export function leaseKey(components: readonly string[]): string {
  return components.filter((part) => part && part !== '.').map((part) => `/${foldComponent(part)}`).join('');
}

/** The keys of every directory above `key`, nearest last, ending with the root `''` first: `/a/b` → `['', '/a']`. */
export function ancestorKeys(key: string): string[] {
  const result: string[] = [];
  for (let at = key.indexOf('/'); at !== -1; at = key.indexOf('/', at + 1)) result.push(key.slice(0, at));
  return result;
}

/** Whether two reservations conflict: the same key, or a tree whose key is an ancestor of the other's. */
export function keysOverlap(a: string, aKind: LeaseKind, b: string, bKind: LeaseKind): boolean {
  return a === b || (aKind === 'tree' && b.startsWith(`${a}/`)) || (bKind === 'tree' && a.startsWith(`${b}/`));
}

/** The first two requests of one call that overlap each other (including a repeated path), or undefined: a call asks for a set that does not conflict with itself. */
export function overlappingRequests<T extends { key: string; kind: LeaseKind }>(requests: readonly T[]): [T, T] | undefined {
  for (let later = 1; later < requests.length; later += 1) {
    for (let earlier = 0; earlier < later; earlier += 1) {
      const [a, b] = [requests[earlier]!, requests[later]!];
      if (keysOverlap(a.key, a.kind, b.key, b.kind)) return [a, b];
    }
  }
  return undefined;
}
