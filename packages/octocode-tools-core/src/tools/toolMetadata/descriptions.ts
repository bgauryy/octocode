import { toolSpecs } from '../../toolContract/resources/tools/index.js';

export const DESCRIPTIONS: Record<string, string> = Object.fromEntries(
  Object.values(toolSpecs).map(tool => [tool.name, tool.description])
);
