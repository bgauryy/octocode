// Plan steps that end a flow with evidence: the page snapshot and screenshot recipes, run on the
// step's own scope (target, frame or session). Each recipe writes fixed file names into the run
// folder, so validation allows one of each per plan.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { run as snapshot } from './page-snapshot.mjs';
import { run as screenshot } from './page-screenshot.mjs';
import type { PlanOp } from './plan-types.mjs';

type Manifest = { complete?: boolean; tiles?: { file: string }[] };

const snapshotStep: PlanOp = async (_state, { row, target }) => {
  await snapshot(target);
  row.artifact = join(target.outputDir, 'page-snapshot.json');
};

const screenshotStep: PlanOp = async (_state, { row, target }) => {
  await screenshot(target);
  const manifest: Manifest = JSON.parse(
    readFileSync(join(target.outputDir, 'screenshot-manifest.json'), 'utf8')
  );
  const first = manifest.tiles?.[0]?.file;
  if (!first) throw new Error('Screenshot produced no image');
  row.artifact = first;
  row.count = manifest.tiles?.length;
};

export const captureOps: Record<string, PlanOp> = {
  snapshot: snapshotStep,
  screenshot: screenshotStep,
};
