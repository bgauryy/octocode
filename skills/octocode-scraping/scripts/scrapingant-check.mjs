#!/usr/bin/env node
/** @deprecated Use provider-check.mjs — compatibility shim. */
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const result = spawnSync(process.execPath, [join(here, 'provider-check.mjs'), ...process.argv.slice(2)], { stdio: 'inherit' });
process.exit(result.status ?? 1);
