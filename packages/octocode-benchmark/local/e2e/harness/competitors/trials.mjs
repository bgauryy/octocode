/** Keep warm costs separate while retaining first-trial evidence and failures. */
export function retainColdTrial(warm, cold) {
  if (!cold) return;
  warm.cold = cold;
  const failures = [...cold.fails];
  if (cold.sensors?.violations.length) failures.push(`${cold.sensors.violations.length} continuation coverage violations`);
  if (cold.sensors?.schemaErrors) failures.push(`${cold.sensors.schemaErrors} schema errors`);
  if (!cold.ok || failures.length) {
    warm.ok = false;
    warm.fails.push(...failures.map(failure => `cold trial: ${failure}`));
  }
}

function qualityComplete(arm) {
  if (!arm?.ok || arm.sensors?.violations?.length || arm.sensors?.coverageGaps?.length || arm.replay?.failed) return false;
  return !arm.cold || qualityComplete(arm.cold);
}

/** Failed or explicitly partial evidence cannot win an efficiency comparison. */
export const qualityPair = (row, arm = 'competitor') => qualityComplete(row.octocode) && qualityComplete(row[arm]);
