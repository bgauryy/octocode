import { sectionToDomain } from "./shared.ts";

export const WINDOW_OFFSETS = {
  "24h": 24 * 60 * 60 * 1000,
  "48h": 48 * 60 * 60 * 1000,
  "7d": 7 * 24 * 60 * 60 * 1000,
  "14d": 14 * 24 * 60 * 60 * 1000,
  "30d": 30 * 24 * 60 * 60 * 1000
};

export function parseFeedCatalog(markdown) {
  const lines = markdown.split(/\r?\n/);
  const feeds = [];
  const seen = new Set();
  let currentSection = "Unknown";
  let currentSubsection = "";
  let awaitingFence = false;
  let insideFeedBlock = false;
  let rssColIdx = -1;
  let pastSeparator = false;

  function extractUrl(value) {
    const clean = String(value || "")
      .replace(/`/g, "")
      .trim();

    const markdownLinkMatch = clean.match(/\((https?:\/\/[^\s)]+)\)/);
    if (markdownLinkMatch) {
      return markdownLinkMatch[1];
    }

    const urlMatch = clean.match(/https?:\/\/\S+/);
    if (urlMatch) {
      return urlMatch[0].replace(/[),.;]+$/, "");
    }

    if (/^[a-z0-9][\w.-]+\.[a-z]{2,}(\/\S*)?$/.test(clean)) {
      return `https://${clean}`;
    }

    return "";
  }

  function addFeed(url, lineNum) {
    if (!url) return;
    const dedupeKey = `${currentSection}::${url}`;
    if (seen.has(dedupeKey)) return;
    seen.add(dedupeKey);
    feeds.push({
      section: currentSection,
      domain: sectionToDomain(currentSection),
      url,
      line: lineNum
    });
  }

  function extractRepoIds(value) {
    return [...String(value || "").matchAll(/\b([A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+)\b/g)].map(
      (match) => match[1]
    );
  }

  function releaseFeedUrl(repoId) {
    return `https://github.com/${repoId}/releases.atom`;
  }

  lines.forEach((line, index) => {
    const trimmed = line.trim();

    const sectionMatch = line.match(/^##\s+(.+)$/);
    if (sectionMatch) {
      currentSection = sectionMatch[1].trim();
      currentSubsection = "";
      rssColIdx = -1;
      pastSeparator = false;
    }

    const subsectionMatch = line.match(/^###\s+(.+)$/);
    if (subsectionMatch) {
      currentSubsection = subsectionMatch[1].trim();
      rssColIdx = -1;
      pastSeparator = false;
    }

    if (/^### RSS Feeds/.test(trimmed)) {
      awaitingFence = true;
      insideFeedBlock = false;
      return;
    }

    if (awaitingFence && /^```/.test(trimmed)) {
      awaitingFence = false;
      insideFeedBlock = true;
      return;
    }

    if (insideFeedBlock && /^```/.test(trimmed)) {
      insideFeedBlock = false;
      return;
    }

    if (insideFeedBlock) {
      if (!trimmed.startsWith("#")) {
        addFeed(extractUrl(trimmed), index + 1);
      }
      return;
    }

    if (trimmed.startsWith("|")) {
      const cells = trimmed
        .replace(/^\|/, "")
        .replace(/\|$/, "")
        .split("|")
        .map((c) => c.trim());

      const idx = cells.findIndex((c) => /^RSS$/i.test(c.replace(/`/g, "").trim()));
      if (idx >= 0) {
        rssColIdx = idx;
        pastSeparator = false;
        return;
      }

      if (rssColIdx >= 0 && !pastSeparator && cells.every((c) => /^[-:\s]+$/.test(c))) {
        pastSeparator = true;
        return;
      }

      if (rssColIdx >= 0 && pastSeparator && cells.length > rssColIdx) {
        const rssCell = cells[rssColIdx].replace(/`/g, "").trim();
        if (rssCell && rssCell !== "—" && rssCell !== "-") {
          addFeed(extractUrl(rssCell), index + 1);
        }
      }
      return;
    }

    if (rssColIdx >= 0) {
      rssColIdx = -1;
      pastSeparator = false;
    }

    if (/^Release Feed Pattern/i.test(currentSubsection)) {
      for (const repoId of extractRepoIds(line)) {
        addFeed(releaseFeedUrl(repoId), index + 1);
      }
    }
  });

  return feeds;
}

function stripCdata(value) {
  return String(value || "").replace(/<!\[CDATA\[([\s\S]*?)]]>/g, "$1");
}

function stripTags(value) {
  return stripCdata(value).replace(/<[^>]*>/g, " ");
}

function decodeEntities(value) {
  return String(value || "")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&#x27;/g, "'")
    .replace(/&#(\d+);/g, (_, n) => String.fromCharCode(Number(n)));
}

function clean(value) {
  return decodeEntities(stripTags(value)).replace(/\s+/g, " ").trim();
}

export function looksLikeFeed(contentType, xml) {
  if (/(application|text)\/(atom\+xml|rss\+xml|xml)/i.test(contentType || "")) {
    return true;
  }

  return /<(rss|feed|rdf:rdf)(\s|>)/i.test(xml) || /<(entry|item)(\s|>)/i.test(xml);
}

export function extractFeedTitle(xml) {
  const match = String(xml || "").match(/<title[^>]*>([\s\S]*?)<\/title>/i);
  return match ? clean(match[1]).slice(0, 200) : "";
}

export function extractEntryBlocks(xml) {
  return String(xml || "").match(/<(item|entry)(\s|>)[\s\S]*?<\/(item|entry)>/gi) || [];
}

export function extractTagValue(block, tagName) {
  const escaped = String(tagName || "").replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const regex = new RegExp(`<${escaped}(\\s[^>]*)?>([\\s\\S]*?)<\\/${escaped}>`, "i");
  const match = String(block || "").match(regex);
  return match ? clean(match[2]) : "";
}

export function extractLink(block) {
  const linkTags = [...String(block || "").matchAll(/<link\b([^>]*)\/?>/gi)];
  const candidates = linkTags
    .map((match) => {
      const attrs = match[1] || "";
      const hrefMatch = attrs.match(/\bhref=["']([^"']+)["']/i);
      if (!hrefMatch || !hrefMatch[1]) return null;
      const relMatch = attrs.match(/\brel=["']([^"']+)["']/i);
      return {
        href: hrefMatch[1],
        rel: (relMatch ? relMatch[1] : "").toLowerCase()
      };
    })
    .filter(Boolean);

  const preferredLink =
    candidates.find((candidate) => !candidate.rel || candidate.rel === "alternate") ||
    candidates.find((candidate) => candidate.rel !== "self") ||
    candidates[0];

  if (preferredLink) {
    return preferredLink.href;
  }

  const linkContent = extractTagValue(block, "link");
  if (linkContent && linkContent.startsWith("http")) {
    return linkContent;
  }

  const guid = extractTagValue(block, "guid");
  return guid && guid.startsWith("http") ? guid : "";
}

export function parseDateValue(value) {
  const timestamp = Date.parse(String(value || ""));
  return Number.isNaN(timestamp) ? null : new Date(timestamp);
}

export function formatDateISO(date) {
  return date.toISOString().slice(0, 10);
}

export function formatDateDisplay(date) {
  const months = [
    "Jan",
    "Feb",
    "Mar",
    "Apr",
    "May",
    "Jun",
    "Jul",
    "Aug",
    "Sep",
    "Oct",
    "Nov",
    "Dec"
  ];
  return `${months[date.getMonth()]} ${date.getDate()}`;
}

export function extractItems(xml, feedUrl, feedTitle, domain) {
  return extractEntryBlocks(xml).flatMap((block) => {
    const title = extractTagValue(block, "title");
    if (!title) {
      return [];
    }

    const link = extractLink(block);
    const dateValue =
      extractTagValue(block, "pubDate") ||
      extractTagValue(block, "updated") ||
      extractTagValue(block, "published") ||
      extractTagValue(block, "dc:date");
    const date = parseDateValue(dateValue);
    const description =
      extractTagValue(block, "description") ||
      extractTagValue(block, "summary") ||
      extractTagValue(block, "content") ||
      extractTagValue(block, "content:encoded") ||
      "";

    return [
      {
        title: title.slice(0, 300),
        link: link || feedUrl,
        date: date ? formatDateDisplay(date) : "",
        dateISO: date ? formatDateISO(date) : "",
        dateTs: date ? date.getTime() : null,
        hasDate: Boolean(date),
        description: description.slice(0, 500),
        source: feedTitle || feedUrl,
        feedUrl,
        domain
      }
    ];
  });
}

export async function readResponseBody(response, maxBytes = 1024 * 1024) {
  if (!response.body) {
    return "";
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let totalBytes = 0;
  let text = "";

  try {
    while (totalBytes < maxBytes) {
      const { done, value } = await reader.read();
      if (done) {
        break;
      }

      const remaining = maxBytes - totalBytes;
      const chunk = value.byteLength > remaining ? value.subarray(0, remaining) : value;
      totalBytes += chunk.byteLength;
      text += decoder.decode(chunk, { stream: true });

      if (chunk.byteLength < value.byteLength) {
        break;
      }
    }
  } finally {
    try {
      await reader.cancel();
    } catch {
    void 0;
  }
  }

  text += decoder.decode();
  return text;
}

function windowCutoff(windowLabel, now = new Date()) {
  if (!windowLabel || !WINDOW_OFFSETS[windowLabel]) {
    return null;
  }

  return new Date(now.getTime() - WINDOW_OFFSETS[windowLabel]);
}

export function analyzeFreshness(xml, windowLabel, now = new Date()) {
  const blocks = extractEntryBlocks(xml);
  const cutoff = windowCutoff(windowLabel, now);
  const datedEntries = [];
  let undatedEntryCount = 0;

  for (const block of blocks) {
    const dateValue =
      extractTagValue(block, "pubDate") ||
      extractTagValue(block, "updated") ||
      extractTagValue(block, "published") ||
      extractTagValue(block, "dc:date");
    const date = parseDateValue(dateValue);
    if (date) {
      datedEntries.push(date);
    } else {
      undatedEntryCount += 1;
    }
  }

  const newestDate = datedEntries.length
    ? new Date(Math.max(...datedEntries.map((date) => date.getTime())))
    : null;
  const recentCount = cutoff
    ? datedEntries.filter((date) => date.getTime() >= cutoff.getTime()).length
    : datedEntries.length;
  const freshnessMode = !windowLabel
    ? "not-requested"
    : datedEntries.length === 0 && blocks.length > 0
      ? "undated"
      : recentCount > 0
        ? "recent"
        : "stale";

  return {
    entryCount: blocks.length,
    datedEntryCount: datedEntries.length,
    undatedEntryCount,
    recentCount,
    newestEntryAt: newestDate ? newestDate.toISOString() : "",
    fresh: !windowLabel || recentCount > 0,
    freshnessMode
  };
}
