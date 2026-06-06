#!/usr/bin/env node

import { ensureNativeDependencies } from './common/ensure-deps.js';

ensureNativeDependencies(import.meta.url, { tag: '[octocode-scan]' });

const { main, EXIT_ERROR } = await import('./pipeline/main.js');
const { OptionsError } = await import('./pipeline/create-options.js');
try {
  const exitCode = await main();
  process.exitCode = exitCode;
} catch (err: unknown) {
  if (err instanceof OptionsError) {
    process.stderr.write(`${err.message}\n`);
  } else {
    console.error(err);
  }
  process.exitCode = EXIT_ERROR;
}
