#!/usr/bin/env node
// chars.mjs — Count Unicode codepoints in stdin/--file/--text.
// Deterministic, dependency-free benchmark ruler.
//
// Usage:
//   node chars.mjs [--file P | --text S | <stdin>]
import { readFileSync } from 'fs';

const args = process.argv.slice(2);
let text;
if (args[0] === '--file') {
  text = readFileSync(args[1], 'utf8');
} else if (args[0] === '--text') {
  text = args.slice(1).join(' ');
} else {
  text = readFileSync(0, 'utf8');
}
process.stdout.write(String([...String(text ?? '')].length));
