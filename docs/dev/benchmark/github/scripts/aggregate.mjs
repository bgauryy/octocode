#!/usr/bin/env node
// aggregate.mjs — Sum log.jsonl entries for one question.
// Usage: node aggregate.mjs <log> <q>   →   prints "calls in_chars out_chars elapsed_ms"
import { readFileSync, existsSync } from 'fs';
const [log, q] = process.argv.slice(2);
if (!log || !q) { console.error('Usage: aggregate.mjs <log> <q>'); process.exit(1); }
if (!existsSync(log)) { console.log('0 0 0 0'); process.exit(0); }
const rows = readFileSync(log, 'utf8').split('\n').filter(Boolean).map(JSON.parse).filter(r => r.q === +q);
const sum = (k) => rows.reduce((s, r) => s + (r[k] || 0), 0);
console.log(`${rows.length} ${sum('in_chars')} ${sum('out_chars')} ${sum('elapsed_ms')}`);
