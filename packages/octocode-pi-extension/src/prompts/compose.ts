import {
  authority,
  thinkFirst,
  workMode,
  memory,
  tools,
  searchAndResearch,
  octocodeCli,
  skills,
  code,
  docs,
  output,
  context,
  agents,
  browserAgent,
  safety,
} from './sections/index.js';

/** The full Octocode system prompt — sections in the order they appear below. */
export const SYSTEM_PROMPT =
  [
    authority,
    thinkFirst,
    workMode,
    memory,
    tools,
    searchAndResearch,
    octocodeCli,
    skills,
    code,
    docs,
    output,
    context,
    agents,
    browserAgent,
    safety,
  ].join('\n\n') + '\n';
