export function summarizeChecks(checks) {
  return {
    total: checks.length,
    passed: checks.filter(check => check.ok === true).length,
    failed: checks.filter(check => check.ok === false).length,
    notApplicable: checks.filter(check => check.ok === null).length,
  };
}

export function formatCheckSummary(summary) {
  return `${summary.passed} passed, ${summary.failed} failed, ${summary.notApplicable} not applicable (${summary.total} records)`;
}
