// Built-in flows: one data file each; flow-core validates every one against the real plan validator.
import type { FlowDefinition } from './flow-types.mjs';
import consoleNetworkAudit from './console-network-audit.mjs';
import extractLinks from './extract-links.mjs';
import fillForm from './fill-form.mjs';
import fullPageScreenshot from './full-page-screenshot.mjs';
import loginForm from './login-form.mjs';
import paginateList from './paginate-list.mjs';

export const builtinFlows: FlowDefinition[] = [
  loginForm,
  fillForm,
  extractLinks,
  paginateList,
  fullPageScreenshot,
  consoleNetworkAudit,
];
