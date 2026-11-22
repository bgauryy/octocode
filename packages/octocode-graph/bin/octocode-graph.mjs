#!/usr/bin/env node
import { runGraphCli } from '../dist/cli.js';

try {
  process.exitCode = await runGraphCli(process.argv.slice(2));
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
