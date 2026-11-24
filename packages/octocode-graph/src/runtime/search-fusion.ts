// Reciprocal rank fusion: ranked lists of search documents become one order, and each document keeps the lists that
// found it (`found` on a search row). The name list, BM25F and an optional provider rerank fuse this way.

/** The rank offset of reciprocal rank fusion: a list's first entries count most, its tail never vanishes. */
export const RRF_K = 60;

/** One ranked list for fusion: documents best first, and how much the list counts. */
export interface RankedList {
  signal: string;
  docs: number[];
  weight: number;
}

/** Reciprocal rank fusion: each document's score and the lists that hold it. */
export function fuse(lists: RankedList[]) {
  const score = new Map<number, number>();
  const found = new Map<number, string[]>();
  for (const list of lists)
    list.docs.forEach((d, i) => {
      score.set(d, (score.get(d) ?? 0) + list.weight / (RRF_K + i + 1));
      const signals = found.get(d);
      if (signals) signals.push(list.signal);
      else found.set(d, [list.signal]);
    });
  return { score, found };
}
