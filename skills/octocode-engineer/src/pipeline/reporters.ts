import type { Finding, ReporterFormat } from '../types/index.js';


export function formatFindings(
  findings: Finding[],
  format: ReporterFormat,
  root: string
): string {
  switch (format) {
    case 'compact':
      return formatCompact(findings, root);
    case 'github-actions':
      return formatGitHubActions(findings, root);
    default:
      return '';
  }
}

function relativePath(file: string, root: string): string {
  return file.startsWith(root) ? file.slice(root.length + 1) : file;
}

function severityToLevel(severity: string): string {
  switch (severity) {
    case 'critical': return 'error';
    case 'high': return 'error';
    case 'medium': return 'warning';
    case 'low': return 'notice';
    default: return 'warning';
  }
}


function formatCompact(findings: Finding[], root: string): string {
  return findings
    .map(f => {
      const file = relativePath(f.file, root);
      const loc = f.lineStart ? `${file}:${f.lineStart}` : file;
      return `${f.severity}:${loc} - [${f.category}] ${f.title}`;
    })
    .join('\n');
}


function formatGitHubActions(findings: Finding[], root: string): string {
  return findings
    .map(f => {
      const file = relativePath(f.file, root);
      const line = f.lineStart || 1;
      const level = severityToLevel(f.severity);
      return `::${level} file=${file},line=${line}::${f.title} [${f.category}]`;
    })
    .join('\n');
}
