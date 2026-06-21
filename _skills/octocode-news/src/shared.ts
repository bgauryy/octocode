import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";

export const SECTION_DOMAIN_MAP: Record<string, string> = {
  "A — AI": "ai",
  "B — Devtools": "devtools",
  "C — Web Platform": "web",
  "D — Security": "security",
  "E — Repos & Releases": "repos",
  "Cross-Domain Discovery": "cross"
};

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
export const DEFAULT_SOURCES = path.resolve(SCRIPT_DIR, "..", "references", "sources.md");

export function expandUser(filePath: string): string {
  if (!filePath) return filePath;
  if (filePath === "~") return os.homedir();
  if (filePath.startsWith("~/")) return path.join(os.homedir(), filePath.slice(2));
  return filePath;
}

export function resolvePath(filePath: string): string {
  return path.resolve(expandUser(filePath));
}

export function sectionToDomain(section: string, fallback = "cross"): string {
  if (!section) return fallback;
  if (/^Cross[-\s]Domain/i.test(section)) return "cross";
  for (const [label, domain] of Object.entries(SECTION_DOMAIN_MAP)) {
    if (section === label || section.startsWith(label)) return domain;
  }
  return fallback;
}

export async function mapConcurrent<T, R>(
  items: T[],
  concurrency: number,
  fn: (item: T, index: number) => Promise<R>
): Promise<R[]> {
  const results = new Array<R>(items.length);
  let cursor = 0;
  async function worker() {
    while (true) {
      const idx = cursor++;
      if (idx >= items.length) return;
      results[idx] = await fn(items[idx], idx);
    }
  }
  await Promise.all(Array.from({ length: Math.min(concurrency, items.length) }, () => worker()));
  return results;
}
