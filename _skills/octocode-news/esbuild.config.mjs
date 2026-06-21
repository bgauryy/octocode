#!/usr/bin/env node

import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { build } from "esbuild";
import { minify } from "html-minifier-terser";

const ROOT_DIR = path.dirname(fileURLToPath(import.meta.url));
const SRC_DIR = path.join(ROOT_DIR, "src");
const OUT_DIR = path.join(ROOT_DIR, "scripts");
const TEMPLATE_ENTRYPOINTS = ["report-template.html"];
const TEMPLATE_ASSETS = ["report-template.css"];
const ENTRYPOINTS = [
  "build-report.ts",
  "catalog-sources.ts",
  "check-rss.ts",
  "fetch-rss.ts",
  "report-schema.ts",
  "rss-core.ts",
  "shared.ts"
];
const CLI_ENTRYPOINTS = ["build-report.ts", "catalog-sources.ts", "check-rss.ts", "fetch-rss.ts"];
const SUPPORTED_SOURCE_EXTENSIONS = new Set([".ts", ".html", ".css"]);

async function listDirectFiles(dirPath) {
  const entries = await fs.readdir(dirPath, { withFileTypes: true });
  return entries
    .filter((entry) => entry.isFile())
    .map((entry) => entry.name)
    .sort();
}

function expectedOutputForSource(sourceFile) {
  if (sourceFile.endsWith(".ts")) {
    return sourceFile.replace(/\.ts$/, ".mjs");
  }
  if (sourceFile.endsWith(".html")) {
    return sourceFile;
  }
  if (sourceFile.endsWith(".css")) {
    return null;
  }
  throw new Error(`Unsupported source file: ${sourceFile}`);
}

async function validateSourceCoverage() {
  const sourceFiles = await listDirectFiles(SRC_DIR);
  const unsupportedSourceFiles = sourceFiles.filter(
    (sourceFile) => !SUPPORTED_SOURCE_EXTENSIONS.has(path.extname(sourceFile))
  );

  if (unsupportedSourceFiles.length) {
    throw new Error(
      `Unsupported src files found. Only .ts and .html are buildable: ${unsupportedSourceFiles.join(", ")}`
    );
  }

  const missingTsEntrypoints = sourceFiles.filter(
    (sourceFile) => sourceFile.endsWith(".ts") && !ENTRYPOINTS.includes(sourceFile)
  );
  if (missingTsEntrypoints.length) {
    throw new Error(
      `Every src TypeScript file must be listed in ENTRYPOINTS. Missing: ${missingTsEntrypoints.join(", ")}`
    );
  }

  const missingTemplateEntrypoints = sourceFiles.filter(
    (sourceFile) => sourceFile.endsWith(".html") && !TEMPLATE_ENTRYPOINTS.includes(sourceFile)
  );
  if (missingTemplateEntrypoints.length) {
    throw new Error(
      `Every src HTML file must be listed in TEMPLATE_ENTRYPOINTS. Missing: ${missingTemplateEntrypoints.join(", ")}`
    );
  }

  const missingTemplateAssets = sourceFiles.filter(
    (sourceFile) => sourceFile.endsWith(".css") && !TEMPLATE_ASSETS.includes(sourceFile)
  );
  if (missingTemplateAssets.length) {
    throw new Error(
      `Every src CSS file must be listed in TEMPLATE_ASSETS. Missing: ${missingTemplateAssets.join(", ")}`
    );
  }

  const declaredSources = [...ENTRYPOINTS, ...TEMPLATE_ENTRYPOINTS, ...TEMPLATE_ASSETS].sort();
  const unknownDeclaredSources = declaredSources.filter(
    (sourceFile) => !sourceFiles.includes(sourceFile)
  );
  if (unknownDeclaredSources.length) {
    throw new Error(
      `Declared build sources do not exist in src/: ${unknownDeclaredSources.join(", ")}`
    );
  }

  return sourceFiles;
}

async function resetOutputDir() {
  await fs.rm(OUT_DIR, { recursive: true, force: true });
  await fs.mkdir(OUT_DIR, { recursive: true });
}

async function validateBuiltOutputs(sourceFiles) {
  const actualOutputs = await listDirectFiles(OUT_DIR);
  const expectedOutputs = sourceFiles.map(expectedOutputForSource).filter(Boolean).sort();

  const missingOutputs = expectedOutputs.filter(
    (outputFile) => !actualOutputs.includes(outputFile)
  );
  const unexpectedOutputs = actualOutputs.filter(
    (outputFile) => !expectedOutputs.includes(outputFile)
  );

  if (missingOutputs.length || unexpectedOutputs.length) {
    const parts = [];
    if (missingOutputs.length) {
      parts.push(`missing outputs: ${missingOutputs.join(", ")}`);
    }
    if (unexpectedOutputs.length) {
      parts.push(`unexpected outputs: ${unexpectedOutputs.join(", ")}`);
    }
    throw new Error(`scripts/ build output does not match src/ coverage (${parts.join("; ")})`);
  }

  return {
    expectedOutputs,
    actualOutputs
  };
}

async function bundleEntryPoints(entrypoints, { banner } = {}) {
  if (!entrypoints.length) return;

  await build({
    entryPoints: entrypoints.map((entry) => path.join(SRC_DIR, entry)),
    outdir: OUT_DIR,
    outbase: SRC_DIR,
    bundle: true,
    charset: "utf8",
    format: "esm",
    legalComments: "none",
    minify: true,
    platform: "node",
    target: "node22",
    logLevel: "info",
    outExtension: {
      ".js": ".mjs"
    },
    ...(banner ? { banner: { js: banner } } : {})
  });
}

async function buildTemplate() {
  const templateName = TEMPLATE_ENTRYPOINTS[0];
  const templatePath = path.join(SRC_DIR, templateName);
  const outputPath = path.join(OUT_DIR, templateName);
  let templateHtml = await fs.readFile(templatePath, "utf8");

  if (templateHtml.includes("__INLINE_CSS__")) {
    const cssPath = templatePath.replace(/\.html$/, ".css");
    const css = await fs.readFile(cssPath, "utf8");
    templateHtml = templateHtml.replace("__INLINE_CSS__", css);
  }

  const minifiedHtml = await minify(templateHtml, {
    collapseBooleanAttributes: true,
    collapseWhitespace: true,
    keepClosingSlash: true,
    minifyCSS: true,
    minifyJS: true,
    removeComments: true,
    removeRedundantAttributes: false,
    sortAttributes: true,
    sortClassName: true
  });
  await fs.writeFile(outputPath, minifiedHtml.trim() + "\n", "utf8");
}

async function main() {
  const sourceFiles = await validateSourceCoverage();
  await resetOutputDir();
  await bundleEntryPoints(CLI_ENTRYPOINTS, { banner: "#!/usr/bin/env node" });
  await bundleEntryPoints(ENTRYPOINTS.filter((entry) => !CLI_ENTRYPOINTS.includes(entry)));
  await buildTemplate();
  const outputCheck = await validateBuiltOutputs(sourceFiles);

  console.log(
    JSON.stringify(
      {
        builtAt: new Date().toISOString(),
        sourceDir: SRC_DIR,
        outputDir: OUT_DIR,
        sourceFiles,
        cliEntrypoints: CLI_ENTRYPOINTS,
        moduleEntrypoints: ENTRYPOINTS.filter((entry) => !CLI_ENTRYPOINTS.includes(entry)),
        template: TEMPLATE_ENTRYPOINTS[0],
        outputFiles: outputCheck.actualOutputs
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
