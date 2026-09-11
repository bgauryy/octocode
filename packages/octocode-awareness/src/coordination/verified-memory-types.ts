import type { DatabaseSync } from 'node:sqlite';
import type { MemoryRecallModeV1 } from '../memory-hardening.js';
import type { AwarenessOperationCall } from '../schema/operation-types.js';

export type VerifiedMemoryPartialReason = 'limit' | 'terminal-limit' | 'snapshot_changed';

export interface VerifiedMemoryV1 {
  version: 1;
  memoryId: string;
  workspacePath: string;
  label: string;
  text: string;
  scope: 'project' | 'artifact';
  artifact?: string;
  sourceDigest: string;
  verifiedAt: string;
  validUntil?: string;
  importance: number;
  file?: string[];
  area?: string;
  why?: string;
  constraint?: string;
  historyRef?: string;
  historyEvidence?: {
    state: 'recorded' | 'incomplete' | 'unavailable';
    reason: string;
    next?: { call: AwarenessOperationCall<'history.read'> };
  };
  explanation?: string;
}

export interface VerifiedMemoryRecallParams {
  memoryId?: string;
  query?: string;
  label?: string;
  sourceDigest?: string;
  scope?: 'project' | 'artifact';
  artifact?: string;
  limit?: number;
  offset?: number;
  now?: string;
  mode?: MemoryRecallModeV1;
  minSimilarity?: number;
  file?: string | string[];
  area?: string;
  revision?: string;
  strictScope?: boolean;
}

export interface VerifiedMemoryPageV1 {
  memories: VerifiedMemoryV1[];
  partial: boolean;
  partialReasons: VerifiedMemoryPartialReason[];
  revision: string;
  terminalLimit?: { code: 'MEMORY_SEMANTIC_LIMIT'; candidateLimit: number; message: string };
  warnings?: string[];
  next?: { params: VerifiedMemoryRecallParams };
}

export interface VerifiedMemoryHost {
  readonly db: DatabaseSync;
  readonly canonicalWorkspace: string;
  writeTransaction<T>(operation: () => T): T;
  embedMemory(memoryId: string, text: string): boolean;
}

export interface VerifiedMemoryStoreParams {
  label: string;
  text: string;
  scope?: 'project' | 'artifact';
  artifact?: string;
  sourceDigest: string;
  verifiedAt?: string;
  validUntil?: string;
  importance?: number;
  tags?: string | string[] | null;
  file?: string | string[] | null;
  area?: string;
  why?: string;
  constraint?: string;
  historyRef?: string;
  supersedes?: string[];
}
