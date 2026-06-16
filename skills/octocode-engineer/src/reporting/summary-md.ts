import fs from 'node:fs';
import path from 'node:path';

import { isTestFile } from '../common/utils.js';
import { PILLAR_CATEGORIES, SEVERITY_ORDER } from '../types/index.js';

import type { ReportAnalysisSummary } from './analysis.js';
import type {
  AgentOutputData,
  FileEntry,
  Finding,
  FindingStats,
  HotFile,
  ScanSummaryData,
} from '../types/index.js';

const CATEGORY_PILLAR_MAP: Record<string, string> = Object.entries(
  PILLAR_CATEGORIES
).reduce<Record<string, string>>((acc, [pillar, categories]) => {
  for (const category of categories) acc[category] = pillar;
  return acc;
}, {});

export function severityBreakdown(findings: Finding[]): Record<string, number> {
  const counts: Record<string, number> = {
    critical: 0,
    high: 0,
    medium: 0,
    low: 0,
    info: 0,
  };
  for (const f of findings) counts[f.severity] = (counts[f.severity] || 0) + 1;
  return counts;
}

export function categoryBreakdown(findings: Finding[]): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const f of findings) counts[f.category] = (counts[f.category] || 0) + 1;
  return counts;
}

export function computeHealthScore(
  findings: Finding[],
  totalFiles: number
): number {
  return computeHealthScoreFromSeverityBreakdown(
    severityBreakdown(findings),
    totalFiles
  );
}

function computeHealthScoreFromSeverityBreakdown(
  breakdown: Record<string, number>,
  totalFiles: number
): number {
  if (totalFiles === 0) return 100;
  const weights = { critical: 25, high: 10, medium: 3, low: 1, info: 0 };
  let penalty = 0;
  for (const [severity, count] of Object.entries(breakdown)) {
    penalty += (weights[severity as keyof typeof weights] || 0) * count;
  }
  const weightedFindingsPerFile = penalty / totalFiles;
  const rawScore = Math.max(
    0,
    Math.min(100, Math.round(100 / (1 + weightedFindingsPerFile / 10)))
  );
  if (penalty === 0) return rawScore;
  if ((breakdown.critical ?? 0) > 0) return Math.min(rawScore, 95);
  if ((breakdown.high ?? 0) > 0) return Math.min(rawScore, 98);
  return Math.min(rawScore, 99);
}

export function collectTagCloud(
  findings: Finding[]
): { tag: string; count: number }[] {
  const tagCounts = new Map<string, number>();
  for (const f of findings) {
    if (!f.tags) continue;
    for (const tag of f.tags) {
      tagCounts.set(tag, (tagCounts.get(tag) || 0) + 1);
    }
  }
  return [...tagCounts.entries()]
    .map(([tag, count]) => ({ tag, count }))
    .sort((a, b) => b.count - a.count);
}

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function summarizeActiveFeatures(activeFeatures: Set<string>): string[] {
  const remaining = new Set(activeFeatures);
  const labels: string[] = [];

  for (const [pillar, categories] of Object.entries(PILLAR_CATEGORIES)) {
    if (categories.length > 0 && categories.every(cat => remaining.has(cat))) {
      labels.push(pillar);
      for (const cat of categories) remaining.delete(cat);
    }
  }

  return [...labels, ...[...remaining].sort()];
}

function isPillarActive(
  pillarKey: string,
  activeFeatures: Set<string> | null
): boolean {
  if (!activeFeatures) return true;
  const pillarCats = PILLAR_CATEGORIES[pillarKey] || [];
  return pillarCats.some(cat => activeFeatures.has(cat));
}

type FindingLike = Omit<Finding, 'id'> & { id?: string };

export function diversifyFindings<T extends FindingLike>(
  sorted: T[],
  limit: number
): T[] {
  if (!Number.isFinite(limit) || limit >= sorted.length) return sorted;

  const groups = new Map<string, T[]>();
  for (const f of sorted) {
    const cat = f.category;
    if (!groups.has(cat)) groups.set(cat, []);
    groups.get(cat)!.push(f);
  }

  const categoryOrder = [...groups.entries()].sort((a, b) => {
    const aTop = SEVERITY_ORDER[a[1][0].severity] ?? 0;
    const bTop = SEVERITY_ORDER[b[1][0].severity] ?? 0;
    return bTop - aTop;
  });

  const result: T[] = [];
  const cursors = new Map<string, number>();
  for (const [cat] of categoryOrder) cursors.set(cat, 0);

  while (result.length < limit) {
    let picked = false;
    for (const [cat, items] of categoryOrder) {
      if (result.length >= limit) break;
      const cursor = cursors.get(cat)!;
      if (cursor < items.length) {
        result.push(items[cursor]);
        cursors.set(cat, cursor + 1);
        picked = true;
      }
    }
    if (!picked) break;
  }
  return result;
}

export function diverseTopRecommendations(
  findings: Finding[],
  limit: number = 20,
  maxPerCategory: number = 2
): Finding[] {
  const result: Finding[] = [];
  const countByCategory = new Map<string, number>();
  for (const f of findings) {
    const catCount = countByCategory.get(f.category) || 0;
    if (catCount >= maxPerCategory) continue;
    result.push(f);
    countByCategory.set(f.category, catCount + 1);
    if (result.length >= limit) break;
  }
  return result;
}

export interface SummaryMdOptions {
  dir: string;
  report: import('./writer.js').FullReport;
  outputFiles: Record<string, string>;
  architectureFindings: Finding[];
  codeQualityFindings: Finding[];
  deadCodeFindings: Finding[];
  hotFiles?: import('../types/index.js').HotFile[];
  activeFeatures?: Set<string> | null;
  scope?: string[] | null;
  root?: string;
  scopeSymbols?: Map<string, string[]> | null;
  semanticEnabled?: boolean;
  securityFindings?: Finding[];
  testQualityFindings?: Finding[];
  reportAnalysis?: ReportAnalysisSummary;
  fileInventory?: FileEntry[];
}

function formatCliPath(filePath: string): string {
  return JSON.stringify(filePath.replace(/\\/g, '/'));
}

export function generateSummaryMd(opts: SummaryMdOptions): string {
  const {
    dir,
    report,
    outputFiles,
    architectureFindings,
    codeQualityFindings,
    deadCodeFindings,
    hotFiles = [],
    activeFeatures = null,
    scope = null,
    root = process.cwd(),
    scopeSymbols = null,
    semanticEnabled = false,
    securityFindings = [],
    testQualityFindings = [],
    reportAnalysis = null,
    fileInventory = report.fileInventory || [],
  } = opts;
  const allFindings = report.optimizationFindings || [];
  const summary: ScanSummaryData = report.summary;
  const agentOutput: AgentOutputData = report.agentOutput;
  const findingStats: FindingStats | null = agentOutput?.findingStats ?? null;
  const depGraph = report.dependencyGraph;
  const relativeScanDir = path.relative(root, dir) || '.';
  const exampleFileFilter = ((scope?.[0] ?? 'src/index').split(':')[0] || 'src/index')
    .replace(/\\/g, '/');
  const overallFindingStats = findingStats?.overall ?? {
    totalFindings: allFindings.length,
    severityBreakdown: severityBreakdown(allFindings),
  };

  const lines: string[] = [];
  lines.push('# Code Quality Scan Report\n');
  lines.push(`**Generated**: ${report.generatedAt}  `);
  lines.push(`**Root**: \`${report.repoRoot}\`\n`);

  lines.push('## Scan Scope\n');
  lines.push(`| Metric | Count |`);
  lines.push(`|--------|-------|`);
  lines.push(`| Files analyzed | ${summary.totalFiles ?? '—'} |`);
  lines.push(`| Functions | ${summary.totalFunctions ?? '—'} |`);
  lines.push(`| Flow nodes | ${summary.totalFlows ?? '—'} |`);
  lines.push(`| Dependency files | ${summary.totalDependencyFiles ?? '—'} |`);
  lines.push(`| Packages | ${summary.totalPackages ?? '—'} |`);
  lines.push('');

  lines.push('## Findings Overview\n');
  lines.push(`| Severity | Count |`);
  lines.push(`|----------|-------|`);
  lines.push(`| Critical | ${overallFindingStats.severityBreakdown.critical ?? 0} |`);
  lines.push(`| High | ${overallFindingStats.severityBreakdown.high ?? 0} |`);
  lines.push(`| Medium | ${overallFindingStats.severityBreakdown.medium ?? 0} |`);
  lines.push(`| Low | ${overallFindingStats.severityBreakdown.low ?? 0} |`);
  lines.push(`| **Total** | **${overallFindingStats.totalFindings}** |`);
  lines.push('');

  renderScanAnnotations(lines, {
    allFindings, overallFindingStats, agentOutput,
    activeFeatures, scope, root, scopeSymbols, semanticEnabled,
  });

  const renderPillarCategories = (
    pillarKey: string,
    findings: Finding[]
  ): void => {
    const breakdown = categoryBreakdown(findings);
    const pillarCats = PILLAR_CATEGORIES[pillarKey] || [];
    const isFiltered = activeFeatures !== null;
    for (const cat of pillarCats) {
      const count = breakdown[cat] || 0;
      const skipped = isFiltered && !activeFeatures!.has(cat);
      lines.push(skipped ? `- \`${cat}\`: — *(skipped)*` : `- \`${cat}\`: ${count}`);
    }
    lines.push('');
  };

  const totalFiles = summary.totalFiles || 1;
  const archStats = findingStats?.pillars?.['architecture'];
  const qualStats = findingStats?.pillars?.['code-quality'];
  const deadStats = findingStats?.pillars?.['dead-code'];
  const secStats = findingStats?.pillars?.['security'];
  const testStats = findingStats?.pillars?.['test-quality'];

  const pillarHealth = computePillarHealthScores(totalFiles, overallFindingStats, {
    archStats, qualStats, deadStats, secStats, testStats,
    architectureFindings, codeQualityFindings, deadCodeFindings,
    securityFindings, testQualityFindings,
  });

  const pushPillarSummary = buildPillarSummaryPusher(lines, activeFeatures, outputFiles);
  const qualityRating = computeQualityAspectRatings(allFindings, {
    fileInventory,
    hotFiles,
    reportAnalysis,
    includeTests: Boolean(
      (report.options as { includeTests?: boolean } | undefined)?.includeTests
    ),
  });

  renderHealthScores(lines, pillarHealth, activeFeatures);
  renderFeatureScores(
    lines,
    computeFeatureScores(allFindings, totalFiles, activeFeatures, { hotFiles })
  );
  renderQualityAspectRatings(lines, qualityRating);

  renderTagCloud(lines, allFindings);

  if (reportAnalysis) {
    renderAnalysisSignals(lines, reportAnalysis);
  }

  renderAgentInstructions(lines, outputFiles, allFindings);

  lines.push('## Architecture Health\n');
  pushPillarSummary(
    'architecture',
    archStats?.totalFindings ?? architectureFindings.length,
    pillarHealth.archHealth,
    'architecture',
    'architecture.json'
  );
  if (depGraph) {
    lines.push(`| Metric | Value |`);
    lines.push(`|--------|-------|`);
    lines.push(`| Modules | ${depGraph.totalModules} |`);
    lines.push(`| Import edges | ${depGraph.totalEdges} |`);
    lines.push(`| Cycles | ${depGraph.cycles?.length ?? 0} |`);
    lines.push(`| Critical paths | ${depGraph.criticalPaths?.length ?? 0} |`);
    lines.push(`| Root modules | ${depGraph.rootsCount} |`);
    lines.push(`| Leaf modules | ${depGraph.leavesCount} |`);
    lines.push(
      `| Test-only modules | ${depGraph.testOnlyModules?.length ?? 0} |`
    );
    lines.push(`| Unresolved imports | ${depGraph.unresolvedEdgeCount} |`);
    lines.push('');
  }
  renderPillarCategories('architecture', architectureFindings);

  renderHotspots(lines, hotFiles);

  renderPillarSections(lines, {
    architectureFindings,
    codeQualityFindings,
    deadCodeFindings,
    securityFindings,
    testQualityFindings,
    archStats,
    qualStats,
    deadStats,
    secStats,
    testStats,
    ...pillarHealth,
    activeFeatures,
    outputFiles,
    renderPillarCategories,
    pushPillarSummary,
  });

  renderRecommendations(lines, agentOutput);

  if (outputFiles.astTrees) {
    renderAstTreesSection(lines, dir, outputFiles, root, relativeScanDir, exampleFileFilter);
  }

  renderOutputFilesTable(lines, dir, outputFiles);

  if (report.parseErrors?.length > 0) {
    lines.push('## Parse Errors\n');
    lines.push(`${report.parseErrors.length} file(s) failed to parse:\n`);
    for (const err of report.parseErrors.slice(0, 10)) {
      lines.push(`- \`${err.file}\`: ${err.message}`);
    }
    lines.push('');
  }

  return lines.join('\n');
}

interface PillarHealthScores {
  overallHealth: number;
  archHealth: number;
  qualHealth: number;
  deadHealth: number;
  secHealth: number;
  testHealth: number;
}

export interface FeatureScoreRow {
  category: string;
  pillar: string;
  findings: number;
  affectedFiles: number;
  hotspotHits: number;
  hotspotMaxRisk: number;
  contextPenalty: number;
  severityBreakdown: Record<string, number>;
  score: number;
  grade: string;
}

export interface FeatureScoreContext {
  hotFiles?: import('../types/index.js').HotFile[];
}

export interface QualityAspectSignal {
  label: string;
  value: string;
  effect: 'positive' | 'negative' | 'neutral';
}

export interface QualityAspectRating {
  aspect: string;
  label: string;
  weight: number;
  score: number;
  grade: string;
  confidence: 'high' | 'medium' | 'low';
  rationale: string;
  signals: QualityAspectSignal[];
}

export interface QualityRatingSummary {
  model: string;
  overallScore: number;
  overallGrade: string;
  aspects: QualityAspectRating[];
}

export interface QualityAspectContext {
  fileInventory?: FileEntry[];
  hotFiles?: HotFile[];
  reportAnalysis?: ReportAnalysisSummary | null;
  includeTests?: boolean;
  includeGenerated?: boolean;
}

const SEVERITY_PRESSURE_WEIGHT: Record<Finding['severity'], number> = {
  critical: 1.0,
  high: 0.75,
  medium: 0.45,
  low: 0.2,
  info: 0.05,
};

const CATEGORY_PRESSURE_WEIGHT: Record<string, number> = {
  'dependency-critical-path': 0.45,
  'broker-module': 0.55,
  'bridge-module': 0.55,
  'distance-from-main-sequence': 0.6,
  'over-abstraction': 0.65,
  'concrete-dependency': 0.65,
  'move-to-caller': 0.35,
  'similar-function-body': 0.55,
  'dead-export': 0.65,
  'semantic-dead-export': 0.6,
};

function clampScore(score: number): number {
  return Math.max(0, Math.min(100, Math.round(score)));
}

function softPenalty(
  ratio: number,
  maxPenalty: number,
  sensitivity: number = 1.4
): number {
  if (ratio <= 0) return 0;
  return Math.round(maxPenalty * (1 - Math.exp(-ratio * sensitivity)));
}

function confidenceFromSample(sampleSize: number): 'high' | 'medium' | 'low' {
  if (sampleSize >= 25) return 'high';
  if (sampleSize >= 8) return 'medium';
  return 'low';
}

function average(values: number[]): number {
  if (values.length === 0) return 0;
  return values.reduce((acc, value) => acc + value, 0) / values.length;
}

function normalizeScanPath(filePath: string): string {
  return filePath.replace(/\\/g, '/');
}

function isGeneratedLikePath(filePath: string): boolean {
  const normalized = normalizeScanPath(filePath).toLowerCase();
  return (
    /(?:^|\/)(?:dist|build|coverage|out|vendor|vendors|generated|gen|\.cache)(?:\/|$)/.test(
      normalized
    ) ||
    /\.min\.(?:js|jsx|mjs|cjs|css)$/i.test(normalized) ||
    /\.bundle\./i.test(normalized)
  );
}

function shouldIncludeQualityPath(
  filePath: string,
  opts: { includeTests: boolean; includeGenerated: boolean }
): boolean {
  const normalized = normalizeScanPath(filePath);
  if (!opts.includeTests && isTestFile(normalized)) return false;
  if (!opts.includeGenerated && isGeneratedLikePath(normalized)) return false;
  return true;
}

function findingTouchesIncludedPath(
  finding: Finding,
  opts: { includeTests: boolean; includeGenerated: boolean }
): boolean {
  const referenced = new Set<string>();
  if (finding.file) referenced.add(finding.file);
  for (const file of finding.files || []) referenced.add(file);
  if (referenced.size === 0) return true;
  for (const file of referenced) {
    if (shouldIncludeQualityPath(file, opts)) return true;
  }
  return false;
}

function findingTouchesAnyFile(finding: Finding, files: Set<string>): boolean {
  if (finding.file && files.has(finding.file)) return true;
  return (finding.files || []).some(file => files.has(file));
}

function classifyFileNameStyle(baseName: string): string {
  if (/^[a-z0-9]+(?:-[a-z0-9]+)+$/.test(baseName)) return 'kebab';
  if (/^[a-z0-9]+(?:_[a-z0-9]+)+$/.test(baseName)) return 'snake';
  if (/^[a-z]+(?:[A-Z][a-z0-9]*)+$/.test(baseName)) return 'camel';
  if (/^[a-z0-9]+$/.test(baseName)) return 'flat';
  return 'other';
}

function fileUniverse(
  findings: Finding[],
  fileInventory: FileEntry[]
): Set<string> {
  const files = new Set<string>();
  for (const entry of fileInventory) files.add(entry.file);
  for (const finding of findings) {
    if (finding.file) files.add(finding.file);
    for (const file of finding.files || []) files.add(file);
  }
  return files;
}

function weightedFindingPressure(
  findings: Finding[],
  totalFiles: number,
  options: { applyCategoryWeight?: boolean } = {}
): number {
  if (findings.length === 0) return 0;
  const weightedCount = findings.reduce((sum, finding) => {
    const severityWeight = SEVERITY_PRESSURE_WEIGHT[finding.severity] ?? 0.2;
    const categoryWeight = options.applyCategoryWeight
      ? (CATEGORY_PRESSURE_WEIGHT[finding.category] ?? 1)
      : 1;
    return sum + severityWeight * categoryWeight;
  }, 0);
  return weightedCount / Math.max(1, totalFiles);
}

function stripSharedPathPrefix(filePaths: string[]): string[] {
  if (filePaths.length === 0) return [];
  const segments = filePaths.map(file =>
    normalizeScanPath(file).split('/').filter(Boolean)
  );
  let prefixLen = 0;
  while (true) {
    const token = segments[0][prefixLen];
    if (!token) break;
    if (segments.every(parts => parts[prefixLen] === token)) {
      prefixLen += 1;
      continue;
    }
    break;
  }

  return segments.map(parts => {
    const trimmed = parts.slice(prefixLen);
    if (trimmed.length > 0) return trimmed.join('/');
    return parts.join('/');
  });
}

export function computeQualityAspectRatings(
  findings: Finding[],
  context: QualityAspectContext = {}
): QualityRatingSummary {
  const includeTests = context.includeTests ?? false;
  const includeGenerated = context.includeGenerated ?? false;
  const filteringOptions = { includeTests, includeGenerated };
  const fileInventory = (context.fileInventory || []).filter(entry =>
    shouldIncludeQualityPath(entry.file, filteringOptions)
  );
  const hotFiles = (context.hotFiles || []).filter(entry =>
    shouldIncludeQualityPath(entry.file, filteringOptions)
  );
  const reportAnalysis = context.reportAnalysis || null;
  const filteredFindings = findings.filter(finding =>
    findingTouchesIncludedPath(finding, filteringOptions)
  );

  const files = fileUniverse(filteredFindings, fileInventory);
  const totalFiles = Math.max(1, files.size);
  const functions = fileInventory.flatMap(entry => entry.functions || []);
  const totalFunctions = Math.max(1, functions.length);

  const findingsByPillar = {
    architecture: filteredFindings.filter(
      finding => (CATEGORY_PILLAR_MAP[finding.category] || 'unmapped') === 'architecture'
    ),
    codeQuality: filteredFindings.filter(
      finding => (CATEGORY_PILLAR_MAP[finding.category] || 'unmapped') === 'code-quality'
    ),
    deadCode: filteredFindings.filter(
      finding => (CATEGORY_PILLAR_MAP[finding.category] || 'unmapped') === 'dead-code'
    ),
    testQuality: filteredFindings.filter(
      finding => (CATEGORY_PILLAR_MAP[finding.category] || 'unmapped') === 'test-quality'
    ),
  };

  const severeFindings = filteredFindings.filter(
    finding => finding.severity === 'critical' || finding.severity === 'high'
  );

  const aspects: QualityAspectRating[] = [];

  const architectureSevereFindings = findingsByPillar.architecture.filter(
    finding => finding.severity === 'critical' || finding.severity === 'high'
  );
  const architectureDensity = weightedFindingPressure(
    findingsByPillar.architecture,
    totalFiles,
    { applyCategoryWeight: true }
  );
  const architectureSevereDensity = weightedFindingPressure(
    architectureSevereFindings,
    totalFiles,
    { applyCategoryWeight: true }
  );
  const cycleDensity = weightedFindingPressure(
    findingsByPillar.architecture.filter(
      finding =>
        finding.category === 'dependency-cycle'
        || finding.category === 'cycle-cluster'
    ),
    totalFiles,
    { applyCategoryWeight: true }
  );
  const hotspotSample = [...hotFiles]
    .sort((a, b) => b.riskScore - a.riskScore)
    .slice(0, 8);
  const hotspotPressure = Math.min(
    1,
    average(hotspotSample.map(entry => Math.max(0, entry.riskScore))) / 100
  );
  const signalConfidenceBoost = reportAnalysis?.strongestGraphSignal?.confidence === 'high'
    ? 3
    : reportAnalysis?.strongestGraphSignal?.confidence === 'medium'
      ? 1
      : 0;
  const architectureScore = clampScore(
    100
    - softPenalty(architectureDensity, 24, 1.35)
    - softPenalty(architectureSevereDensity, 22, 1.9)
    - softPenalty(cycleDensity, 16, 2.2)
    - softPenalty(hotspotPressure, 12, 1.3)
    + signalConfidenceBoost
  );
  aspects.push({
    aspect: 'architecture-structure',
    label: 'Architecture & Structure',
    weight: 30,
    score: architectureScore,
    grade: gradeScore(architectureScore),
    confidence: confidenceFromSample(findingsByPillar.architecture.length),
    rationale:
      'Rates structural integrity using architecture findings, severity concentration, cycle pressure, and hotspot intensity, then tempers it with AI graph-signal confidence.',
    signals: [
      {
        label: 'Architecture findings / file',
        value: architectureDensity.toFixed(2),
        effect: findingsByPillar.architecture.length > 0 ? 'negative' : 'neutral',
      },
      {
        label: 'Severe architecture findings',
        value: architectureSevereDensity.toFixed(2),
        effect: architectureSevereFindings.length > 0 ? 'negative' : 'neutral',
      },
      {
        label: 'Hotspot pressure (avg risk top files)',
        value: average(hotspotSample.map(entry => entry.riskScore)).toFixed(1),
        effect: hotspotSample.length > 0 ? 'negative' : 'neutral',
      },
    ],
  });

  const filePaths = [...files];
  const topologyPaths = stripSharedPathPrefix(filePaths);
  const depthValues = topologyPaths.map(file =>
    normalizeScanPath(file).split('/').filter(Boolean).length
  );
  const avgDepth = average(depthValues);
  const topLevelCounts = new Map<string, number>();
  const folderCounts = new Map<string, number>();
  const dirSegments = new Set<string>();
  const vagueDirPattern = /^(util|utils|common|shared|misc|helper|helpers|tmp|temp)$/i;
  for (const file of topologyPaths) {
    const normalized = normalizeScanPath(file);
    const parts = normalized.split('/').filter(Boolean);
    const top = parts[0] || '.';
    const folder = parts.length <= 1 ? '.' : parts.slice(0, -1).join('/');
    topLevelCounts.set(top, (topLevelCounts.get(top) || 0) + 1);
    folderCounts.set(folder, (folderCounts.get(folder) || 0) + 1);
    for (const segment of parts.slice(0, -1)) dirSegments.add(segment);
  }
  const dominantRootRatio =
    topLevelCounts.size === 0
      ? 0
      : Math.max(...topLevelCounts.values()) / totalFiles;
  const vagueDirRatio =
    dirSegments.size === 0
      ? 0
      : [...dirSegments].filter(segment => vagueDirPattern.test(segment)).length
        / dirSegments.size;
  const maxFolderFileCount =
    folderCounts.size === 0 ? 0 : Math.max(...folderCounts.values());
  const maxFolderShare = maxFolderFileCount / totalFiles;
  const folderBloatPressure =
    maxFolderFileCount < 12 ? 0 : Math.max(0, maxFolderShare - 0.35);
  const bloatedFolderCount = [...folderCounts.values()].filter(
    count => count >= 20 || (count >= 12 && count / totalFiles >= 0.35)
  ).length;
  const folderBucketCount = Math.max(1, folderCounts.size);
  const bloatedFolderRatio = bloatedFolderCount / folderBucketCount;
  const folderScore = clampScore(
    100
    - softPenalty(Math.max(0, (avgDepth - 4) / 3), 20, 1.3)
    - softPenalty(Math.max(0, dominantRootRatio - 0.55), 18, 2.0)
    - softPenalty(vagueDirRatio, 24, 2.2)
    - softPenalty(folderBloatPressure, 18, 2.0)
    - softPenalty(bloatedFolderRatio, 14, 1.8)
  );
  aspects.push({
    aspect: 'folder-topology',
    label: 'Folder Topology',
    weight: 15,
    score: folderScore,
    grade: gradeScore(folderScore),
    confidence: confidenceFromSample(filePaths.length),
    rationale:
      'Rates how navigable the folder model is by blending depth balance, top-level concentration, leaf-folder bloat, and reliance on vague utility/common directories.',
    signals: [
      {
        label: 'Average path depth',
        value: avgDepth.toFixed(1),
        effect: avgDepth > 6 ? 'negative' : 'neutral',
      },
      {
        label: 'Dominant root share',
        value: `${Math.round(dominantRootRatio * 100)}%`,
        effect: dominantRootRatio > 0.65 ? 'negative' : 'neutral',
      },
      {
        label: 'Vague directory ratio',
        value: `${Math.round(vagueDirRatio * 100)}%`,
        effect: vagueDirRatio > 0.2 ? 'negative' : 'neutral',
      },
      {
        label: 'Largest folder file count',
        value: String(maxFolderFileCount),
        effect: maxFolderFileCount >= 20 ? 'negative' : 'neutral',
      },
      {
        label: 'Bloated folders',
        value: `${bloatedFolderCount}/${folderBucketCount}`,
        effect: bloatedFolderCount > 0 ? 'negative' : 'neutral',
      },
    ],
  });

  const genericNamePattern =
    /^(foo|bar|baz|tmp|temp|data|value|handler|util|helper|thing|stuff|fn|func)$/i;
  const genericFilePattern =
    /^(utils?|helpers?|common|shared|misc|tmp|temp|new|old|types?)$/i;
  const namedFunctions = functions
    .map(fn => fn.name || fn.nameHint || '')
    .filter(name => name.length > 0);
  const anonymousCount = namedFunctions.filter(
    name => name === '<anonymous>' || name === 'default'
  ).length;
  const explicitNamed = namedFunctions.filter(
    name => name !== '<anonymous>' && name !== 'default'
  );
  const genericFunctionCount = explicitNamed.filter(name =>
    genericNamePattern.test(name)
  ).length;
  const shortFunctionCount = explicitNamed.filter(name => name.length <= 2).length;
  const fileBaseNames = filePaths.map(file => path.basename(file, path.extname(file)));
  const genericFileCount = fileBaseNames.filter(name =>
    genericFilePattern.test(name)
  ).length;
  const namingScore = clampScore(
    100
    - softPenalty(anonymousCount / totalFunctions, 30, 2.3)
    - softPenalty(genericFunctionCount / Math.max(1, explicitNamed.length), 24, 2.0)
    - softPenalty(genericFileCount / totalFiles, 15, 1.8)
    - softPenalty(shortFunctionCount / Math.max(1, explicitNamed.length), 10, 1.8)
  );
  aspects.push({
    aspect: 'naming-quality',
    label: 'Naming Quality',
    weight: 15,
    score: namingScore,
    grade: gradeScore(namingScore),
    confidence: confidenceFromSample(functions.length),
    rationale:
      'Rates naming clarity by balancing anonymous/generic function names, short ambiguous names, and generic file basenames.',
    signals: [
      {
        label: 'Anonymous function share',
        value: `${Math.round((anonymousCount / totalFunctions) * 100)}%`,
        effect: anonymousCount > 0 ? 'negative' : 'neutral',
      },
      {
        label: 'Generic function names',
        value: String(genericFunctionCount),
        effect: genericFunctionCount > 0 ? 'negative' : 'neutral',
      },
      {
        label: 'Generic file names',
        value: String(genericFileCount),
        effect: genericFileCount > 0 ? 'negative' : 'neutral',
      },
    ],
  });

  const sharedPathPattern = /(^|\/)(common|shared|utils?|lib|core)(\/|$)/i;
  const sharedFiles = fileInventory.filter(entry =>
    sharedPathPattern.test(entry.file.replace(/\\/g, '/'))
  );
  const sharedFileSet = new Set(sharedFiles.map(entry => entry.file));
  const sharedFindings = filteredFindings.filter(finding =>
    findingTouchesAnyFile(finding, sharedFileSet)
  );
  const sharedSevere = sharedFindings.filter(
    finding => finding.severity === 'critical' || finding.severity === 'high'
  );
  const sharedImportPressure = average(
    sharedFiles.map(entry => {
      const internalImports =
        entry.symbolUsageSummary?.internalImportCount
        ?? entry.dependencyProfile.importedSymbols.filter(ref => !!ref.resolvedModule).length;
      const declaredExports =
        entry.symbolUsageSummary?.declaredExportCount
        ?? entry.dependencyProfile.declaredExports.length;
      return internalImports / (declaredExports + 1);
    })
  );
  const commonScore = sharedFiles.length === 0
    ? 88
    : clampScore(
      100
      - softPenalty(sharedFindings.length / sharedFiles.length, 24, 1.6)
      - softPenalty(sharedSevere.length / sharedFiles.length, 28, 2.2)
      - softPenalty(sharedImportPressure, 18, 1.5)
    );
  aspects.push({
    aspect: 'common-layer-health',
    label: 'Common/Shared Layer Health',
    weight: 15,
    score: commonScore,
    grade: gradeScore(commonScore),
    confidence: confidenceFromSample(sharedFiles.length),
    rationale:
      sharedFiles.length === 0
        ? 'No explicit common/shared layer was detected, so this aspect is neutral-positive by default.'
        : 'Rates whether shared/common code stays stable and lightweight by combining finding density, severe issue concentration, and internal dependency pressure.',
    signals: [
      {
        label: 'Shared files',
        value: String(sharedFiles.length),
        effect: sharedFiles.length === 0 ? 'neutral' : 'positive',
      },
      {
        label: 'Shared-layer findings',
        value: String(sharedFindings.length),
        effect: sharedFindings.length > 0 ? 'negative' : 'neutral',
      },
      {
        label: 'Shared import pressure',
        value: sharedImportPressure.toFixed(2),
        effect: sharedImportPressure > 1 ? 'negative' : 'neutral',
      },
    ],
  });

  const maintainabilityFindings = [
    ...findingsByPillar.codeQuality,
    ...findingsByPillar.deadCode,
    ...findingsByPillar.testQuality,
  ];
  const testDebtCategories = new Set([
    'test-no-assertion',
    'low-assertion-density',
    'excessive-mocking',
    'missing-test-cleanup',
    'focused-test',
    'fake-timer-no-restore',
    'missing-mock-restoration',
  ]);
  const testDebtFindings = filteredFindings.filter(finding =>
    testDebtCategories.has(finding.category)
  );
  const avgCognitiveComplexity = average(
    functions.map(fn => fn.cognitiveComplexity || fn.complexity || 0)
  );
  const maintainabilityDensity = weightedFindingPressure(
    maintainabilityFindings,
    totalFiles,
    { applyCategoryWeight: true }
  );
  const severeDensity = weightedFindingPressure(severeFindings, totalFiles);
  const testDebtDensity = weightedFindingPressure(testDebtFindings, totalFiles, {
    applyCategoryWeight: true,
  });
  const maintainabilityScore = clampScore(
    100
    - softPenalty(maintainabilityDensity, 20, 1.3)
    - softPenalty(severeDensity, 22, 1.8)
    - softPenalty(Math.max(0, (avgCognitiveComplexity - 8) / 12), 22, 1.4)
    - softPenalty(testDebtDensity, 12, 1.5)
  );
  aspects.push({
    aspect: 'maintainability-evolvability',
    label: 'Maintainability & Evolvability',
    weight: 15,
    score: maintainabilityScore,
    grade: gradeScore(maintainabilityScore),
    confidence: confidenceFromSample(maintainabilityFindings.length),
    rationale:
      'Rates how safely the codebase can evolve by blending quality/dead-code/test debt density, severe issue concentration, and cognitive complexity pressure.',
    signals: [
      {
        label: 'Maintainability findings / file',
        value: maintainabilityDensity.toFixed(2),
        effect: maintainabilityFindings.length > 0 ? 'negative' : 'neutral',
      },
      {
        label: 'Average cognitive complexity',
        value: avgCognitiveComplexity.toFixed(1),
        effect: avgCognitiveComplexity > 12 ? 'negative' : 'neutral',
      },
      {
        label: 'Test debt findings',
        value: String(testDebtFindings.length),
        effect: testDebtFindings.length > 0 ? 'negative' : 'neutral',
      },
    ],
  });

  const styleCounts = new Map<string, number>();
  for (const baseName of fileBaseNames) {
    const style = classifyFileNameStyle(baseName);
    styleCounts.set(style, (styleCounts.get(style) || 0) + 1);
  }
  const folderStyleCounts = new Map<string, number>();
  for (const segment of dirSegments) {
    const style = classifyFileNameStyle(segment);
    folderStyleCounts.set(style, (folderStyleCounts.get(style) || 0) + 1);
  }
  const dominantStyleRatio =
    styleCounts.size === 0 ? 1 : Math.max(...styleCounts.values()) / totalFiles;
  const folderSegmentCount = Math.max(1, dirSegments.size);
  const dominantFolderStyleRatio =
    folderStyleCounts.size === 0
      ? 1
      : Math.max(...folderStyleCounts.values()) / folderSegmentCount;
  const tsFileCount = filePaths.filter(file => /\.(ts|tsx)$/i.test(file)).length;
  const jsFileCount = filePaths.filter(file => /\.(js|jsx|mjs|cjs)$/i.test(file)).length;
  const mixedExtensionRatio = (Math.min(tsFileCount, jsFileCount) / totalFiles) * 2;
  const consistencyScore = clampScore(
    100
    - softPenalty(1 - dominantStyleRatio, 24, 2.0)
    - softPenalty(1 - dominantFolderStyleRatio, 12, 1.8)
    - softPenalty(mixedExtensionRatio, 12, 1.6)
    - softPenalty(genericFileCount / totalFiles, 10, 1.6)
  );
  aspects.push({
    aspect: 'codebase-consistency',
    label: 'Codebase Consistency',
    weight: 10,
    score: consistencyScore,
    grade: gradeScore(consistencyScore),
    confidence: confidenceFromSample(filePaths.length),
    rationale:
      'Rates naming/structure consistency with soft penalties for mixed file and folder naming styles, mixed TS/JS surface area, and generic file naming concentration.',
    signals: [
      {
        label: 'Dominant naming style',
        value: `${Math.round(dominantStyleRatio * 100)}%`,
        effect: dominantStyleRatio >= 0.7 ? 'positive' : 'negative',
      },
      {
        label: 'Dominant folder naming style',
        value: `${Math.round(dominantFolderStyleRatio * 100)}%`,
        effect: dominantFolderStyleRatio >= 0.7 ? 'positive' : 'negative',
      },
      {
        label: 'TS/JS mix ratio',
        value: `${Math.round((Math.min(tsFileCount, jsFileCount) / totalFiles) * 100)}%`,
        effect: mixedExtensionRatio > 0.5 ? 'negative' : 'neutral',
      },
      {
        label: 'Detected file naming styles',
        value: String(styleCounts.size),
        effect: styleCounts.size > 3 ? 'negative' : 'neutral',
      },
    ],
  });

  const totalWeight = aspects.reduce((sum, aspect) => sum + aspect.weight, 0) || 1;
  const weightedScore =
    aspects.reduce((sum, aspect) => sum + aspect.score * aspect.weight, 0)
    / totalWeight;

  return {
    model: 'hybrid-ai-structure-v1',
    overallScore: clampScore(weightedScore),
    overallGrade: gradeScore(clampScore(weightedScore)),
    aspects,
  };
}

function estimatePillarFileCount(
  totalFiles: number,
  findings: Finding[]
): number {
  if (totalFiles <= 0) return 0;
  const coveredFiles = new Set<string>();
  for (const finding of findings) {
    if (finding.file) coveredFiles.add(finding.file);
    for (const file of finding.files ?? []) coveredFiles.add(file);
  }
  if (coveredFiles.size === 0) return totalFiles;
  const floor = Math.max(1, Math.ceil(totalFiles * 0.1));
  return Math.max(floor, Math.min(totalFiles, coveredFiles.size));
}

function computePillarHealthScores(
  totalFiles: number,
  overallFindingStats: { totalFindings: number; severityBreakdown: Record<string, number> },
  ctx: {
    archStats?: { severityBreakdown: Record<string, number> };
    qualStats?: { severityBreakdown: Record<string, number> };
    deadStats?: { severityBreakdown: Record<string, number> };
    secStats?: { severityBreakdown: Record<string, number> };
    testStats?: { severityBreakdown: Record<string, number> };
    architectureFindings: Finding[];
    codeQualityFindings: Finding[];
    deadCodeFindings: Finding[];
    securityFindings: Finding[];
    testQualityFindings: Finding[];
  }
): PillarHealthScores {
  const score = (
    stats: { severityBreakdown: Record<string, number> } | undefined,
    fallback: Finding[],
    pillarFiles: number
  ) => computeHealthScoreFromSeverityBreakdown(
    stats?.severityBreakdown ?? severityBreakdown(fallback),
    pillarFiles
  );
  const archFiles = estimatePillarFileCount(totalFiles, ctx.architectureFindings);
  const qualFiles = estimatePillarFileCount(totalFiles, ctx.codeQualityFindings);
  const deadFiles = estimatePillarFileCount(totalFiles, ctx.deadCodeFindings);
  const secFiles = estimatePillarFileCount(totalFiles, ctx.securityFindings);
  const testFiles = estimatePillarFileCount(totalFiles, ctx.testQualityFindings);
  return {
    overallHealth: computeHealthScoreFromSeverityBreakdown(overallFindingStats.severityBreakdown, totalFiles),
    archHealth: score(ctx.archStats, ctx.architectureFindings, archFiles),
    qualHealth: score(ctx.qualStats, ctx.codeQualityFindings, qualFiles),
    deadHealth: score(ctx.deadStats, ctx.deadCodeFindings, deadFiles),
    secHealth: score(ctx.secStats, ctx.securityFindings, secFiles),
    testHealth: score(ctx.testStats, ctx.testQualityFindings, testFiles),
  };
}

function gradeScore(s: number): string {
  return s >= 80 ? 'A' : s >= 60 ? 'B' : s >= 40 ? 'C' : s >= 20 ? 'D' : 'F';
}

function resolveScoredCategories(activeFeatures: Set<string> | null): string[] {
  const ordered = Object.values(PILLAR_CATEGORIES).flat();
  if (!activeFeatures) return ordered;
  return ordered.filter(category => activeFeatures.has(category));
}

export function computeFeatureScores(
  findings: Finding[],
  totalFiles: number,
  activeFeatures: Set<string> | null,
  context: FeatureScoreContext = {}
): FeatureScoreRow[] {
  const hotFileRisk = new Map<string, number>();
  for (const hf of context.hotFiles || []) {
    hotFileRisk.set(hf.file, hf.riskScore);
  }

  const categories = resolveScoredCategories(activeFeatures);
  const seenCategories = new Set(categories);
  for (const finding of findings) {
    if (!seenCategories.has(finding.category)) {
      if (!activeFeatures || activeFeatures.has(finding.category)) {
        categories.push(finding.category);
        seenCategories.add(finding.category);
      }
    }
  }

  const findingsByCategory = new Map<string, Finding[]>();
  for (const finding of findings) {
    if (!findingsByCategory.has(finding.category)) {
      findingsByCategory.set(finding.category, []);
    }
    findingsByCategory.get(finding.category)!.push(finding);
  }

  return categories
    .map(category => {
      const categoryFindings = findingsByCategory.get(category) || [];
      const breakdown = severityBreakdown(categoryFindings);
      const affected = new Set<string>();
      for (const finding of categoryFindings) {
        if (finding.file) affected.add(finding.file);
        for (const file of finding.files ?? []) affected.add(file);
      }
      const denominator =
        affected.size > 0 ? Math.max(1, affected.size) : Math.max(1, totalFiles);
      const baseScore = computeHealthScoreFromSeverityBreakdown(
        breakdown,
        denominator
      );
      let hotspotHits = 0;
      let hotspotMaxRisk = 0;
      for (const file of affected) {
        const risk = hotFileRisk.get(file) || 0;
        if (risk <= 0) continue;
        hotspotHits += 1;
        hotspotMaxRisk = Math.max(hotspotMaxRisk, risk);
      }
      const overlapRatio = affected.size > 0 ? hotspotHits / affected.size : 0;
      const riskWeight = hotspotMaxRisk >= 90
        ? 10
        : hotspotMaxRisk >= 75
          ? 7
          : hotspotMaxRisk >= 60
            ? 4
            : 2;
      const contextPenalty = hotspotHits === 0
        ? 0
        : Math.min(20, Math.round(overlapRatio * 10 + riskWeight));
      const score = Math.max(0, baseScore - contextPenalty);
      return {
        category,
        pillar: CATEGORY_PILLAR_MAP[category] || 'unmapped',
        findings: categoryFindings.length,
        affectedFiles: affected.size,
        hotspotHits,
        hotspotMaxRisk,
        contextPenalty,
        severityBreakdown: breakdown,
        score,
        grade: gradeScore(score),
      };
    })
    .sort((a, b) => {
      if (a.score !== b.score) return a.score - b.score;
      if (a.findings !== b.findings) return b.findings - a.findings;
      return a.category.localeCompare(b.category);
    });
}

function renderHealthScores(
  lines: string[],
  health: PillarHealthScores,
  activeFeatures: Set<string> | null
): void {
  lines.push('## Health Scores\n');
  lines.push('| Pillar | Score | Grade |');
  lines.push('|--------|-------|-------|');
  const pushRow = (label: string, pillarKey: string, score: number): void => {
    if (!isPillarActive(pillarKey, activeFeatures)) {
      lines.push(`| ${label} | — | skipped |`);
      return;
    }
    lines.push(`| ${label} | ${score}/100 | ${gradeScore(score)} |`);
  };
  lines.push(
    `| **Overall** | **${health.overallHealth}/100** | **${gradeScore(health.overallHealth)}** |`
  );
  pushRow('Architecture', 'architecture', health.archHealth);
  pushRow('Code Quality', 'code-quality', health.qualHealth);
  pushRow('Dead Code & Hygiene', 'dead-code', health.deadHealth);
  pushRow('Security', 'security', health.secHealth);
  pushRow('Test Quality', 'test-quality', health.testHealth);
  lines.push('');
}

function renderFeatureScores(lines: string[], rows: FeatureScoreRow[]): void {
  lines.push('## Feature Scores\n');
  lines.push(
    'Per-category scoring for all active features (or all categories when unfiltered).\n'
  );
  lines.push(
    '| Category | Pillar | Findings | Affected Files | Hotspot Hits | Context Penalty | Score | Grade |'
  );
  lines.push(
    '|----------|--------|----------|----------------|--------------|-----------------|-------|-------|'
  );
  for (const row of rows) {
    lines.push(
      `| \`${row.category}\` | ${row.pillar} | ${row.findings} | ${row.affectedFiles} | ${row.hotspotHits} | -${row.contextPenalty} | ${row.score}/100 | ${row.grade} |`
    );
  }
  lines.push('');
}

function renderQualityAspectRatings(
  lines: string[],
  rating: QualityRatingSummary
): void {
  lines.push('## AI + Structure Ratings\n');
  lines.push(
    'Hybrid, soft-signal scoring that blends structural findings with architecture context, naming quality, folder topology, and shared-layer health.\n'
  );
  lines.push(
    `**Overall Hybrid Rating**: ${rating.overallScore}/100 (${rating.overallGrade})  `
  );
  lines.push(`**Model**: \`${rating.model}\`\n`);
  lines.push(
    '| Aspect | Weight | Score | Grade | Confidence | Why it scored this way |'
  );
  lines.push(
    '|--------|--------|-------|-------|------------|------------------------|'
  );
  for (const aspect of rating.aspects) {
    lines.push(
      `| ${aspect.label} | ${aspect.weight}% | ${aspect.score}/100 | ${aspect.grade} | ${aspect.confidence} | ${aspect.rationale} |`
    );
  }
  lines.push('');
}

function buildPillarSummaryPusher(
  lines: string[],
  activeFeatures: Set<string> | null,
  outputFiles: Record<string, string>
): (pillarKey: string, count: number, score: number, artifactKey?: string, artifactName?: string) => void {
  return (pillarKey, findingsCount, score, artifactKey, artifactName): void => {
    if (!isPillarActive(pillarKey, activeFeatures)) {
      lines.push('> skipped by feature filter\n');
      return;
    }
    if (artifactKey && outputFiles[artifactKey]) {
      lines.push(
        `> ${findingsCount} findings (score: ${score}/100) — see [\`${artifactName}\`](./${outputFiles[artifactKey]})\n`
      );
      return;
    }
    if (artifactName) {
      lines.push(
        `> ${findingsCount} findings (score: ${score}/100) — no \`${artifactName}\` written for this scan\n`
      );
      return;
    }
    lines.push(`> ${findingsCount} findings (score: ${score}/100)\n`);
  };
}

function renderScanAnnotations(
  lines: string[],
  ctx: {
    allFindings: Finding[];
    overallFindingStats: { totalFindings: number; severityBreakdown: Record<string, number> };
    agentOutput: AgentOutputData;
    activeFeatures: Set<string> | null;
    scope: string[] | null;
    root: string;
    scopeSymbols: Map<string, string[]> | null;
    semanticEnabled: boolean;
  }
): void {
  const { allFindings, overallFindingStats, agentOutput } = ctx;
  const totalBefore: number | undefined =
    overallFindingStats.totalFindings || agentOutput?.totalBeforeTruncation;
  const dropped = agentOutput?.droppedCategories;
  if (totalBefore && totalBefore > allFindings.length) {
    lines.push(
      `> **Truncated**: Showing ${allFindings.length} of ${totalBefore} findings (\`--findings-limit ${allFindings.length}\`).`
    );
    if (dropped && dropped.length > 0) {
      lines.push(`> Dropped categories: ${dropped.map(c => `\`${c}\``).join(', ')}`);
    }
    lines.push('');
  }

  if (ctx.activeFeatures) {
    const featureLabels = summarizeActiveFeatures(ctx.activeFeatures);
    lines.push(`> **Features filter**: \`--features=${featureLabels.join(',')}\``);
    lines.push('');
  }

  if (ctx.scope && ctx.scope.length > 0) {
    const scopeDisplay = ctx.scope.map(s => path.relative(ctx.root, s)).filter(Boolean);
    if (scopeDisplay.length > 0) {
      let scopeLabel = scopeDisplay.map(p => `\`${p}\``).join(', ');
      if (ctx.scopeSymbols && ctx.scopeSymbols.size > 0) {
        const symParts: string[] = [];
        for (const [absFile, names] of ctx.scopeSymbols) {
          const rel = path.relative(ctx.root, absFile);
          symParts.push(...names.map(n => `\`${rel}:${n}\``));
        }
        scopeLabel = symParts.join(', ');
      }
      lines.push(`> **Scoped scan**: Only showing findings for: ${scopeLabel}`);
      lines.push('');
    }
  }

  if (ctx.semanticEnabled) {
    lines.push(
      '> **Semantic analysis**: TypeChecker + LanguageService enabled (14 additional categories)'
    );
    lines.push('');
  }
}

function renderTagCloud(lines: string[], allFindings: Finding[]): void {
  const tagCloud = collectTagCloud(allFindings);
  if (tagCloud.length === 0) return;
  lines.push('## Top Concern Tags\n');
  lines.push(
    'Searchable tags across all findings — use to filter `findings.json` with `jq`.\n'
  );
  for (const { tag, count } of tagCloud.slice(0, 12)) {
    lines.push(`- \`${tag}\`: ${count} findings`);
  }
  lines.push('');
}

function renderAnalysisSignals(
  lines: string[],
  reportAnalysis: ReportAnalysisSummary
): void {
  lines.push('## Analysis Signals\n');
  lines.push(
    `- **Graph Signal**: ${reportAnalysis.strongestGraphSignal?.summary || 'No dominant graph signal in this scan.'}`
  );
  lines.push(
    `- **AST Signal**: ${reportAnalysis.strongestAstSignal?.summary || 'No dominant AST signal in this scan.'}`
  );
  lines.push(
    `- **Combined Interpretation**: ${reportAnalysis.combinedInterpretation?.summary || 'No combined interpretation available yet.'}`
  );
  lines.push(
    `- **Confidence**: ${reportAnalysis.combinedInterpretation?.confidence || reportAnalysis.strongestGraphSignal?.confidence || reportAnalysis.strongestAstSignal?.confidence || 'low'}`
  );
  const validationSummary = reportAnalysis.recommendedValidation
    ? `${reportAnalysis.recommendedValidation.summary} (tools: ${reportAnalysis.recommendedValidation.tools.join(' -> ')})`
    : 'Use Octocode local tools to confirm the strongest signal before presenting it as fact.';
  lines.push(`- **Recommended Validation**: ${validationSummary}`);
  const megaFolderSignal = reportAnalysis.graphSignals.find(
    signal => signal.kind === 'mega-folder-cluster'
  );
  if (megaFolderSignal) {
    lines.push(`- **Structural Layout Alert**: ${megaFolderSignal.summary}`);
  }
  if (reportAnalysis.investigationPrompts.length > 0) {
    lines.push('');
    lines.push('**Investigation Prompts**');
    for (const prompt of reportAnalysis.investigationPrompts.slice(0, 4)) {
      lines.push(`- ${prompt}`);
    }
  }
  lines.push('');
}

function renderAgentInstructions(
  lines: string[],
  outputFiles: Record<string, string>,
  allFindings: Finding[]
): void {
  const hasCriticalOrHigh = allFindings.some(
    f => f.severity === 'critical' || f.severity === 'high'
  );
  lines.push('## Agent Instructions — Validate Before Presenting\n');
  lines.push(
    '> **Core rule**: Findings are hypotheses from deterministic AST/graph detectors. '
    + 'Validate with Octocode local + LSP tools before presenting any finding as fact.\n'
  );

  lines.push('### Triage Order\n');
  lines.push('1. **This file first** — health scores + analysis signals drive triage priority');
  if (hasCriticalOrHigh) {
    lines.push(
      '2. **High/critical findings** — filter `findings.json`: '
      + '`jq \'.optimizationFindings[] | select(.severity == "critical" or .severity == "high")\' findings.json`'
    );
  } else {
    lines.push('2. **Findings by severity** — start from the top of `findings.json` (already sorted by severity)');
  }
  lines.push('3. **Pillar JSONs** — drill into `architecture.json`, `code-quality.json`, etc. only for categories that need investigation');
  lines.push('4. **`file-inventory.json`** — per-file deep dives: functions, flows, `effectProfile`, `cfgFlags`, `dependencyProfile`');
  lines.push('');

  lines.push('### Validation Tool Chain\n');
  lines.push('Each finding includes `lspHints[]`, `correlatedSignals[]`, and `recommendedValidation`. Use them.\n');
  lines.push('```');
  lines.push('Finding → localSearchCode (get lineHint) → LSP tool → localGetFileContent → verdict');
  lines.push('```\n');
  lines.push('| Step | Tool | Purpose |');
  lines.push('|------|------|---------|');
  lines.push('| 1. Search | `localSearchCode(pattern, path)` | **Always first** — get `lineHint` for LSP. Never guess lineHint. |');
  lines.push('| 2. Locate | `lspGetSemantics(type=definition, lineHint)` | Jump to definition across files |');
  lines.push('| 3. Consumers | `lspGetSemantics(type=references, lineHint)` | Count usages, split test/prod with `includePattern`/`excludePattern` |');
  lines.push('| 4. Call flow | `lspGetSemantics(type=callers/callees, lineHint)` | Trace call chains — **functions only**, fails on types/vars |');
  lines.push('| 5. Read code | `localGetFileContent(path, matchString=...)` | Confirm code at reported location |');
  lines.push('| 6. AST proof | `ast/search.js -p <pattern> --root <path>` | Structural proof on **live source** — zero false positives |');
  if (outputFiles.astTrees) {
    lines.push('| 7. AST triage | `ast/tree-search.js -i <scan-dir> -k <Kind>` | Fast triage on scan snapshot — `-k FunctionDeclaration`, `-p \'IfStatement\\|ForStatement\'`, `--file` filter, `-C 2` context |');
  }
  lines.push('');

  lines.push('### False Positive Checklist\n');
  lines.push('Before reporting a finding to the user:\n');
  lines.push('- [ ] Ran `lspHints[]` from the finding — result matches expectation?');
  lines.push('- [ ] Code exists at reported `file:lineStart` — confirmed with `localGetFileContent`?');
  lines.push('- [ ] Pattern confirmed in live source — `ast/search.js -p` or `localSearchCode`?');
  lines.push('- [ ] Not in generated, vendored, or test-only code?');
  lines.push('- [ ] `correlatedSignals[]` — multiple signals on same file strengthen confidence');
  lines.push('- [ ] Consumer count verified with `lspGetSemantics(type=references)` — matches claimed impact?');
  lines.push('');
  lines.push('**Rate each finding**: `confirmed` (evidence supports) · `dismissed` (explain why) · `uncertain` (state what\'s missing)\n');
}

function renderHotspots(
  lines: string[],
  hotFiles: SummaryMdOptions['hotFiles']
): void {
  if (!hotFiles || hotFiles.length === 0) return;
  lines.push('## Change Risk Hotspots\n');
  lines.push(
    'Files most dangerous to change — high fan-in, complexity, or cycle membership.\n'
  );
  lines.push(
    '| File | Risk | Fan-In | Fan-Out | Complexity | Exports | Cycle | Critical Path |'
  );
  lines.push(
    '|------|------|--------|---------|------------|---------|-------|---------------|'
  );
  for (const hf of hotFiles.slice(0, 15)) {
    lines.push(
      `| \`${hf.file}\` | ${hf.riskScore} | ${hf.fanIn} | ${hf.fanOut} | ${hf.complexityScore} | ${hf.exportCount} | ${hf.inCycle ? 'Y' : '-'} | ${hf.onCriticalPath ? 'Y' : '-'} |`
    );
  }
  lines.push('');
}

function renderPillarSections(
  lines: string[],
  ctx: {
    architectureFindings: Finding[];
    codeQualityFindings: Finding[];
    deadCodeFindings: Finding[];
    securityFindings: Finding[];
    testQualityFindings: Finding[];
    archStats: { totalFindings: number; severityBreakdown: Record<string, number> } | undefined;
    qualStats: { totalFindings: number; severityBreakdown: Record<string, number> } | undefined;
    deadStats: { totalFindings: number; severityBreakdown: Record<string, number> } | undefined;
    secStats: { totalFindings: number; severityBreakdown: Record<string, number> } | undefined;
    testStats: { totalFindings: number; severityBreakdown: Record<string, number> } | undefined;
    archHealth: number;
    qualHealth: number;
    deadHealth: number;
    secHealth: number;
    testHealth: number;
    activeFeatures: Set<string> | null;
    outputFiles: Record<string, string>;
    renderPillarCategories: (pillarKey: string, findings: Finding[]) => void;
    pushPillarSummary: (pillarKey: string, count: number, score: number, artifactKey?: string, artifactName?: string) => void;
  }
): void {
  const { architectureFindings, codeQualityFindings, deadCodeFindings, securityFindings, testQualityFindings } = ctx;
  const { qualStats, deadStats, secStats, testStats } = ctx;
  const { qualHealth, deadHealth, secHealth, testHealth } = ctx;
  const { renderPillarCategories, pushPillarSummary } = ctx;

  lines.push('## Code Quality\n');
  pushPillarSummary(
    'code-quality',
    qualStats?.totalFindings ?? codeQualityFindings.length,
    qualHealth,
    'codeQuality',
    'code-quality.json'
  );
  renderPillarCategories('code-quality', codeQualityFindings);

  lines.push('## Dead Code & Hygiene\n');
  pushPillarSummary(
    'dead-code',
    deadStats?.totalFindings ?? deadCodeFindings.length,
    deadHealth,
    'deadCode',
    'dead-code.json'
  );
  renderPillarCategories('dead-code', deadCodeFindings);

  lines.push('## Security\n');
  pushPillarSummary(
    'security',
    secStats?.totalFindings ?? securityFindings.length,
    secHealth,
    'security',
    'security.json'
  );
  renderPillarCategories('security', securityFindings);

  lines.push('## Test Quality\n');
  pushPillarSummary(
    'test-quality',
    testStats?.totalFindings ?? testQualityFindings.length,
    testHealth,
    'testQuality',
    'test-quality.json'
  );
  renderPillarCategories('test-quality', testQualityFindings);

  const untestedCount = architectureFindings.filter(
    f => f.category === 'untested-critical-code'
  ).length;
  if (
    untestedCount > 0 &&
    (testStats?.totalFindings ?? testQualityFindings.length) === 0
  ) {
    lines.push(
      `> **Note**: Test Quality reflects analyzed test files only. ${untestedCount} modules flagged as \`untested-critical-code\` (architecture pillar) have no test coverage — use \`--include-tests\` for test-quality analysis.\n`
    );
  }
}

function renderRecommendations(
  lines: string[],
  agentOutput: AgentOutputData
): void {
  const topRecs = agentOutput?.topRecommendations ?? [];
  if (topRecs.length > 0) {
    lines.push('## Top Recommendations\n');
    for (const rec of topRecs.slice(0, 10)) {
      lines.push(
        `- **[${rec.severity.toUpperCase()}]** \`${rec.file}\` — ${rec.title} *(${rec.category})* `
      );
    }
    lines.push('');
  }
}

function renderAstTreesSection(
  lines: string[],
  dir: string,
  outputFiles: Record<string, string>,
  root: string,
  relativeScanDir: string,
  exampleFileFilter: string
): void {
  const astTreePath = path.resolve(dir, outputFiles.astTrees);
  const astTreeArg = formatCliPath(astTreePath);
  lines.push('## AST Trees (`ast-trees.txt`)\n');
  lines.push(
    'Compact indented text format — each node is `Kind[startLine:endLine]`, nesting = indentation.\n'
  );
  lines.push(
    `Run these commands from the skill directory. Current scan: \`${relativeScanDir}\`.\n`
  );
  lines.push('```');
  lines.push('SourceFile[1:152]');
  lines.push('  ImportDeclaration[1]');
  lines.push('  FunctionDeclaration[3:20]');
  lines.push('    Block[4:19]');
  lines.push('      IfStatement[5:12] ...');
  lines.push('```\n');
  lines.push('**Smart navigation:**\n');
  lines.push(
    `- Find functions: \`node scripts/ast/tree-search.js -i ${astTreeArg} -k function_declaration --limit 25\``
  );
  lines.push(
    `- Find classes: \`node scripts/ast/tree-search.js -i ${astTreeArg} -k class_declaration --limit 25\``
  );
  lines.push(
    `- Find control flow: \`node scripts/ast/tree-search.js -i ${astTreeArg} -p 'IfStatement|SwitchStatement|ForStatement|WhileStatement' --limit 25\``
  );
  lines.push(
    `- Narrow to one file: \`node scripts/ast/tree-search.js -i ${astTreeArg} --file "${exampleFileFilter}" -k function_declaration --limit 10\``
  );
  lines.push(
    `- Raw text fallback: \`rg 'FunctionDeclaration|IfStatement' ${astTreeArg}\``
  );
  lines.push('');
}

function renderOutputFilesTable(
  lines: string[],
  dir: string,
  outputFiles: Record<string, string>
): void {
  lines.push('## Output Files\n');
  lines.push('| File | Size | Description |');
  lines.push('|------|------|-------------|');
  const descriptions: Record<string, string> = {
    summary: 'Scan metadata, agent output, parse errors',
    architecture:
      'Dependency graph, cycles, critical paths, architecture findings',
    codeQuality: 'Duplicate detection, complexity, god modules/functions',
    deadCode: 'Dead files/exports/re-exports, unused deps, boundary violations',
    fileInventory: 'Per-file function/flow/dependency details',
    findings: 'All findings across all categories (master list)',
    graph: 'Mermaid dependency graph',
    astTrees:
      'AST tree snapshots (compact indented text — grep/regex friendly)',
    summaryMd: 'This file — human-readable overview',
  };
  for (const [key, file] of Object.entries(outputFiles)) {
    let size = '—';
    try {
      size = formatFileSize(fs.statSync(path.join(dir, file)).size);
    } catch {
      size = '—';
    }
    lines.push(
      `| [\`${file}\`](./${file}) | ${size} | ${descriptions[key] || key} |`
    );
  }
  lines.push('');
}
