import fs from "node:fs/promises";
import path from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

import {
  normalizeReportData,
  mergeFromSectionDir,
  splitNormalizedReport,
  PRIMARY_DOMAIN_ORDER,
  OPTIONAL_DOMAIN_ORDER
} from "./report-schema.ts";
import { resolvePath } from "./shared.ts";

const execFileAsync = promisify(execFile);
const META_PLACEHOLDER = "__REPORT_META__";
const CSS_PLACEHOLDER = "__INLINE_CSS__";
const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_TEMPLATE = path.join(SCRIPT_DIR, "report-template.html");
const ALL_DOMAIN_IDS = [...PRIMARY_DOMAIN_ORDER, ...OPTIONAL_DOMAIN_ORDER];

function printUsage() {
  console.log(`Usage:
  Single-file mode (backwards compatible):
    node scripts/build-report.mjs --input <report.json> --output <report.html> [options]

  Section-dir mode (parallel workflow):
    node scripts/build-report.mjs --section-dir <dir> --output <report.html> [options]

    The directory must contain meta.json and one {domainId}.json per section:
      meta.json, ai.json, devtools.json, web.json, security.json, repos.json [, cross.json]

Options:
  --json-out <path>            Write validated merged JSON (default: derived from --output)
  --template <path>            Custom HTML template
  --open                       Open the HTML in the default browser
  --require-full-content       Enforce full-page content evidence on every item

Examples:
  node scripts/build-report.mjs \\
    --section-dir ~/tmp/20260401-120000-sections/ \\
    --json-out ~/tmp/20260401-120000-whats-new.json \\
    --output ~/tmp/20260401-120000-whats-new.html \\
    --require-full-content --open

  node scripts/build-report.mjs \\
    --input ~/tmp/20260401-120000-whats-new.raw.json \\
    --output ~/tmp/20260401-120000-whats-new.html \\
    --require-full-content --open
`);
}

function defaultJsonPath(outputPath) {
  const ext = path.extname(outputPath);
  if (ext === ".html" || ext === ".htm") {
    return outputPath.slice(0, -ext.length) + ".json";
  }
  return outputPath + ".json";
}

function parseArgs(argv) {
  const args = {
    input: "",
    sectionDir: "",
    output: "",
    jsonOut: "",
    template: DEFAULT_TEMPLATE,
    open: false,
    requireFullContent: false,
    help: false
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--help" || arg === "-h") {
      args.help = true;
      continue;
    }
    if (arg === "--open") {
      args.open = true;
      continue;
    }
    if (arg === "--require-full-content") {
      args.requireFullContent = true;
      continue;
    }
    if (arg === "--input") {
      args.input = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg === "--section-dir") {
      args.sectionDir = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg === "--output") {
      args.output = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg === "--json-out") {
      args.jsonOut = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg === "--template") {
      args.template = argv[index + 1] || "";
      index += 1;
      continue;
    }

    throw new Error(`Unknown argument: ${arg}`);
  }

  return args;
}

function collectItems(report) {
  return [...report.topItems, ...report.sections.flatMap((section) => section.items)];
}

function validateFullContentEvidence(report) {
  const failures = [];
  const items = collectItems(report);

  const tldrLength = report.tldr.trim().length;
  if (tldrLength < 120) {
    failures.push(`tldr is too short (${tldrLength} chars). Write a fuller executive summary.`);
  }

  for (const item of items) {
    const evidence = item.contentEvidence;
    if (!evidence) {
      failures.push(`${item.title}: missing contentEvidence`);
      continue;
    }
    const whyImportant = item.whyImportant || item.whyInteresting;
    if (!whyImportant || !whyImportant.trim()) {
      failures.push(`${item.title}: missing whyImportant`);
    }
    if (!Array.isArray(item.references) || item.references.length === 0) {
      failures.push(`${item.title}: missing references`);
    }
    if (evidence.method !== "full-page") {
      failures.push(`${item.title}: contentEvidence.method must be "full-page"`);
    }

    if (!Number.isInteger(evidence.chars) || evidence.chars < 200) {
      failures.push(`${item.title}: contentEvidence.chars must be >= 200`);
    }

    const summaryLength = item.summary.trim().length;
    if (summaryLength < 120) {
      failures.push(`${item.title}: summary is too short (${summaryLength} chars)`);
    }
  }

  if (!Array.isArray(report.sourcesChecked) || report.sourcesChecked.length === 0) {
    failures.push("sourcesChecked must include at least one recorded source.");
  }

  if (failures.length > 0) {
    throw new Error(
      `Full-content validation failed (${failures.length} validation issues):\n- ${failures.join("\n- ")}`
    );
  }
}

async function openInDefaultBrowser(filePath) {
  if (process.platform === "darwin") {
    await execFileAsync("open", [filePath]);
    return;
  }
  if (process.platform === "win32") {
    await execFileAsync("cmd", ["/c", "start", "", filePath]);
    return;
  }
  await execFileAsync("xdg-open", [filePath]);
}

function safeJsonEmbed(value: unknown) {
  return JSON.stringify(value, null, 2).trim().replace(/</g, "\\u003c");
}

function injectSections(html: string, normalized: ReturnType<typeof normalizeReportData>) {
  const { meta, sectionMap } = splitNormalizedReport(normalized);

  if (!html.includes(META_PLACEHOLDER)) {
    throw new Error(`Template placeholder ${META_PLACEHOLDER} not found.`);
  }

  html = html.replace(META_PLACEHOLDER, () => safeJsonEmbed(meta));

  for (const id of ALL_DOMAIN_IDS) {
    const placeholder = `__SECTION_${id.toUpperCase()}__`;
    const section = sectionMap.get(id);
    html = html.replace(placeholder, () => (section ? safeJsonEmbed(section) : "null"));
  }

  return html;
}

async function readJsonFile(filePath: string) {
  const raw = await fs.readFile(filePath, "utf8");
  try {
    return JSON.parse(raw);
  } catch (error) {
    throw new Error(`Could not parse JSON at ${filePath}: ${error.message}`);
  }
}

async function loadFromSectionDir(dirPath: string) {
  const metaPath = path.join(dirPath, "meta.json");
  const meta = await readJsonFile(metaPath);

  const sectionFiles: Array<Record<string, unknown>> = [];
  for (const id of ALL_DOMAIN_IDS) {
    const filePath = path.join(dirPath, `${id}.json`);
    try {
      const data = await readJsonFile(filePath);
      sectionFiles.push(data);
    } catch {
      if (PRIMARY_DOMAIN_ORDER.includes(id as (typeof PRIMARY_DOMAIN_ORDER)[number])) {
        throw new Error(`Required section file missing: ${filePath}`);
      }
    }
  }

  return mergeFromSectionDir(meta, sectionFiles);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printUsage();
    return;
  }

  if (!args.input && !args.sectionDir) {
    throw new Error("Provide either --input <file> or --section-dir <dir>.");
  }
  if (args.input && args.sectionDir) {
    throw new Error("Use --input or --section-dir, not both.");
  }
  if (!args.output) {
    throw new Error("Missing required --output path.");
  }

  const outputPath = resolvePath(args.output);
  const templatePath = resolvePath(args.template || DEFAULT_TEMPLATE);
  const jsonOutPath = resolvePath(args.jsonOut || defaultJsonPath(outputPath));

  let merged: Record<string, unknown>;
  if (args.sectionDir) {
    merged = await loadFromSectionDir(resolvePath(args.sectionDir));
  } else {
    merged = await readJsonFile(resolvePath(args.input));
  }

  const normalized = normalizeReportData(merged);
  if (args.requireFullContent) {
    validateFullContentEvidence(normalized);
  }

  const prettyJson = JSON.stringify(normalized, null, 2) + "\n";
  await fs.mkdir(path.dirname(jsonOutPath), { recursive: true });
  await fs.writeFile(jsonOutPath, prettyJson, "utf8");

  await fs.mkdir(path.dirname(outputPath), { recursive: true });
  let html = await fs.readFile(templatePath, "utf8");

  if (html.includes(CSS_PLACEHOLDER)) {
    const cssPath = templatePath.replace(/\.html$/, ".css");
    const css = await fs.readFile(cssPath, "utf8");
    html = html.replace(CSS_PLACEHOLDER, () => css);
  }

  html = injectSections(html, normalized);
  await fs.writeFile(outputPath, html, "utf8");

  if (args.open) {
    await openInDefaultBrowser(outputPath);
  }

  console.log(
    JSON.stringify(
      {
        mode: args.sectionDir ? "section-dir" : "single-file",
        input: args.sectionDir ? resolvePath(args.sectionDir) : resolvePath(args.input),
        json: jsonOutPath,
        html: outputPath,
        opened: args.open
      },
      null,
      2
    )
  );
}

main().catch((error) => {
  console.error(error.message || String(error));
  process.exitCode = 1;
});
