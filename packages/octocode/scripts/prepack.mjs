#!/usr/bin/env node
import { fileURLToPath } from 'node:url';
import { stageSkills } from './stage-skills.mjs';

stageSkills(
  fileURLToPath(new URL('../../../skills/', import.meta.url)),
  fileURLToPath(new URL('../skills/', import.meta.url))
);
console.log('✓ skills staged from monorepo root');
