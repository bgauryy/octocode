// Plan steps that end a flow with evidence: the page snapshot and screenshot recipes, run on the
// step's own scope (target, frame or session). Each recipe writes fixed file names into the run
// folder, so validation allows one of each per plan.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { run as snapshot } from './page-snapshot.mjs';
import { run as screenshot, tileFile } from './page-screenshot.mjs';
import { CAPTURE_RECIPES, recipeEnv } from './recipe-options.mjs';
import type { PlanOp } from './plan-types.mjs';

type Manifest = { complete?: boolean; tiles?: { file: string }[] };

// Options act as the standalone tool's do: the same variables, applied over this process's defaults.
// A recipe reports a failed wait or a missing element through the exit code; in a plan that fails the step.
async function runRecipe(
  op: string,
  recipe: typeof snapshot | typeof screenshot,
  target: Parameters<typeof snapshot>[0],
  options: unknown
) {
  const previous = process.exitCode;
  process.exitCode = undefined;
  try {
    await recipe(target, {
      ...process.env,
      ...(options === undefined ? {} : recipeEnv(CAPTURE_RECIPES[op], options)),
    });
    if (process.exitCode)
      throw new Error(`${op} did not finish: a wait or selector was not met`);
  } finally {
    process.exitCode = previous;
  }
}

const snapshotStep: PlanOp = async (_state, { step, row, target }) => {
  await runRecipe('snapshot', snapshot, target, step.options);
  row.artifact = join(target.outputDir, 'page-snapshot.json');
};

const screenshotStep: PlanOp = async (_state, { step, row, target }) => {
  await runRecipe('screenshot', screenshot, target, step.options);
  const manifest: Manifest = JSON.parse(
    readFileSync(join(target.outputDir, 'screenshot-manifest.json'), 'utf8')
  );
  const first = manifest.tiles?.[0]?.file;
  if (!first) throw new Error('Screenshot produced no image');
  row.artifact = tileFile(first);
  row.count = manifest.tiles?.length;
};

export const captureOps: Record<string, PlanOp> = {
  snapshot: snapshotStep,
  screenshot: screenshotStep,
};
