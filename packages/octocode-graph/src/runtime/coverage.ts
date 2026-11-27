// Completeness of a stitched analysis: preserve each built chunk's partial
// disclosure and name every planned chunk that produced no evidence.
import type { Chunk, Coverage, SectionData } from './types.ts';

export function chunkCoverage(
  chunks: readonly Pick<Chunk, 'id' | 'path'>[],
  datas: ReadonlyMap<string, Pick<SectionData, 'coverage' | 'snapshot'>>
) {
  const notBuilt = chunks.filter(c => !datas.has(c.id)).map(c => c.id);
  const partialChunks: NonNullable<Coverage['partialChunks']> = chunks.flatMap(
    c => {
      const data = datas.get(c.id);
      if (!data) return [];
      const partialReasons = [
        ...new Set([
          ...(data.coverage?.partialReasons ?? []),
          ...(data.snapshot?.partialReasons ?? []),
        ]),
      ];
      return data.coverage?.isPartial ||
        data.snapshot?.isPartial ||
        partialReasons.length
        ? [{ id: c.id, path: c.path, partialReasons }]
        : [];
    }
  );
  const partialReasons = [
    ...new Set([
      ...(notBuilt.length ? ['chunksNotBuilt'] : []),
      ...partialChunks.flatMap(c => c.partialReasons),
    ]),
  ];
  return {
    partial: {
      isPartial: notBuilt.length > 0 || partialChunks.length > 0,
      partialReasons,
    },
    partialChunks,
    notBuilt,
  };
}
