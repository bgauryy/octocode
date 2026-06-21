import fs from "node:fs/promises";
import path from "node:path";

import {
  WINDOW_OFFSETS,
  analyzeFreshness,
  extractFeedTitle,
  extractItems,
  looksLikeFeed,
  parseFeedCatalog,
  readResponseBody
} from "./rss-core.ts";
import { DEFAULT_SOURCES, mapConcurrent, resolvePath } from "./shared.ts";

function parseArgs(argv) {
  const args = {
    sources: DEFAULT_SOURCES,
    window: "7d",
    jsonOut: "",
    timeoutMs: 20000,
    concurrency: 8,
    includeHealth: false,
    help: false
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--help" || a === "-h") {
      args.help = true;
      continue;
    }
    if (a === "--sources") {
      args.sources = argv[++i] || "";
      continue;
    }
    if (a === "--window") {
      args.window = argv[++i] || "7d";
      continue;
    }
    if (a === "--json-out") {
      args.jsonOut = argv[++i] || "";
      continue;
    }
    if (a === "--timeout-ms") {
      args.timeoutMs = Number(argv[++i] || 20000);
      continue;
    }
    if (a === "--concurrency") {
      args.concurrency = Number(argv[++i] || 8);
      continue;
    }
    if (a === "--include-health") {
      args.includeHealth = true;
      continue;
    }
    throw new Error(`Unknown argument: ${a}`);
  }
  if (!WINDOW_OFFSETS[args.window]) {
    throw new Error(`Invalid --window: ${args.window}. Use 24h|48h|7d|14d|30d.`);
  }
  if (!Number.isFinite(args.timeoutMs) || args.timeoutMs <= 0) {
    throw new Error("Expected --timeout-ms to be a positive number.");
  }
  if (!Number.isInteger(args.concurrency) || args.concurrency <= 0) {
    throw new Error("Expected --concurrency to be a positive integer.");
  }
  return args;
}

async function fetchFeed(feed, opts) {
  const start = Date.now();
  try {
    const res = await fetch(feed.url, {
      headers: {
        "user-agent": "whats-new-rss-fetcher/1.0",
        accept:
          "application/atom+xml, application/rss+xml, application/xml, text/xml;q=0.9, */*;q=0.1"
      },
      redirect: "follow",
      signal: AbortSignal.timeout(opts.timeoutMs)
    });

    const contentType = res.headers.get("content-type") || "";

    if (!res.ok) {
      const base = {
        ...feed,
        status: `http-${res.status}`,
        items: [],
        feedTitle: "",
        finalUrl: res.url || feed.url,
        durationMs: Date.now() - start
      };
      if (opts.includeHealth) {
        base.health = { ok: false, reason: `HTTP ${res.status}`, contentType };
      }
      return base;
    }

    const xml = await readResponseBody(res, 1024 * 1024);
    const feedTitle = extractFeedTitle(xml);
    const finalUrl = res.url || feed.url;
    const items = extractItems(xml, finalUrl, feedTitle, feed.domain);

    const result = {
      ...feed,
      status: "ok",
      items,
      feedTitle,
      finalUrl,
      durationMs: Date.now() - start
    };

    if (opts.includeHealth) {
      const validFeed = looksLikeFeed(contentType, xml);
      const freshness = validFeed ? analyzeFreshness(xml, opts.window) : null;
      const reason = !validFeed
        ? "Not a valid feed"
        : freshness && !freshness.fresh
          ? `Stale (no entries in ${opts.window})`
          : "ok";
      result.health = {
        ok: validFeed && (freshness ? freshness.fresh : true),
        validFeed,
        contentType,
        reason,
        ...(freshness || {})
      };
    }

    return result;
  } catch (err) {
    const base = {
      ...feed,
      status: err.name === "TimeoutError" ? "timeout" : "error",
      items: [],
      feedTitle: "",
      finalUrl: feed.url,
      error: err.message,
      durationMs: Date.now() - start
    };
    if (opts.includeHealth) {
      base.health = { ok: false, reason: err.message };
    }
    return base;
  }
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    console.log(`Usage:
  node fetch-rss.mjs --window <24h|7d|14d|30d> --json-out <output.json> [--sources <sources.md>] [--timeout-ms <ms>] [--concurrency <n>] [--include-health]

Fetches ALL RSS feeds from sources.md, extracts items within the time window, and outputs structured JSON for the research agent.
With --include-health, each feed summary includes health-check data (valid feed, freshness, stale/broken status), making a separate check-rss run optional.`);
    return;
  }

  const sourcesPath = resolvePath(args.sources);
  const markdown = await fs.readFile(sourcesPath, "utf8");
  const feeds = parseFeedCatalog(markdown);

  if (feeds.length === 0) throw new Error(`No RSS feeds found in ${sourcesPath}.`);

  const now = new Date();
  const cutoffMs = now.getTime() - WINDOW_OFFSETS[args.window];
  const cutoffDate = new Date(cutoffMs);

  console.log(
    `Fetching ${feeds.length} RSS feeds (window: ${args.window}, cutoff: ${cutoffDate.toISOString()})...`
  );

  const results = await mapConcurrent(feeds, args.concurrency, (feed) => fetchFeed(feed, args));

  const allItems = [];
  const feedSummaries = [];
  const byDomain: Record<string, number> = {};
  const seenLinks = new Set();
  let totalUndatedItemsExcluded = 0;

  for (const r of results) {
    const windowItems = r.items.filter((item) => {
      return Number.isFinite(item.dateTs) && item.dateTs >= cutoffMs;
    });
    const undatedItems = r.items.filter((item) => !Number.isFinite(item.dateTs)).length;
    totalUndatedItemsExcluded += undatedItems;

    feedSummaries.push({
      url: r.url,
      finalUrl: r.finalUrl,
      domain: r.domain,
      section: r.section,
      feedTitle: r.feedTitle,
      status: r.status,
      totalItems: r.items.length,
      datedItems: r.items.length - undatedItems,
      undatedItems,
      windowItems: windowItems.length,
      durationMs: r.durationMs,
      error: r.error || undefined,
      ...(r.health ? { health: r.health } : {})
    });

    for (const item of windowItems) {
      const key = item.link || item.title;
      if (seenLinks.has(key)) continue;
      seenLinks.add(key);
      allItems.push(item);
      byDomain[item.domain] = (byDomain[item.domain] || 0) + 1;
    }
  }

  allItems.sort((a, b) => (b.dateTs || 0) - (a.dateTs || 0));

  const okFeeds = results.filter((r) => r.status === "ok").length;
  const failedFeeds = results.filter((r) => r.status !== "ok").length;

  const output = {
    fetchedAt: now.toISOString(),
    window: args.window,
    cutoff: cutoffDate.toISOString(),
    summary: {
      totalFeeds: feeds.length,
      successfulFeeds: okFeeds,
      failedFeeds,
      totalItemsInWindow: allItems.length,
      totalUndatedItemsExcluded,
      byDomain
    },
    feeds: feedSummaries,
    items: allItems.map((item) => {
      const { dateTs, hasDate, ...rest } = item;
      void dateTs;
      void hasDate;
      return rest;
    })
  };

  console.log(`\nDone: ${okFeeds}/${feeds.length} feeds OK, ${failedFeeds} failed`);
  console.log(`Items in window: ${allItems.length}`);
  for (const [dom, count] of Object.entries(byDomain).sort((a, b) => b[1] - a[1])) {
    console.log(`  ${dom}: ${count}`);
  }

  if (args.jsonOut) {
    const outPath = resolvePath(args.jsonOut);
    await fs.mkdir(path.dirname(outPath), { recursive: true });
    await fs.writeFile(outPath, JSON.stringify(output, null, 2) + "\n", "utf8");
    console.log(`\nJSON written: ${outPath}`);
  } else {
    console.log(JSON.stringify(output, null, 2));
  }
}

main().catch((err) => {
  console.error(err.message || String(err));
  process.exitCode = 1;
});
