/** Public compatibility barrel for memory.ts. */
export { findSimilarMemories, decayComponents, decayScore } from './memory-scoring.js';
export { lexicalSearch } from './memory-search.js';
export { bumpAccess, insertMemory, insertMemoryWithSimilarityGate } from './memory-write.js';
export { getMemory } from './memory-recall.js';
export { archiveMemories, restoreMemories, forgetMemory } from './memory-lifecycle.js';
export { mineWeakness } from './memory-weakness.js';
export { storeEmbedding, searchByEmbedding, loadMemoriesByIds } from './memory-embeddings.js';
export type { GuardedMemoryInsertResult } from './memory-write.js';
export type { WeaknessCluster, MineWeaknessResult, MineWeaknessParams } from './memory-weakness.js';
