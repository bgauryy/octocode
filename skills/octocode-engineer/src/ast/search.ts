#!/usr/bin/env node

import { ensureNativeDependencies } from '../common/ensure-deps.js';

ensureNativeDependencies(import.meta.url, { tag: '[octocode-ast-search]' });

const { main } = await import('./search-main.js');
main().catch((error: unknown) => {
  console.error(error);
  process.exit(1);
});
