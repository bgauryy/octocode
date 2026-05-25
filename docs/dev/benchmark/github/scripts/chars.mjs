#!/usr/bin/env node
// chars.mjs — Count Unicode codepoints in stdin/--file/--text.
// Deterministic. No tokenizer, no fallback, no dependencies.
import { readFileSync } from 'fs';
const a = process.argv.slice(2);
const i = a.indexOf('--file'), j = a.indexOf('--text');
let text;
if (j !== -1) text = a[j + 1] ?? '';
else if (i !== -1) text = readFileSync(a[i + 1], 'utf8');
else { const c = []; if (!process.stdin.isTTY) for await (const x of process.stdin) c.push(x); text = c.join(''); }
process.stdout.write(String([...text].length) + '\n');
