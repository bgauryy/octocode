#!/usr/bin/env node
import { runScrapingTool } from './scraping-bridge.mjs';
process.exitCode = runScrapingTool(
  'corpus-run',
  process.argv.slice(2),
  'Usage: corpus-run-local.mjs [--scraping-skill-dir <dir>] [corpus-run options]\n\nOptional dependency: octocode-scraping. Pass its folder with --scraping-skill-dir.',
  'The optional octocode-scraping skill is required for corpus queries. Pass --scraping-skill-dir <dir>.'
);
