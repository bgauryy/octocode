import type { GrammarCapability } from '@octocodeai/octocode-engine';

import { contextUtils } from './utils/contextUtils.js';

/** Runtime parser inventory owned by octocode-engine's canonical registry. */
export function getGrammarCapabilities(): GrammarCapability[] {
  return contextUtils.getGrammarCapabilities();
}

export type { GrammarCapability };
