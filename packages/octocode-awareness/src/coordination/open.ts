import { AwarenessStore } from './coordination-continuity.js';
import type { AwarenessOptions } from './coordination-shared.js';

export function openAwarenessStore(options: AwarenessOptions = {}): AwarenessStore {
  return new AwarenessStore(options);
}
