import { createRequire } from 'node:module';

const globalWithRequire = globalThis as { require?: unknown };
globalWithRequire.require ??= createRequire(import.meta.url);
