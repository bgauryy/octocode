import fs from "node:fs/promises";
import path from "node:path";

import {
  analyzeFreshness,
  extractFeedTitle,
  looksLikeFeed,
  parseFeedCatalog,
  readResponseBody
} from "./rss-core.ts";
import { DEFAULT_SOURCES, mapConcurrent, resolvePath } from "./shared.ts";

function printUsage() {
  console.log(`Usage:
  node skills/whats-new/scripts/check-rss.mjs [--sources <sources.md>] [--json-out <report.json>] [--timeout-ms <ms>] [--concurrency <n>] [--failures-only] [--window-label <24h|7d|14d|30d>]

NOTE: For standard discovery runs, prefer "fetch-rss --include-health" which combines
item fetching and health checking in a single network pass. This standalone script
remains useful for targeted feed audits and debugging.

What it does:
  1. Reads RSS/Atom URLs from sources.md tables, fenced RSS blocks, and repo release-feed patterns.
  2. Fetches them concurrently.
  3. Checks whether the response looks like a real feed.
  4. Optionally validates freshness by selected window label.
  5. Prints a summary and optionally writes a JSON report.
`);
}

function parseArgs(argv) {
  const args = {
    sources: DEFAULT_SOURCES,
    jsonOut: "",
    timeoutMs: 15000,
    concurrency: 6,
    failuresOnly: false,
    windowLabel: "",
    help: false
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--help" || arg === "-h") {
      args.help = true;
      continue;
    }
    if (arg === "--failures-only") {
      args.failuresOnly = true;
      continue;
    }
    if (arg === "--sources") {
      args.sources = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg === "--json-out") {
      args.jsonOut = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg === "--timeout-ms") {
      args.timeoutMs = Number(argv[index + 1] || 0);
      index += 1;
      continue;
    }
    if (arg === "--concurrency") {
      args.concurrency = Number(argv[index + 1] || 0);
      index += 1;
      continue;
    }
    if (arg === "--window-label") {
      args.windowLabel = (argv[index + 1] || "").trim();
      index += 1;
      continue;
    }

    throw new Error(`Unknown argument: ${arg}`);
  }

  if (!Number.isFinite(args.timeoutMs) || args.timeoutMs <= 0) {
    throw new Error("Expected --timeout-ms to be a positive number.");
  }
  if (!Number.isInteger(args.concurrency) || args.concurrency <= 0) {
    throw new Error("Expected --concurrency to be a positive integer.");
  }
  if (args.windowLabel && !["24h", "7d", "14d", "30d"].includes(args.windowLabel)) {
    throw new Error('Expected --window-label to be one of: "24h", "7d", "14d", "30d".');
  }

  return args;
}

async function checkFeed(feed, options) {
  const startedAt = Date.now();

  try {
    const response = await fetch(feed.url, {
      headers: {
        "user-agent": "whats-new-rss-check/1.0",
        accept:
          "application/atom+xml, application/rss+xml, application/xml, text/xml;q=0.9, */*;q=0.1"
      },
      redirect: "follow",
      signal: AbortSignal.timeout(options.timeoutMs)
    });

    const sample = await readResponseBody(response, 1024 * 1024);
    const contentType = response.headers.get("content-type") || "";
    const validFeed = response.ok && looksLikeFeed(contentType, sample);
    const title = validFeed ? extractFeedTitle(sample).slice(0, 160) : "";
    const freshness = validFeed ? analyzeFreshness(sample, options.windowLabel) : null;
    const freshnessRequired = Boolean(options.windowLabel);
    const emptyFeed = Boolean(validFeed && freshness && freshness.entryCount === 0);
    const undatedFailure = Boolean(
      freshnessRequired && freshness && freshness.entryCount > 0 && freshness.datedEntryCount === 0
    );
    const freshnessFailed = Boolean(
      freshnessRequired && freshness && freshness.datedEntryCount > 0 && !freshness.fresh
    );
    const isOk = validFeed && !emptyFeed && !undatedFailure && !freshnessFailed;

    return {
      ...feed,
      ok: isOk,
      status: response.status,
      contentType,
      finalUrl: response.url,
      title,
      reason: isOk
        ? "ok"
        : emptyFeed
          ? "Feed returned no entries."
          : undatedFailure
            ? "Feed has entries but no parseable item dates."
            : freshnessFailed
              ? `No entries found inside ${options.windowLabel}.`
              : response.ok
                ? "Response did not look like RSS/Atom/XML."
                : `HTTP ${response.status}`,
      windowLabel: options.windowLabel || "",
      freshnessRequired,
      entryCount: freshness ? freshness.entryCount : 0,
      datedEntryCount: freshness ? freshness.datedEntryCount : 0,
      undatedEntryCount: freshness ? freshness.undatedEntryCount : 0,
      recentCount: freshness ? freshness.recentCount : 0,
      newestEntryAt: freshness ? freshness.newestEntryAt : "",
      fresh: freshness ? freshness.fresh : false,
      freshnessMode: freshness ? freshness.freshnessMode : "invalid",
      durationMs: Date.now() - startedAt
    };
  } catch (error) {
    const causeMessage =
      error && typeof error === "object" && error.cause && typeof error.cause.message === "string"
        ? ` (${error.cause.message})`
        : "";

    return {
      ...feed,
      ok: false,
      status: null,
      contentType: "",
      finalUrl: feed.url,
      title: "",
      reason: `${error.name}: ${error.message}${causeMessage}`,
      windowLabel: options.windowLabel || "",
      freshnessRequired: Boolean(options.windowLabel),
      entryCount: 0,
      datedEntryCount: 0,
      undatedEntryCount: 0,
      recentCount: 0,
      newestEntryAt: "",
      fresh: false,
      freshnessMode: "error",
      durationMs: Date.now() - startedAt
    };
  }
}

function buildSummary(results) {
  const passed = results.filter((result) => result.ok);
  const failed = results.filter((result) => !result.ok);
  const empty = results.filter((result) => result.reason === "Feed returned no entries.");
  const stale = results.filter(
    (result) =>
      result.freshnessRequired &&
      result.status !== null &&
      result.reason.startsWith("No entries found inside")
  );
  const undated = results.filter(
    (result) =>
      result.freshnessRequired && result.reason === "Feed has entries but no parseable item dates."
  );
  const bySection = new Map();

  for (const result of results) {
    const current = bySection.get(result.section) || { total: 0, passed: 0, failed: 0 };
    current.total += 1;
    current.passed += result.ok ? 1 : 0;
    current.failed += result.ok ? 0 : 1;
    bySection.set(result.section, current);
  }

  return {
    total: results.length,
    passed: passed.length,
    failed: failed.length,
    empty: empty.length,
    stale: stale.length,
    undated: undated.length,
    sections: Array.from(bySection.entries()).map(([section, counts]) => ({
      section,
      ...counts
    }))
  };
}

function formatResult(result) {
  const marker = result.ok ? "PASS" : "FAIL";
  const status = result.status === null ? "ERR" : String(result.status);
  const titlePart = result.title ? ` | ${result.title}` : "";
  const freshnessPart = result.freshnessRequired
    ? ` | recent=${result.recentCount} newest=${result.newestEntryAt || "n/a"}`
    : "";
  return `${marker} [${status}] ${result.section} :: ${result.url}${titlePart}${freshnessPart}${result.ok ? "" : ` | ${result.reason}`}`;
}

async function maybeWriteJsonReport(filePath, payload) {
  if (!filePath) {
    return;
  }

  const resolved = resolvePath(filePath);
  await fs.mkdir(path.dirname(resolved), { recursive: true });
  await fs.writeFile(resolved, JSON.stringify(payload, null, 2) + "\n", "utf8");
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printUsage();
    return;
  }

  const sourcesPath = resolvePath(args.sources || DEFAULT_SOURCES);
  const markdown = await fs.readFile(sourcesPath, "utf8");
  const feeds = parseFeedCatalog(markdown);

  if (feeds.length === 0) {
    throw new Error(`No RSS feed URLs found in ${sourcesPath}.`);
  }

  const results = await mapConcurrent(feeds, args.concurrency, (feed) => checkFeed(feed, args));
  const summary = buildSummary(results);
  const filteredResults = args.failuresOnly ? results.filter((result) => !result.ok) : results;

  console.log(`Checked ${summary.total} feed URLs from ${sourcesPath}`);
  console.log(`Passed: ${summary.passed} | Failed: ${summary.failed}`);
  if (args.windowLabel) {
    console.log(
      `Freshness window: ${args.windowLabel} | Empty feeds: ${summary.empty} | Stale feeds: ${summary.stale} | Undated feeds: ${summary.undated}`
    );
  }
  for (const section of summary.sections) {
    console.log(`- ${section.section}: ${section.passed}/${section.total} passed`);
  }
  console.log("");
  for (const result of filteredResults) {
    console.log(formatResult(result));
  }

  await maybeWriteJsonReport(args.jsonOut, {
    checkedAt: new Date().toISOString(),
    sourcesPath,
    summary,
    results
  });

  if (summary.failed > 0) {
    process.exitCode = 1;
  }
}

main().catch((error) => {
  console.error(error.message || String(error));
  process.exitCode = 1;
});
