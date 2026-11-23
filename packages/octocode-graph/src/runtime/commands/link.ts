#!/usr/bin/env node
// Agents' links of a saved graph (links-proposed.ts): propose one with quoted evidence, remove one, confirm one as a
// second reviewer, or list them. The tool verifies every quote before it stores a link.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  LinkRefused,
  proposeLink,
  readProposed,
  removeLink,
  reviewLink,
} from '../links-proposed.ts';

const USAGE = `usage: octocode-graph link <result-dir> add '<json>' | add -      (the link as JSON, - reads stdin)
       octocode-graph link <result-dir> remove <id>
       octocode-graph link <result-dir> review <id> --by <reviewer>
       octocode-graph link <result-dir> list
  A link: {"relation":"documents|configures|relates","source":"<file|file#symbol>","target":"<file|file#symbol>",
  "claim":"<what it says>","anchors":[{"path":"<file>","line":N,"endLine":N,"quote":"<text at those lines>"}],
  "proposedBy":"<agent>"}. Each endpoint's file needs an anchor; each quote must be at its lines and occur once in its
  file. Find candidates with: octocode-graph query <result-dir> "links --unresolved". Prints one JSON line; exit 1
  when the link is refused (the reason: citation-file, citation-line, quote-mismatch, quote-not-unique,
  unknown-endpoint, missing-anchor, invalid), 2 on a usage error.`;

const print = (value: unknown) =>
  process.stdout.write(`${JSON.stringify(value)}\n`);

function main(argv: string[]) {
  if (argv.includes('--help') || argv.includes('-h')) {
    process.stdout.write(`${USAGE}\n`);
    return 0;
  }
  const [dir, action, ...rest] = argv;
  if (!dir || !action) {
    process.stderr.write(`${USAGE}\n`);
    return 2;
  }
  const out = resolve(dir);
  try {
    if (action === 'add' && rest.length === 1) {
      const text = rest[0] === '-' ? readFileSync(0, 'utf8') : rest[0];
      print(proposeLink(out, JSON.parse(text)));
    } else if (action === 'remove' && rest.length === 1) {
      removeLink(out, rest[0]);
      print({ removed: rest[0] });
    } else if (action === 'review' && rest.length === 3 && rest[1] === '--by')
      print(reviewLink(out, rest[0], rest[2]));
    else if (action === 'list' && !rest.length) print(readProposed(out));
    else {
      process.stderr.write(`${USAGE}\n`);
      return 2;
    }
    return 0;
  } catch (error) {
    if (error instanceof LinkRefused || error instanceof SyntaxError) {
      print({
        refused: error instanceof LinkRefused ? error.reason : 'invalid',
        error: error.message,
      });
      return 1;
    }
    throw error;
  }
}

process.exitCode = main(process.argv.slice(2));
