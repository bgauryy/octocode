import fs from "node:fs/promises";
import path from "node:path";

import { DEFAULT_SOURCES, resolvePath, sectionToDomain } from "./shared.ts";

const DOMAIN_ORDER = ["cross", "ai", "devtools", "web", "security", "repos"];
const CUSTOM_RESOURCE_SUBSECTION_PATTERNS = [
  /^Community/i,
  /^Optional Aggregators/i,
  /^Validation/i,
  /^Framework-specific/i,
  /^Tier 3/i,
  /^Release Feed Pattern/i,
  /^Validation Tools/i
];

function parseArgs(argv) {
  const args = {
    sources: DEFAULT_SOURCES,
    jsonOut: "",
    domains: "",
    help: false
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--help" || arg === "-h") {
      args.help = true;
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
    if (arg === "--domains") {
      args.domains = argv[index + 1] || "";
      index += 1;
      continue;
    }
    throw new Error(`Unknown argument: ${arg}`);
  }

  return args;
}

function printUsage() {
  console.log(`Usage:
  node skills/whats-new/scripts/catalog-sources.mjs [--sources <sources.md>] [--json-out <catalog.json>] [--domains <ai,devtools,...>]

What it does:
  1. Parses the whats-new sources catalog.
  2. Extracts websites, RSS feeds, validation repos, and custom resources.
  3. Outputs a machine-readable checklist for full coverage research runs.`);
}

function cleanCell(value) {
  return String(value || "")
    .replace(/<br\s*\/?>/gi, " ")
    .replace(/`/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

function extractUrl(value) {
  const clean = String(value || "")
    .replace(/`/g, "")
    .trim();
  const markdownLink = clean.match(/\((https?:\/\/[^\s)]+)\)/);
  if (markdownLink) return markdownLink[1];
  const direct = clean.match(/https?:\/\/[^\s|)]+/);
  if (direct) return direct[0].replace(/[),.;]+$/, "");
  if (/^[a-z0-9][\w.-]+\.[a-z]{2,}(\/\S*)?$/.test(clean)) {
    return `https://${clean}`;
  }
  return "";
}

function parseMarkdownTable(blockLines) {
  if (blockLines.length < 2) return [];

  const rows = blockLines.map((line) =>
    line
      .trim()
      .replace(/^\|/, "")
      .replace(/\|$/, "")
      .split("|")
      .map((cell) => cleanCell(cell))
  );

  const headers = rows[0];
  const body = rows.slice(2).filter((cells) => cells.some((cell) => cell));

  return body.map((cells) =>
    headers.reduce((acc, header, index) => {
      acc[header] = cells[index] || "";
      return acc;
    }, {})
  );
}

function parseRepoEntries(lines) {
  const joined = lines.join("\n");
  const matches = joined.match(/`([^`]+)`/g) || [];
  const repos = [];
  const seen = new Set();

  for (const match of matches) {
    const value = match.replace(/`/g, "").trim();
    if (extractUrl(value)) continue;
    if (looksLikeFeedPath(value)) continue;
    if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(value)) continue;
    if (seen.has(value)) continue;
    seen.add(value);
    repos.push({ repo: value });
  }

  return repos;
}

function looksLikeFeedPath(value) {
  const normalized = String(value || "")
    .trim()
    .toLowerCase();
  if (!normalized.includes("/")) return false;
  if (!/\.(xml|atom|rss|json)$/.test(normalized)) return false;

  const [firstSegment] = normalized.split("/", 1);
  return ["daily", "weekly", "monthly"].includes(firstSegment);
}

function extractRepoId(value) {
  const clean = String(value || "")
    .replace(/`/g, "")
    .trim();
  if (extractUrl(clean)) return "";
  const match = clean.match(/\b([A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+)\b/);
  return match ? match[1] : "";
}

function releaseFeedUrl(repoId) {
  return `https://github.com/${repoId}/releases.atom`;
}

function parseCustomResources(lines) {
  return lines
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith("**"))
    .map((line) => line.replace(/^-\s+/, "").replace(/\*\*/g, "").trim())
    .filter((line) => {
      const stripped = line
        .replace(/`[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+`/g, "")
        .replace(/[,\s]/g, "");
      return Boolean(stripped);
    })
    .filter(Boolean)
    .map((label) => ({ label }));
}

function looksLikeCustomResourceSubsection(subsection) {
  return CUSTOM_RESOURCE_SUBSECTION_PATTERNS.some((pattern) => pattern.test(subsection || ""));
}

function ensureDomainBucket(catalog, section, domain) {
  if (!domain) return null;
  if (!catalog.has(domain)) {
    catalog.set(domain, {
      id: domain,
      section,
      websites: [],
      rssFeeds: [],
      repos: [],
      customResources: []
    });
  }
  return catalog.get(domain);
}

function pushUnique(list, item, uniqueKey) {
  if (!item) return;
  const key = uniqueKey(item);
  if (list.some((entry) => uniqueKey(entry) === key)) return;
  list.push(item);
}

function flushTableBlock(catalog, tableLines, section, subsection) {
  if (!tableLines.length) return;
  const domain = sectionToDomain(section, "");
  const bucket = ensureDomainBucket(catalog, section, domain);
  if (!bucket) return;

  const rows = parseMarkdownTable(tableLines);
  for (const row of rows) {
    const url = extractUrl(row.URL || row.Url || row.Link || "");
    const repoId = extractRepoId(row.Repo || row.Source || row.Name || "");
    const name = row.Source || row.Repo || row.Name || url || repoId;
    const notes = [row.Notes, row.Coverage].filter(Boolean).join(" ").trim();

    if (url) {
      pushUnique(
        bucket.websites,
        { name, url, notes, subsection: subsection || "Websites" },
        (entry) => `${entry.name}::${entry.url}`
      );
    }

    const rssRaw = cleanCell(row.RSS || "");
    if (rssRaw && rssRaw !== "—" && rssRaw !== "-") {
      const rssUrl = extractUrl(rssRaw);
      if (rssUrl) {
        pushUnique(
          bucket.rssFeeds,
          {
            name: name || rssUrl,
            url: rssUrl,
            websiteUrl: url || undefined,
            subsection: subsection || "Websites"
          },
          (entry) => entry.url
        );
      }
    }

    if (!url && repoId) {
      pushUnique(
        bucket.repos,
        { repo: repoId, notes, subsection: subsection || "Repos" },
        (entry) => entry.repo
      );
    }
  }
}

function flushFencedBlock(catalog, fenceLines, section, subsection) {
  if (!fenceLines.length || !/^RSS Feeds/i.test(subsection || "")) return;
  const domain = sectionToDomain(section, "");
  const bucket = ensureDomainBucket(catalog, section, domain);
  if (!bucket) return;

  for (const line of fenceLines) {
    const url = extractUrl(line);
    if (!url) continue;
    pushUnique(
      bucket.rssFeeds,
      {
        url,
        subsection: subsection || "RSS Feeds"
      },
      (entry) => entry.url
    );
  }
}

function flushTextBlock(catalog, textLines, section, subsection) {
  if (!textLines.length) return;
  const domain = sectionToDomain(section, "");
  const bucket = ensureDomainBucket(catalog, section, domain);
  if (!bucket) return;

  const repos = parseRepoEntries(textLines);
  for (const repo of repos) {
    pushUnique(bucket.repos, { ...repo, subsection: subsection || "Repos" }, (entry) => entry.repo);
  }

  if (/^Release Feed Pattern/i.test(subsection || "")) {
    for (const repo of repos) {
      pushUnique(
        bucket.rssFeeds,
        {
          name: repo.repo,
          url: releaseFeedUrl(repo.repo),
          websiteUrl: `https://github.com/${repo.repo}`,
          subsection: subsection || "Release Feed Pattern"
        },
        (entry) => entry.url
      );
    }
  }

  if (/^Repos/i.test(subsection || "")) {
    return;
  }

  if (
    /^Custom Resources/i.test(subsection || "") ||
    looksLikeCustomResourceSubsection(subsection)
  ) {
    for (const resource of parseCustomResources(textLines)) {
      pushUnique(
        bucket.customResources,
        { ...resource, subsection: subsection || "Custom Resources" },
        (entry) => entry.label
      );
    }
  }
}

function parseCatalog(markdown) {
  const lines = markdown.split(/\r?\n/);
  const catalog = new Map();
  let currentSection = "";
  let currentSubsection = "";
  let tableLines = [];
  let textLines = [];
  let fenceLines = [];
  let insideFence = false;

  function flushOpenBlocks() {
    flushTableBlock(catalog, tableLines, currentSection, currentSubsection);
    flushTextBlock(catalog, textLines, currentSection, currentSubsection);
    flushFencedBlock(catalog, fenceLines, currentSection, currentSubsection);
    tableLines = [];
    textLines = [];
    fenceLines = [];
  }

  for (const line of lines) {
    const trimmed = line.trim();
    const sectionMatch = line.match(/^##\s+(.+)$/);
    const subsectionMatch = line.match(/^###\s+(.+)$/);

    if (sectionMatch) {
      flushOpenBlocks();
      currentSection = sectionMatch[1].trim();
      currentSubsection = "";
      continue;
    }

    if (subsectionMatch) {
      flushOpenBlocks();
      currentSubsection = subsectionMatch[1].trim();
      continue;
    }

    if (/^```/.test(trimmed)) {
      if (insideFence) {
        insideFence = false;
      } else {
        flushTableBlock(catalog, tableLines, currentSection, currentSubsection);
        flushTextBlock(catalog, textLines, currentSection, currentSubsection);
        tableLines = [];
        textLines = [];
        insideFence = true;
      }
      continue;
    }

    if (insideFence) {
      fenceLines.push(line);
      continue;
    }

    if (trimmed.startsWith("|")) {
      tableLines.push(line);
      continue;
    }

    if (tableLines.length && !trimmed.startsWith("|")) {
      flushTableBlock(catalog, tableLines, currentSection, currentSubsection);
      tableLines = [];
    }

    if (!trimmed || trimmed === "---") {
      continue;
    }

    textLines.push(line);
  }

  flushOpenBlocks();

  return DOMAIN_ORDER.map((domain) => catalog.get(domain))
    .filter(Boolean)
    .map((entry) => ({
      ...entry,
      counts: {
        websites: entry.websites.length,
        rssFeeds: entry.rssFeeds.length,
        repos: entry.repos.length,
        customResources: entry.customResources.length,
        total:
          entry.websites.length +
          entry.rssFeeds.length +
          entry.repos.length +
          entry.customResources.length
      }
    }));
}

function buildSummary(domains) {
  const byDomain = {};
  let totalWebsites = 0;
  let totalRssFeeds = 0;
  let totalRepos = 0;
  let totalCustomResources = 0;

  for (const domain of domains) {
    byDomain[domain.id] = domain.counts;
    totalWebsites += domain.counts.websites;
    totalRssFeeds += domain.counts.rssFeeds;
    totalRepos += domain.counts.repos;
    totalCustomResources += domain.counts.customResources;
  }

  return {
    totalDomains: domains.length,
    totalWebsites,
    totalRssFeeds,
    totalRepos,
    totalCustomResources,
    totalResources: totalWebsites + totalRssFeeds + totalRepos + totalCustomResources,
    byDomain
  };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printUsage();
    return;
  }

  const sourcesPath = resolvePath(args.sources);
  const markdown = await fs.readFile(sourcesPath, "utf8");
  const allDomains = parseCatalog(markdown);
  const requestedDomains = new Set(
    String(args.domains || "")
      .split(",")
      .map((value) => value.trim())
      .filter(Boolean)
  );

  const domains = requestedDomains.size
    ? allDomains.filter((domain) => requestedDomains.has(domain.id))
    : allDomains;

  const output = {
    generatedAt: new Date().toISOString(),
    sourcesPath,
    summary: buildSummary(domains),
    domains
  };

  if (args.jsonOut) {
    const outPath = resolvePath(args.jsonOut);
    await fs.mkdir(path.dirname(outPath), { recursive: true });
    await fs.writeFile(outPath, JSON.stringify(output, null, 2) + "\n", "utf8");
    console.log(`Catalog written: ${outPath}`);
  } else {
    console.log(JSON.stringify(output, null, 2));
  }
}

main().catch((error) => {
  console.error(error.message || String(error));
  process.exitCode = 1;
});
