#!/usr/bin/env node
import { runScrapingTool } from './scraping-bridge.mjs';
process.exitCode = runScrapingTool(
  'har-ingest',
  process.argv.slice(2),
  'Usage: har-ingest-to-scrape.mjs [--scraping-skill-dir <dir>] --session-dir <dir> (--har <file.har> | --from-cdp-dir <run>) [har-ingest options]\n\nOptional dependency: octocode-scraping. Pass its folder with --scraping-skill-dir.',
  'The optional octocode-scraping skill is required for HAR ingestion. Pass --scraping-skill-dir <dir>.'
);
