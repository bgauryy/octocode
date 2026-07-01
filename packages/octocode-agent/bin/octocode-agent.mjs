#!/usr/bin/env node
// Executable entry for the octocode-agent platform.
// All logic lives in ./launcher.mjs (side-effect-free, unit-tested); this wrapper
// only bridges the process to it and forwards the exit code.
import { main } from './launcher.mjs';

process.exitCode = main(process.argv.slice(2));
