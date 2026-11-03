// Exit 1 is the CLI's successful empty result. Every other failure remains
// attached to its actual invocation, including failures on later pages.
export function callFailure(result, body) {
  if ([0, 1].includes(result.code) && !body.error && !body.errorCode && !body.parseError) return null;
  return { exit: result.code, errorCode: body.errorCode, error: body.error ?? body.parseError ?? result.err };
}

export function batteryFailures(rows) {
  return rows.flatMap(row => {
    const failures = (row.failures ?? []).map(failure => ({ lang: row.lang, ...failure }));
    if (failures.length === 0) {
      for (const [operation, status] of Object.entries(workflowStatuses(row))) {
        if (callFailure({ code: status.exit }, status)) failures.push({ lang: row.lang, operation, ...status });
      }
    }
    if (row.ingest?.exit !== 0 && !failures.length) failures.push({ lang: row.lang, tool: 'graphBuild', ...row.ingest });
    if (row.callers?.pagesComplete === false) failures.push({ lang: row.lang, tool: 'graphQuery', error: 'callers pages incomplete' });
    return failures;
  });
}

export function workflowStatuses(row) {
  return Object.fromEntries(Object.entries(row ?? {})
    .filter(([, value]) => value && typeof value === 'object' && Object.hasOwn(value, 'exit'))
    .map(([name, value]) => [name, { exit: value.exit, errorCode: value.errorCode, error: value.error }]));
}
