// Process environment variables the contract governs apart from settings: dotenv trust policy,
// token precedence and credential marking. Setting-bound variables live on each field's `env`.
import type { ContractInput } from '../schema.ts';

export const environment: ContractInput['environment'] = {
  PATH: { dotenv: 'never', description: 'Executable search path' },
  HOME: { dotenv: 'never', description: 'Operating-system home' },
  SHELL: { dotenv: 'never', description: 'Login shell' },
  USER: { dotenv: 'never', description: 'Operating-system user' },
  LOGNAME: { dotenv: 'never', description: 'Operating-system login' },
  PWD: { dotenv: 'never', description: 'Process working directory' },
  TMPDIR: { dotenv: 'never', description: 'Temporary directory' },
  NODE_OPTIONS: { dotenv: 'never', description: 'Node process options' },
  PYTHON: { dotenv: 'never', description: 'Python executable override' },
  GH_TOKEN: { dotenv: 'all', tokenPriority: 0, description: 'GitHub CLI token' },
  GITHUB_TOKEN: { dotenv: 'all', tokenPriority: 1, description: 'GitHub Actions token' },
  TAVILY_API_KEY: {
    dotenv: 'all',
    credential: true,
    documentation: true,
    description: 'Tavily search API credential used by the octocode-brainstorming skill.',
  },
  EXA_API_KEY: {
    dotenv: 'all',
    credential: true,
    documentation: true,
    description: 'Exa search API credential used by the octocode-brainstorming skill.',
  },
  SERPER_API_KEY: {
    dotenv: 'all',
    credential: true,
    documentation: true,
    description: 'Serper search API credential used by the octocode-brainstorming skill.',
  },
  SCRAPING_ANT: {
    dotenv: 'all',
    credential: true,
    documentation: true,
    description: 'ScrapingAnt API credential used by the octocode-scraping skill.',
  },
  OCTOCODE_GITHUB_CLIENT_ID: { dotenv: 'home', configSource: true, description: 'GitHub OAuth client override' },
  OCTOCODE_HOME: {
    documentation: true,
    dotenv: 'never',
    description: 'Overrides the Octocode configuration directory (default: <os-home>/.octocode)',
  },
  OCTOCODE_CARGO: {
    documentation: true,
    dotenv: 'home',
    description: 'Override the cargo executable used for Rust dependency-graph analysis.',
  },
};
