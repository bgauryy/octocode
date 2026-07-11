import {
  authority,
  thinkFirst,
  workMode,
  awareness,
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
    safety,
    workMode,
    thinkFirst,
    agents,
    awareness,
    tools,
    searchAndResearch,
    octocodeCli,
    skills,
    code,
    docs,
    context,
    browserAgent,
    output,
  ].join('\n\n') + '\n';
