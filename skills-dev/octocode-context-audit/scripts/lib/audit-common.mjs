// Shared state and helpers for the context audit: budgets, token estimate, the findings
// sink, and tolerant file readers.
import fs from "node:fs";
import os from "node:os";

export const HOME = os.homedir();

// Budgets are chars (≈ 4 chars per token). They mark review points, not errors.
export const BUDGET = {
  instructionFile: 8_000,
  instructionTotal: 20_000,
  memoryIndex: 10_000,
  skillDescription: 1_024,
  skillDescriptionsTotal: 12_000,
  mcpInstructions: 4_000,
  mcpToolDescription: 1_000,
  mcpToolSchema: 8_000,
  mcpServerTotal: 60_000,
};
export const tokens = (chars) => Math.round(chars / 4);

export const findings = [];
export const finding = (severity, area, subject, message, evidence = {}) =>
  findings.push({ severity, area, subject, message, ...evidence });

export const readText = (file) => {
  try {
    return fs.readFileSync(file, "utf8");
  } catch {
    return null;
  }
};
export const readJson = (file) => {
  const text = readText(file);
  if (text == null) return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
};

export function projectSlug(dir) {
  return dir.replace(/[^A-Za-z0-9]/g, "-");
}
