import { z } from '@octocodeai/octocode-tools-core/zod';

const MCPCategorySchema = z.enum([
  'browser-automation',
  'database',
  'cloud-platform',
  'developer-tools',
  'file-system',
  'communication',
  'search-web',
  'ai-services',
  'workflow-automation',
  'version-control',
  'data-visualization',
  'coding-agents',
  'security',
  'productivity',
  'monitoring',
  'finance',
  'social-media',
  'aggregator',
  'other',
]);

export type MCPCategory = z.infer<typeof MCPCategorySchema>;

const _MCPRegistryEntrySchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  description: z.string().min(1),
  category: MCPCategorySchema,
  repository: z.string(),
  website: z.string().optional(),
  stars: z.number().optional(),
  installationType: z.enum(['npm', 'npx', 'pip', 'docker', 'source']),
  npmPackage: z.string().optional(),
  pipPackage: z.string().optional(),
  dockerImage: z.string().optional(),
  installConfig: z.object({
    command: z.string().min(1),
    args: z.array(z.string()),
    env: z.record(z.string(), z.string()).optional(),
  }),
  requiredEnvVars: z
    .array(
      z.object({
        name: z.string(),
        description: z.string(),
        example: z.string().optional(),
      })
    )
    .optional(),
  official: z.boolean().optional(),
  tags: z.array(z.string()).optional(),
});

export type MCPRegistryEntry = z.infer<typeof _MCPRegistryEntrySchema>;

export const MCP_REGISTRY: MCPRegistryEntry[] = [
  {
    id: 'playwright-mcp',
    name: 'Playwright MCP',
    description:
      'Official Microsoft Playwright MCP server for browser automation via accessibility snapshots',
    category: 'browser-automation',
    repository: 'https://github.com/microsoft/playwright-mcp',
    website: 'https://playwright.dev',
    installationType: 'npx',
    npmPackage: '@playwright/mcp',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@playwright/mcp@latest'],
    },
    tags: ['browser', 'automation', 'testing', 'microsoft', 'official'],
  },
  {
    id: 'firecrawl-mcp-server',
    name: 'Firecrawl MCP',
    description:
      'Powerful web scraping and search for Claude, Cursor and LLM clients',
    category: 'browser-automation',
    repository: 'https://github.com/firecrawl/firecrawl-mcp-server',
    website: 'https://firecrawl.dev',
    installationType: 'npx',
    npmPackage: 'firecrawl-mcp',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', 'firecrawl-mcp'],
    },
    requiredEnvVars: [
      {
        name: 'FIRECRAWL_API_KEY',
        description: 'Firecrawl API key',
        example: 'fc-xxxxx',
      },
    ],
    tags: ['scraping', 'web', 'search', 'official'],
  },
  {
    id: 'browserbase-mcp',
    name: 'Browserbase MCP',
    description:
      'Cloud browser automation for web navigation, data extraction, form filling',
    category: 'browser-automation',
    repository: 'https://github.com/browserbase/mcp-server-browserbase',
    website: 'https://browserbase.com',
    installationType: 'npx',
    npmPackage: '@browserbasehq/mcp-server-browserbase',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@browserbasehq/mcp-server-browserbase'],
    },
    requiredEnvVars: [
      { name: 'BROWSERBASE_API_KEY', description: 'Browserbase API key' },
      { name: 'BROWSERBASE_PROJECT_ID', description: 'Browserbase project ID' },
    ],
    tags: ['browser', 'cloud', 'automation', 'official'],
  },
  {
    id: 'chrome-devtools-mcp',
    name: 'Chrome DevTools MCP',
    description:
      'Chrome DevTools for coding agents - debugging and browser control',
    category: 'browser-automation',
    repository: 'https://github.com/ChromeDevTools/chrome-devtools-mcp',
    installationType: 'npx',
    npmPackage: 'chrome-devtools-mcp',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', 'chrome-devtools-mcp'],
    },
    tags: ['chrome', 'devtools', 'debugging', 'official'],
  },

  {
    id: 'github-mcp-server',
    name: 'GitHub MCP',
    description:
      "GitHub's official MCP Server for repository management, PRs, issues, and more",
    category: 'version-control',
    repository: 'https://github.com/github/github-mcp-server',
    website: 'https://github.com',
    installationType: 'docker',
    dockerImage: 'ghcr.io/github/github-mcp-server',
    official: true,
    installConfig: {
      command: 'docker',
      args: [
        'run',
        '-i',
        '--rm',
        '-e',
        'GITHUB_PERSONAL_ACCESS_TOKEN',
        'ghcr.io/github/github-mcp-server',
      ],
    },
    requiredEnvVars: [
      {
        name: 'GITHUB_PERSONAL_ACCESS_TOKEN',
        description: 'GitHub Personal Access Token',
      },
    ],
    tags: ['github', 'git', 'repository', 'official'],
  },

  {
    id: 'sqlite-mcp',
    name: 'SQLite MCP',
    description:
      'Comprehensive SQLite database interaction capabilities for Cursor, Windsurf, and AI tools',
    category: 'database',
    repository: 'https://github.com/jparkerweb/mcp-sqlite',
    installationType: 'npx',
    npmPackage: 'mcp-sqlite',
    installConfig: {
      command: 'npx',
      args: ['-y', 'mcp-sqlite', '${DATABASE_PATH}'],
    },
    requiredEnvVars: [
      {
        name: 'DATABASE_PATH',
        description: 'Path to SQLite database file',
        example: '/path/to/database.db',
      },
    ],
    tags: ['database', 'sqlite', 'sql', 'cursor', 'windsurf'],
  },
  {
    id: 'mysql-mcp',
    name: 'MySQL MCP',
    description:
      'MySQL database integration with configurable access controls and schema inspection',
    category: 'database',
    repository: 'https://github.com/designcomputer/mysql_mcp_server',
    installationType: 'pip',
    pipPackage: 'mysql-mcp-server',
    installConfig: {
      command: 'uvx',
      args: ['mysql-mcp-server'],
    },
    requiredEnvVars: [
      { name: 'MYSQL_HOST', description: 'MySQL host' },
      { name: 'MYSQL_USER', description: 'MySQL username' },
      { name: 'MYSQL_PASSWORD', description: 'MySQL password' },
      { name: 'MYSQL_DATABASE', description: 'MySQL database name' },
    ],
    tags: ['database', 'mysql', 'sql'],
  },
  {
    id: 'mongodb-mcp',
    name: 'MongoDB MCP',
    description: 'MongoDB integration for querying and analyzing collections',
    category: 'database',
    repository: 'https://github.com/kiliczsh/mcp-mongo-server',
    installationType: 'npx',
    npmPackage: 'mcp-mongo-server',
    installConfig: {
      command: 'npx',
      args: ['-y', 'mcp-mongo-server'],
    },
    requiredEnvVars: [
      {
        name: 'MONGODB_URI',
        description: 'MongoDB connection URI',
        example: 'mongodb://localhost:27017/mydb',
      },
    ],
    tags: ['database', 'mongodb', 'nosql'],
  },
  {
    id: 'redis-mcp',
    name: 'Redis MCP',
    description:
      'Natural language interface for managing and searching data in Redis',
    category: 'database',
    repository: 'https://github.com/redis/mcp-redis',
    website: 'https://redis.io',
    installationType: 'pip',
    pipPackage: 'redis-mcp-server',
    official: true,
    installConfig: {
      command: 'uvx',
      args: ['redis-mcp-server'],
    },
    requiredEnvVars: [
      {
        name: 'REDIS_URL',
        description: 'Redis connection URL',
        example: 'redis://localhost:6379',
      },
    ],
    tags: ['database', 'redis', 'cache', 'official'],
  },
  {
    id: 'neon-mcp',
    name: 'Neon MCP',
    description:
      'Neon Serverless Postgres - create and manage databases with natural language',
    category: 'database',
    repository: 'https://github.com/neondatabase/mcp-server-neon',
    website: 'https://neon.tech',
    installationType: 'npx',
    npmPackage: '@neondatabase/mcp-server-neon',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@neondatabase/mcp-server-neon'],
    },
    requiredEnvVars: [{ name: 'NEON_API_KEY', description: 'Neon API key' }],
    tags: ['database', 'postgres', 'serverless', 'official'],
  },
  {
    id: 'qdrant-mcp',
    name: 'Qdrant MCP',
    description: 'Vector search engine for keeping and retrieving memories',
    category: 'database',
    repository: 'https://github.com/qdrant/mcp-server-qdrant',
    website: 'https://qdrant.tech',
    installationType: 'pip',
    pipPackage: 'mcp-server-qdrant',
    official: true,
    installConfig: {
      command: 'uvx',
      args: ['mcp-server-qdrant'],
    },
    requiredEnvVars: [
      { name: 'QDRANT_URL', description: 'Qdrant server URL' },
      { name: 'QDRANT_API_KEY', description: 'Qdrant API key (optional)' },
    ],
    tags: ['database', 'vector', 'search', 'official'],
  },
  {
    id: 'snowflake-mcp',
    name: 'Snowflake MCP',
    description:
      'Snowflake data warehouse integration with read/write capabilities',
    category: 'database',
    repository: 'https://github.com/isaacwasserman/mcp-snowflake-server',
    website: 'https://snowflake.com',
    installationType: 'pip',
    pipPackage: 'mcp-snowflake-server',
    installConfig: {
      command: 'uvx',
      args: ['mcp-snowflake-server'],
    },
    requiredEnvVars: [
      {
        name: 'SNOWFLAKE_ACCOUNT',
        description: 'Snowflake account identifier',
      },
      { name: 'SNOWFLAKE_USER', description: 'Snowflake username' },
      { name: 'SNOWFLAKE_PASSWORD', description: 'Snowflake password' },
    ],
    tags: ['database', 'snowflake', 'data-warehouse'],
  },
  {
    id: 'bigquery-mcp',
    name: 'BigQuery MCP',
    description:
      'Google BigQuery integration for schema inspection and queries',
    category: 'database',
    repository: 'https://github.com/LucasHild/mcp-server-bigquery',
    website: 'https://cloud.google.com/bigquery',
    installationType: 'pip',
    pipPackage: 'mcp-server-bigquery',
    installConfig: {
      command: 'uvx',
      args: ['mcp-server-bigquery'],
    },
    requiredEnvVars: [
      {
        name: 'GOOGLE_APPLICATION_CREDENTIALS',
        description: 'Path to GCP service account JSON',
      },
    ],
    tags: ['database', 'bigquery', 'google', 'analytics'],
  },
  {
    id: 'dbhub-mcp',
    name: 'DBHub MCP',
    description:
      'Zero-dependency, token-efficient database MCP for Postgres, MySQL, SQL Server, MariaDB, SQLite',
    category: 'database',
    repository: 'https://github.com/bytebase/dbhub',
    installationType: 'npx',
    npmPackage: 'dbhub',
    installConfig: {
      command: 'npx',
      args: ['-y', 'dbhub'],
    },
    requiredEnvVars: [
      {
        name: 'DATABASE_URL',
        description: 'Database connection URL',
        example: 'postgres://user:pass@localhost:5432/db',
      },
    ],
    tags: [
      'database',
      'postgres',
      'mysql',
      'sqlite',
      'mariadb',
      'sqlserver',
      'multi-database',
    ],
  },
  {
    id: 'pg-aiguide-mcp',
    name: 'PostgreSQL AI Guide MCP',
    description:
      'PostgreSQL documentation and best practices through semantic search and curated skills',
    category: 'database',
    repository: 'https://github.com/timescale/pg-aiguide',
    website: 'https://tigerdata.com',
    installationType: 'npx',
    npmPackage: '@tigerdata/pg-aiguide',
    installConfig: {
      command: 'npx',
      args: ['-y', '@tigerdata/pg-aiguide'],
    },
    tags: ['database', 'postgres', 'postgresql', 'documentation', 'timescale'],
  },
  {
    id: 'cloudflare-mcp',
    name: 'Cloudflare MCP',
    description: 'Integration with Cloudflare Workers, KV, R2, and D1',
    category: 'cloud-platform',
    repository: 'https://github.com/cloudflare/mcp-server-cloudflare',
    website: 'https://cloudflare.com',
    installationType: 'npx',
    npmPackage: '@cloudflare/mcp-server-cloudflare',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@cloudflare/mcp-server-cloudflare'],
    },
    requiredEnvVars: [
      { name: 'CLOUDFLARE_API_TOKEN', description: 'Cloudflare API Token' },
      { name: 'CLOUDFLARE_ACCOUNT_ID', description: 'Cloudflare Account ID' },
    ],
    tags: ['cloudflare', 'cloud', 'workers', 'official'],
  },
  {
    id: 'docker-mcp',
    name: 'Docker MCP',
    description: 'Docker operations for container and compose stack management',
    category: 'cloud-platform',
    repository: 'https://github.com/sondt2709/docker-mcp',
    installationType: 'npx',
    npmPackage: 'docker-mcp',
    installConfig: {
      command: 'npx',
      args: ['-y', 'docker-mcp'],
    },
    tags: ['docker', 'containers', 'devops'],
  },

  {
    id: 'filesystem-mcp',
    name: 'Filesystem MCP',
    description:
      'Direct local file system access with configurable permissions',
    category: 'file-system',
    repository:
      'https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem',
    installationType: 'npx',
    npmPackage: '@modelcontextprotocol/server-filesystem',
    official: true,
    installConfig: {
      command: 'npx',
      args: [
        '-y',
        '@modelcontextprotocol/server-filesystem',
        '${ALLOWED_DIRECTORIES}',
      ],
    },
    requiredEnvVars: [
      {
        name: 'ALLOWED_DIRECTORIES',
        description: 'Comma-separated list of allowed directories',
        example: '/home/user/projects,/tmp',
      },
    ],
    tags: ['filesystem', 'files', 'local', 'official'],
  },
  {
    id: 'git-mcp-official',
    name: 'Git MCP',
    description:
      'Official Git integration for repository operations, commits, and history',
    category: 'version-control',
    repository:
      'https://github.com/modelcontextprotocol/servers/tree/main/src/git',
    installationType: 'pip',
    pipPackage: 'mcp-server-git',
    official: true,
    installConfig: {
      command: 'uvx',
      args: ['mcp-server-git'],
    },
    tags: ['git', 'version-control', 'repository', 'official'],
  },
  {
    id: 'memory-mcp',
    name: 'Memory MCP',
    description:
      'Official memory/persistence server for maintaining context across sessions',
    category: 'developer-tools',
    repository:
      'https://github.com/modelcontextprotocol/servers/tree/main/src/memory',
    installationType: 'npx',
    npmPackage: '@modelcontextprotocol/server-memory',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@modelcontextprotocol/server-memory'],
    },
    tags: ['memory', 'persistence', 'context', 'official'],
  },
  {
    id: 'fetch-mcp',
    name: 'Fetch MCP',
    description: 'Official HTTP fetch server for making web requests',
    category: 'developer-tools',
    repository:
      'https://github.com/modelcontextprotocol/servers/tree/main/src/fetch',
    installationType: 'pip',
    pipPackage: 'mcp-server-fetch',
    official: true,
    installConfig: {
      command: 'uvx',
      args: ['mcp-server-fetch'],
    },
    tags: ['fetch', 'http', 'web', 'requests', 'official'],
  },
  {
    id: 'time-mcp',
    name: 'Time MCP',
    description: 'Official time server for date/time operations and timezones',
    category: 'developer-tools',
    repository:
      'https://github.com/modelcontextprotocol/servers/tree/main/src/time',
    installationType: 'pip',
    pipPackage: 'mcp-server-time',
    official: true,
    installConfig: {
      command: 'uvx',
      args: ['mcp-server-time'],
    },
    tags: ['time', 'date', 'timezone', 'official'],
  },
  {
    id: 'sequential-thinking-mcp',
    name: 'Sequential Thinking MCP',
    description:
      'Official server for step-by-step reasoning and problem solving',
    category: 'developer-tools',
    repository:
      'https://github.com/modelcontextprotocol/servers/tree/main/src/sequentialthinking',
    installationType: 'npx',
    npmPackage: '@modelcontextprotocol/server-sequential-thinking',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@modelcontextprotocol/server-sequential-thinking'],
    },
    tags: ['reasoning', 'thinking', 'problem-solving', 'official'],
  },
  {
    id: 'everything-mcp',
    name: 'Everything MCP',
    description:
      'Official demo server showcasing all MCP capabilities in one package',
    category: 'developer-tools',
    repository:
      'https://github.com/modelcontextprotocol/servers/tree/main/src/everything',
    installationType: 'npx',
    npmPackage: '@modelcontextprotocol/server-everything',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@modelcontextprotocol/server-everything'],
    },
    tags: ['demo', 'all-in-one', 'example', 'official'],
  },
  {
    id: 'exa-mcp',
    name: 'Exa Search MCP',
    description: 'Exa AI Search API for real-time web information retrieval',
    category: 'search-web',
    repository: 'https://github.com/exa-labs/exa-mcp-server',
    website: 'https://exa.ai',
    installationType: 'npx',
    npmPackage: 'exa-mcp-server',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', 'exa-mcp-server'],
    },
    requiredEnvVars: [{ name: 'EXA_API_KEY', description: 'Exa API key' }],
    tags: ['search', 'exa', 'ai', 'official'],
  },
  {
    id: 'perplexity-mcp',
    name: 'Perplexity MCP',
    description: 'Official Perplexity AI search and research integration',
    category: 'search-web',
    repository: 'https://github.com/perplexityai/modelcontextprotocol',
    website: 'https://perplexity.ai',
    installationType: 'npx',
    npmPackage: '@perplexity-ai/mcp-server',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@perplexity-ai/mcp-server'],
    },
    requiredEnvVars: [
      { name: 'PERPLEXITY_API_KEY', description: 'Perplexity API key' },
    ],
    tags: ['search', 'perplexity', 'ai', 'research', 'official'],
  },
  {
    id: 'kagi-mcp',
    name: 'Kagi MCP',
    description: 'Official Kagi search and tools integration',
    category: 'search-web',
    repository: 'https://github.com/kagisearch/kagi-mcp',
    website: 'https://kagi.com',
    installationType: 'pip',
    pipPackage: 'kagimcp',
    official: true,
    installConfig: {
      command: 'uvx',
      args: ['kagimcp'],
    },
    requiredEnvVars: [{ name: 'KAGI_API_KEY', description: 'Kagi API key' }],
    tags: ['search', 'kagi', 'official'],
  },
  {
    id: 'tavily-mcp',
    name: 'Tavily MCP',
    description: 'Tavily AI search API for intelligent web search',
    category: 'search-web',
    repository: 'https://github.com/tavily-ai/tavily-mcp',
    website: 'https://tavily.com',
    installationType: 'npx',
    npmPackage: 'tavily-mcp',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', 'tavily-mcp'],
    },
    requiredEnvVars: [
      { name: 'TAVILY_API_KEY', description: 'Tavily API key' },
    ],
    tags: ['search', 'tavily', 'ai', 'official'],
  },

  {
    id: 'slack-mcp',
    name: 'Slack MCP',
    description:
      'Most powerful MCP server for Slack with Stdio and SSE transports',
    category: 'communication',
    repository: 'https://github.com/korotovsky/slack-mcp-server',
    website: 'https://slack.com',
    installationType: 'npx',
    npmPackage: 'slack-mcp-server',
    installConfig: {
      command: 'npx',
      args: ['-y', 'slack-mcp-server'],
    },
    requiredEnvVars: [
      { name: 'SLACK_BOT_TOKEN', description: 'Slack Bot OAuth Token' },
    ],
    tags: ['slack', 'chat', 'team'],
  },
  {
    id: 'discord-mcp',
    name: 'Discord MCP',
    description:
      'Discord API integration via JDA for seamless bot interaction with MCP clients',
    category: 'communication',
    repository: 'https://github.com/SaseQ/discord-mcp',
    website: 'https://discord.com',
    installationType: 'docker',
    dockerImage: 'saseq/discord-mcp:latest',
    installConfig: {
      command: 'docker',
      args: [
        'run',
        '--rm',
        '-i',
        '-e',
        'DISCORD_TOKEN',
        '-e',
        'DISCORD_GUILD_ID',
        'saseq/discord-mcp:latest',
      ],
    },
    requiredEnvVars: [
      { name: 'DISCORD_TOKEN', description: 'Discord Bot Token' },
      {
        name: 'DISCORD_GUILD_ID',
        description: 'Default Discord Server ID (optional)',
      },
    ],
    tags: ['discord', 'chat', 'community', 'bot'],
  },
  {
    id: 'linear-mcp',
    name: 'Linear MCP',
    description:
      'Linear project management integration for issues, projects, and teams',
    category: 'communication',
    repository: 'https://github.com/tacticlaunch/mcp-linear',
    website: 'https://linear.app',
    installationType: 'npx',
    npmPackage: '@tacticlaunch/mcp-linear',
    installConfig: {
      command: 'npx',
      args: ['-y', '@tacticlaunch/mcp-linear'],
    },
    requiredEnvVars: [
      { name: 'LINEAR_API_KEY', description: 'Linear API key' },
    ],
    tags: ['linear', 'issues', 'project-management'],
  },
  {
    id: 'atlassian-mcp',
    name: 'Atlassian MCP',
    description:
      'Confluence and Jira integration for documentation and issue tracking',
    category: 'communication',
    repository: 'https://github.com/sooperset/mcp-atlassian',
    website: 'https://atlassian.com',
    installationType: 'pip',
    pipPackage: 'mcp-atlassian',
    installConfig: {
      command: 'uvx',
      args: ['mcp-atlassian'],
    },
    requiredEnvVars: [
      { name: 'ATLASSIAN_URL', description: 'Atlassian instance URL' },
      { name: 'ATLASSIAN_EMAIL', description: 'Atlassian email' },
      { name: 'ATLASSIAN_API_TOKEN', description: 'Atlassian API token' },
    ],
    tags: ['atlassian', 'jira', 'confluence'],
  },

  {
    id: 'notion-mcp',
    name: 'Notion MCP',
    description: 'Official Notion integration for pages, databases, and blocks',
    category: 'productivity',
    repository: 'https://github.com/makenotion/notion-mcp-server',
    website: 'https://notion.so',
    installationType: 'npx',
    npmPackage: '@notionhq/notion-mcp-server',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@notionhq/notion-mcp-server'],
    },
    requiredEnvVars: [
      { name: 'NOTION_API_KEY', description: 'Notion API integration token' },
    ],
    tags: ['notion', 'productivity', 'notes', 'database', 'official'],
  },
  {
    id: 'obsidian-mcp',
    name: 'Obsidian MCP',
    description:
      'Obsidian vault integration for file management, search, and manipulation',
    category: 'productivity',
    repository: 'https://github.com/MarkusPfundstein/mcp-obsidian',
    website: 'https://obsidian.md',
    installationType: 'npx',
    npmPackage: 'mcp-obsidian',
    installConfig: {
      command: 'npx',
      args: ['-y', 'mcp-obsidian'],
    },
    requiredEnvVars: [
      { name: 'OBSIDIAN_VAULT_PATH', description: 'Path to Obsidian vault' },
    ],
    tags: ['obsidian', 'notes', 'markdown'],
  },
  {
    id: 'todoist-mcp',
    name: 'Todoist MCP',
    description: 'Natural language task management with Todoist',
    category: 'productivity',
    repository: 'https://github.com/stevengonsalvez/todoist-mcp',
    website: 'https://todoist.com',
    installationType: 'npx',
    npmPackage: 'todoist-mcp-server',
    installConfig: {
      command: 'npx',
      args: ['-y', 'todoist-mcp-server'],
    },
    requiredEnvVars: [
      { name: 'TODOIST_API_TOKEN', description: 'Todoist API token' },
    ],
    tags: ['todoist', 'tasks', 'productivity'],
  },
  {
    id: 'google-workspace-mcp',
    name: 'Google Workspace MCP',
    description:
      'Control Gmail, Calendar, Docs, Sheets, Slides, Chat, Drive and more',
    category: 'productivity',
    repository: 'https://github.com/taylorwilsdon/google_workspace_mcp',
    website: 'https://workspace.google.com',
    installationType: 'pip',
    pipPackage: 'google-workspace-mcp',
    installConfig: {
      command: 'uvx',
      args: ['google-workspace-mcp'],
    },
    requiredEnvVars: [
      {
        name: 'GOOGLE_APPLICATION_CREDENTIALS',
        description: 'Path to Google credentials',
      },
    ],
    tags: ['google', 'workspace', 'gmail', 'docs'],
  },

  {
    id: 'context7-mcp',
    name: 'Context7 MCP',
    description: 'Up-to-date code documentation for LLMs and AI code editors',
    category: 'developer-tools',
    repository: 'https://github.com/upstash/context7',
    website: 'https://context7.com',
    installationType: 'npx',
    npmPackage: '@upstash/context7-mcp',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@upstash/context7-mcp'],
    },
    tags: ['documentation', 'context', 'code', 'official'],
  },
  {
    id: 'desktop-commander-mcp',
    name: 'Desktop Commander MCP',
    description:
      'Terminal control, file system search, and diff file editing capabilities',
    category: 'developer-tools',
    repository: 'https://github.com/wonderwhy-er/DesktopCommanderMCP',
    installationType: 'npx',
    npmPackage: '@wonderwhy-er/desktop-commander',
    installConfig: {
      command: 'npx',
      args: ['-y', '@wonderwhy-er/desktop-commander'],
    },
    tags: ['terminal', 'file-editing', 'diff', 'command-line', 'process'],
  },
  {
    id: 'gemini-mcp-tool',
    name: 'Gemini MCP Tool',
    description:
      'Interact with Google Gemini CLI for large file analysis and codebase exploration',
    category: 'ai-services',
    repository: 'https://github.com/jamubc/gemini-mcp-tool',
    installationType: 'npx',
    npmPackage: 'gemini-mcp-tool',
    installConfig: {
      command: 'npx',
      args: ['-y', 'gemini-mcp-tool'],
    },
    tags: ['gemini', 'google', 'ai', 'file-analysis', 'codebase'],
  },
  {
    id: 'sentry-mcp',
    name: 'Sentry MCP',
    description:
      'Sentry.io integration for error tracking and performance monitoring',
    category: 'developer-tools',
    repository: 'https://github.com/getsentry/sentry-mcp',
    website: 'https://sentry.io',
    installationType: 'npx',
    npmPackage: '@sentry/mcp-server',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@sentry/mcp-server'],
    },
    requiredEnvVars: [
      { name: 'SENTRY_AUTH_TOKEN', description: 'Sentry Auth Token' },
      { name: 'SENTRY_ORG', description: 'Sentry Organization Slug' },
    ],
    tags: ['sentry', 'errors', 'monitoring', 'official'],
  },
  {
    id: 'serena-mcp',
    name: 'Serena MCP',
    description:
      'Powerful coding agent with semantic retrieval and editing via language servers',
    category: 'coding-agents',
    repository: 'https://github.com/oraios/serena',
    installationType: 'pip',
    installConfig: {
      command: 'uvx',
      args: [
        '--from',
        'git+https://github.com/oraios/serena',
        'serena',
        'start-mcp-server',
      ],
    },
    tags: ['coding', 'agent', 'language-server'],
  },
  {
    id: 'figma-mcp',
    name: 'Figma Context MCP',
    description: 'Get Figma design data in ready-to-implement format',
    category: 'developer-tools',
    repository: 'https://github.com/GLips/Figma-Context-MCP',
    website: 'https://figma.com',
    installationType: 'npx',
    npmPackage: 'figma-context-mcp',
    installConfig: {
      command: 'npx',
      args: ['-y', 'figma-context-mcp'],
    },
    requiredEnvVars: [
      {
        name: 'FIGMA_ACCESS_TOKEN',
        description: 'Figma Personal Access Token',
      },
    ],
    tags: ['figma', 'design', 'ui'],
  },
  {
    id: 'talk-to-figma-mcp',
    name: 'Talk to Figma MCP',
    description: 'Cursor + Figma integration for reading and modifying designs',
    category: 'developer-tools',
    repository: 'https://github.com/grab/cursor-talk-to-figma-mcp',
    website: 'https://figma.com',
    installationType: 'npx',
    npmPackage: 'cursor-talk-to-figma-mcp',
    installConfig: {
      command: 'npx',
      args: ['-y', 'cursor-talk-to-figma-mcp'],
    },
    requiredEnvVars: [
      {
        name: 'FIGMA_ACCESS_TOKEN',
        description: 'Figma Personal Access Token',
      },
    ],
    tags: ['figma', 'cursor', 'design'],
  },
  {
    id: 'xcodebuild-mcp',
    name: 'XcodeBuild MCP',
    description:
      'Xcode-related tools for building, testing, and managing iOS/macOS projects',
    category: 'developer-tools',
    repository: 'https://github.com/cameroncooke/XcodeBuildMCP',
    website: 'https://developer.apple.com/xcode',
    installationType: 'npx',
    npmPackage: 'xcodebuildmcp',
    installConfig: {
      command: 'npx',
      args: ['-y', 'xcodebuildmcp'],
    },
    tags: ['xcode', 'ios', 'macos', 'apple', 'swift'],
  },
  {
    id: 'markdownify-mcp',
    name: 'Markdownify MCP',
    description:
      'Convert almost anything to Markdown - PDFs, URLs, images, and more',
    category: 'developer-tools',
    repository: 'https://github.com/zcaceres/markdownify-mcp',
    installationType: 'npx',
    npmPackage: 'mcp-markdownify-server',
    installConfig: {
      command: 'npx',
      args: ['-y', 'mcp-markdownify-server'],
    },
    tags: ['markdown', 'converter', 'pdf', 'ocr'],
  },

  {
    id: 'elevenlabs-mcp',
    name: 'ElevenLabs MCP',
    description:
      'Official ElevenLabs text-to-speech, voice cloning, and audio generation',
    category: 'ai-services',
    repository: 'https://github.com/elevenlabs/elevenlabs-mcp',
    website: 'https://elevenlabs.io',
    installationType: 'pip',
    pipPackage: 'elevenlabs-mcp',
    official: true,
    installConfig: {
      command: 'uvx',
      args: ['elevenlabs-mcp'],
    },
    requiredEnvVars: [
      { name: 'ELEVENLABS_API_KEY', description: 'ElevenLabs API key' },
    ],
    tags: ['elevenlabs', 'tts', 'voice', 'audio', 'official'],
  },
  {
    id: 'llamacloud-mcp',
    name: 'LlamaCloud MCP',
    description: 'Connect to managed indices on LlamaCloud',
    category: 'ai-services',
    repository: 'https://github.com/run-llama/mcp-server-llamacloud',
    website: 'https://cloud.llamaindex.ai',
    installationType: 'npx',
    npmPackage: '@llamaindex/mcp-server-llamacloud',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@llamaindex/mcp-server-llamacloud'],
    },
    requiredEnvVars: [
      { name: 'LLAMA_CLOUD_API_KEY', description: 'LlamaCloud API key' },
    ],
    tags: ['llamaindex', 'rag', 'index', 'official'],
  },
  {
    id: 'n8n-mcp',
    name: 'n8n MCP',
    description:
      'Build n8n workflows using Claude Desktop, Cursor, or Windsurf',
    category: 'workflow-automation',
    repository: 'https://github.com/czlonkowski/n8n-mcp',
    website: 'https://n8n.io',
    installationType: 'npx',
    npmPackage: 'n8n-mcp',
    installConfig: {
      command: 'npx',
      args: ['-y', 'n8n-mcp'],
    },
    requiredEnvVars: [
      { name: 'N8N_API_URL', description: 'n8n instance URL' },
      { name: 'N8N_API_KEY', description: 'n8n API key' },
    ],
    tags: ['n8n', 'automation', 'workflows'],
  },

  {
    id: 'echarts-mcp',
    name: 'ECharts MCP',
    description: 'Generate visual charts using Apache ECharts dynamically',
    category: 'data-visualization',
    repository: 'https://github.com/hustcc/mcp-echarts',
    website: 'https://echarts.apache.org',
    installationType: 'npx',
    npmPackage: 'mcp-echarts',
    installConfig: {
      command: 'npx',
      args: ['-y', 'mcp-echarts'],
    },
    tags: ['visualization', 'charts', 'echarts'],
  },
  {
    id: 'mermaid-mcp',
    name: 'Mermaid MCP',
    description: 'Generate Mermaid diagrams and charts dynamically',
    category: 'data-visualization',
    repository: 'https://github.com/hustcc/mcp-mermaid',
    website: 'https://mermaid.js.org',
    installationType: 'npx',
    npmPackage: 'mcp-mermaid',
    installConfig: {
      command: 'npx',
      args: ['-y', 'mcp-mermaid'],
    },
    tags: ['diagrams', 'mermaid', 'flowcharts'],
  },

  {
    id: 'datadog-mcp',
    name: 'Datadog MCP',
    description: 'Datadog monitoring and observability integration',
    category: 'monitoring',
    repository: 'https://github.com/winor30/mcp-server-datadog',
    website: 'https://datadoghq.com',
    installationType: 'npx',
    npmPackage: '@winor30/mcp-server-datadog',
    installConfig: {
      command: 'npx',
      args: ['-y', '@winor30/mcp-server-datadog'],
    },
    requiredEnvVars: [
      { name: 'DD_API_KEY', description: 'Datadog API key' },
      { name: 'DD_APP_KEY', description: 'Datadog Application key' },
    ],
    tags: ['datadog', 'monitoring', 'observability'],
  },
  {
    id: 'browserstack-mcp',
    name: 'BrowserStack MCP',
    description: 'Official BrowserStack testing and automation integration',
    category: 'developer-tools',
    repository: 'https://github.com/browserstack/mcp-server',
    website: 'https://browserstack.com',
    installationType: 'npx',
    npmPackage: '@browserstack/mcp-server',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@browserstack/mcp-server'],
    },
    requiredEnvVars: [
      { name: 'BROWSERSTACK_USERNAME', description: 'BrowserStack username' },
      {
        name: 'BROWSERSTACK_ACCESS_KEY',
        description: 'BrowserStack access key',
      },
    ],
    tags: ['browserstack', 'testing', 'automation', 'official'],
  },
  {
    id: 'tableau-mcp',
    name: 'Tableau MCP',
    description:
      'Official Tableau data visualization and analytics integration',
    category: 'data-visualization',
    repository: 'https://github.com/tableau/tableau-mcp',
    website: 'https://tableau.com',
    installationType: 'npx',
    npmPackage: '@tableau/mcp-server',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@tableau/mcp-server'],
    },
    requiredEnvVars: [
      { name: 'TABLEAU_TOKEN', description: 'Tableau API token' },
      { name: 'TABLEAU_SITE', description: 'Tableau site name' },
    ],
    tags: ['tableau', 'visualization', 'analytics', 'official'],
  },

  {
    id: 'stripe-mcp',
    name: 'Stripe MCP',
    description: 'Official Stripe Agent Toolkit for payment integration',
    category: 'finance',
    repository: 'https://github.com/stripe/agent-toolkit',
    website: 'https://stripe.com',
    installationType: 'npx',
    npmPackage: '@stripe/agent-toolkit',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@stripe/agent-toolkit', 'mcp'],
    },
    requiredEnvVars: [
      { name: 'STRIPE_SECRET_KEY', description: 'Stripe Secret Key' },
    ],
    tags: ['stripe', 'payments', 'finance', 'official'],
  },
  {
    id: 'alpaca-mcp',
    name: 'Alpaca MCP',
    description:
      'Official Alpaca trading - stocks, ETFs, crypto, and options in plain English',
    category: 'finance',
    repository: 'https://github.com/alpacahq/alpaca-mcp-server',
    website: 'https://alpaca.markets',
    installationType: 'pip',
    pipPackage: 'alpaca-mcp-server',
    official: true,
    installConfig: {
      command: 'uvx',
      args: ['alpaca-mcp-server'],
    },
    requiredEnvVars: [
      { name: 'ALPACA_API_KEY', description: 'Alpaca API key' },
      { name: 'ALPACA_SECRET_KEY', description: 'Alpaca secret key' },
    ],
    tags: ['alpaca', 'trading', 'stocks', 'crypto', 'finance', 'official'],
  },
  {
    id: 'paypal-mcp',
    name: 'PayPal MCP',
    description: 'PayPal Agent Toolkit for payment integration',
    category: 'finance',
    repository: 'https://github.com/paypal/agent-toolkit',
    website: 'https://paypal.com',
    installationType: 'npx',
    npmPackage: '@paypal/agent-toolkit',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@paypal/agent-toolkit'],
    },
    requiredEnvVars: [
      { name: 'PAYPAL_CLIENT_ID', description: 'PayPal Client ID' },
      { name: 'PAYPAL_CLIENT_SECRET', description: 'PayPal Client Secret' },
    ],
    tags: ['paypal', 'payments', 'finance', 'official'],
  },

  {
    id: 'semgrep-mcp',
    name: 'Semgrep MCP',
    description:
      'Static code analysis and security scanning with Semgrep integration',
    category: 'security',
    repository: 'https://github.com/Szowesgad/mcp-server-semgrep',
    website: 'https://semgrep.dev',
    installationType: 'npx',
    npmPackage: 'mcp-server-semgrep',
    installConfig: {
      command: 'npx',
      args: ['-y', 'mcp-server-semgrep'],
    },
    tags: ['semgrep', 'security', 'sast', 'static-analysis'],
  },
  {
    id: 'pipedream-mcp',
    name: 'Pipedream MCP',
    description: 'Connect with 2,500+ APIs with 8,000+ prebuilt tools',
    category: 'aggregator',
    repository:
      'https://github.com/PipedreamHQ/pipedream/tree/master/modelcontextprotocol',
    website: 'https://pipedream.com',
    installationType: 'npx',
    npmPackage: '@pipedream/mcp',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@pipedream/mcp'],
    },
    requiredEnvVars: [
      { name: 'PIPEDREAM_API_KEY', description: 'Pipedream API key' },
    ],
    tags: ['pipedream', 'integrations', 'apis', 'official'],
  },

  {
    id: 'fastmcp',
    name: 'FastMCP',
    description: 'The fast, Pythonic way to build MCP servers and clients',
    category: 'developer-tools',
    repository: 'https://github.com/jlowin/fastmcp',
    installationType: 'pip',
    pipPackage: 'fastmcp',
    installConfig: {
      command: 'pip',
      args: ['install', 'fastmcp'],
    },
    tags: ['framework', 'python', 'sdk'],
  },
  {
    id: 'mcp-use',
    name: 'MCP Use',
    description: 'Easiest way to interact with MCP servers with custom agents',
    category: 'developer-tools',
    repository: 'https://github.com/mcp-use/mcp-use',
    installationType: 'pip',
    pipPackage: 'mcp-use',
    installConfig: {
      command: 'pip',
      args: ['install', 'mcp-use'],
    },
    tags: ['framework', 'agents', 'python'],
  },

  {
    id: 'unity-mcp',
    name: 'Unity MCP',
    description:
      'MCP server for Unity Editor - designed for Claude, Cursor, Gemini and more',
    category: 'developer-tools',
    repository: 'https://github.com/CoplayDev/unity-mcp',
    website: 'https://unity.com',
    installationType: 'pip',
    installConfig: {
      command: 'uvx',
      args: [
        '--from',
        'git+https://github.com/CoplayDev/unity-mcp',
        'unity-mcp',
      ],
    },
    tags: ['unity', 'game-dev', 'editor', 'gamedev'],
  },

  {
    id: 'arxiv-mcp',
    name: 'ArXiv MCP',
    description: 'Search ArXiv research papers',
    category: 'other',
    repository: 'https://github.com/blazickjp/arxiv-mcp-server',
    website: 'https://arxiv.org',
    installationType: 'pip',
    pipPackage: 'arxiv-mcp-server',
    installConfig: {
      command: 'uvx',
      args: ['arxiv-mcp-server'],
    },
    tags: ['arxiv', 'research', 'papers'],
  },

  {
    id: 'octocode-mcp',
    name: 'Octocode MCP',
    description: 'AI-powered developer assistant for GitHub and NPM research',
    category: 'developer-tools',
    repository: 'https://github.com/bgauryy/octocode-mcp',
    installationType: 'npx',
    npmPackage: 'octocode-mcp',
    installConfig: {
      command: 'npx',
      args: ['-y', 'octocode-mcp@latest'],
    },
    requiredEnvVars: [
      {
        name: 'GITHUB_TOKEN',
        description: 'GitHub token (also accepts OCTOCODE_TOKEN or GH_TOKEN)',
      },
    ],
    tags: ['github', 'npm', 'research', 'code'],
  },
  {
    id: 'gpt-researcher-mcp',
    name: 'GPT Researcher MCP',
    description:
      'Deep research agent with MCP support for comprehensive web research and report generation',
    category: 'search-web',
    repository: 'https://github.com/assafelovic/gptr-mcp',
    website: 'https://gptr.dev',
    stars: 24750,
    installationType: 'source',
    installConfig: {
      command: 'python',
      args: ['server.py'],
    },
    requiredEnvVars: [
      {
        name: 'OPENAI_API_KEY',
        description: 'OpenAI API key for LLM capabilities',
        example: 'sk-xxxxx',
      },
      {
        name: 'TAVILY_API_KEY',
        description: 'Tavily API key for web search',
        example: 'tvly-xxxxx',
      },
    ],
    tags: ['research', 'web-search', 'deep-research', 'ai', 'tavily'],
  },
  {
    id: 'mcp-chrome',
    name: 'Chrome MCP',
    description:
      'Chrome extension-based MCP server for browser automation, screenshots, and content analysis',
    category: 'browser-automation',
    repository: 'https://github.com/hangwin/mcp-chrome',
    stars: 9803,
    installationType: 'npx',
    npmPackage: 'mcp-chrome-bridge',
    installConfig: {
      command: 'npx',
      args: ['-y', 'mcp-chrome-bridge'],
    },
    tags: ['chrome', 'browser', 'automation', 'extension', 'screenshot'],
  },
  {
    id: 'aws-mcp',
    name: 'AWS Core MCP',
    description:
      'AWS MCP Servers - dynamic orchestrator for specialized AWS services including Bedrock, Lambda, S3, and CloudWatch',
    category: 'cloud-platform',
    repository: 'https://github.com/awslabs/mcp',
    website: 'https://awslabs.github.io/mcp/',
    stars: 7801,
    installationType: 'pip',
    pipPackage: 'awslabs.core-mcp-server',
    official: true,
    installConfig: {
      command: 'uvx',
      args: ['awslabs.core-mcp-server@latest'],
    },
    requiredEnvVars: [
      {
        name: 'AWS_PROFILE',
        description: 'AWS profile name for credentials',
        example: 'default',
      },
      {
        name: 'AWS_REGION',
        description: 'AWS region',
        example: 'us-east-1',
      },
    ],
    tags: ['aws', 'cloud', 'bedrock', 'lambda', 's3', 'cloudwatch', 'official'],
  },
  {
    id: 'git-mcp',
    name: 'GitMCP',
    description:
      'Remote MCP server that transforms GitHub projects into documentation hubs for AI tools',
    category: 'version-control',
    repository: 'https://github.com/idosal/git-mcp',
    website: 'https://gitmcp.io',
    stars: 7328,
    installationType: 'npx',
    npmPackage: 'mcp-remote',
    installConfig: {
      command: 'npx',
      args: ['-y', 'mcp-remote', 'https://gitmcp.io/docs'],
    },
    tags: ['git', 'github', 'documentation', 'remote', 'version-control'],
  },
  {
    id: 'browser-tools-mcp',
    name: 'BrowserTools MCP',
    description:
      'Monitor browser diagnostics and capture browser data from Cursor and MCP-compatible IDEs via Chrome extension',
    category: 'browser-automation',
    repository: 'https://github.com/AgentDeskAI/browser-tools-mcp',
    website: 'https://browsertools.agentdesk.ai',
    stars: 6961,
    installationType: 'npx',
    npmPackage: '@agentdeskai/browser-tools-mcp',
    installConfig: {
      command: 'npx',
      args: ['-y', '@agentdeskai/browser-tools-mcp@latest'],
    },
    tags: ['browser', 'devtools', 'chrome', 'diagnostics', 'debugging'],
  },
  {
    id: 'hexstrike-ai',
    name: 'HexStrike AI',
    description:
      'Advanced MCP server for AI agents to run 150+ cybersecurity tools for pentesting and vulnerability discovery',
    category: 'security',
    repository: 'https://github.com/0x4m4/hexstrike-ai',
    stars: 5690,
    installationType: 'source',
    installConfig: {
      command: 'python3',
      args: ['hexstrike_mcp.py', '--server', 'http://localhost:8888'],
    },
    requiredEnvVars: [
      {
        name: 'HEXSTRIKE_PATH',
        description: 'Path to cloned hexstrike-ai repository',
        example: '/path/to/hexstrike-ai',
      },
    ],
    tags: ['security', 'pentesting', 'cybersecurity', 'vulnerability'],
  },

  {
    id: 'spec-workflow-mcp',
    name: 'Spec Workflow MCP',
    description:
      'Structured spec-driven development workflow tools for AI-assisted software development with real-time web dashboard',
    category: 'developer-tools',
    repository: 'https://github.com/Pimzino/spec-workflow-mcp',
    stars: 3569,
    installationType: 'npx',
    npmPackage: '@pimzino/spec-workflow-mcp',
    installConfig: {
      command: 'npx',
      args: ['-y', '@pimzino/spec-workflow-mcp@latest', '${PROJECT_PATH}'],
    },
    requiredEnvVars: [
      {
        name: 'PROJECT_PATH',
        description: 'Path to your project directory',
        example: '/path/to/your/project',
      },
    ],
    tags: ['workflow', 'spec-driven', 'dashboard', 'vscode', 'development'],
  },
  {
    id: 'excel-mcp-server',
    name: 'Excel MCP',
    description:
      'Manipulate Excel files without Microsoft Excel - create, read, modify workbooks with formulas, formatting, and charts',
    category: 'productivity',
    repository: 'https://github.com/haris-musa/excel-mcp-server',
    stars: 3079,
    installationType: 'pip',
    pipPackage: 'excel-mcp-server',
    installConfig: {
      command: 'uvx',
      args: ['excel-mcp-server', 'stdio'],
    },
    tags: ['excel', 'spreadsheet', 'office', 'xlsx', 'formulas', 'charts'],
  },
  {
    id: 'mcp-context-forge',
    name: 'MCP Context Forge',
    description:
      'IBM MCP Gateway & Registry - federates MCP and REST services with discovery, auth, rate-limiting, and observability',
    category: 'aggregator',
    repository: 'https://github.com/IBM/mcp-context-forge',
    website: 'https://ibm.github.io/mcp-context-forge/',
    stars: 3056,
    installationType: 'pip',
    pipPackage: 'mcp-contextforge-gateway',
    official: true,
    installConfig: {
      command: 'uvx',
      args: [
        '--from',
        'mcp-contextforge-gateway',
        'mcpgateway',
        '--host',
        '0.0.0.0',
        '--port',
        '4444',
      ],
    },
    requiredEnvVars: [
      {
        name: 'JWT_SECRET_KEY',
        description: 'JWT secret key for authentication',
        example: 'my-secret-key',
      },
    ],
    tags: [
      'ibm',
      'gateway',
      'registry',
      'federation',
      'proxy',
      'official',
      'enterprise',
    ],
  },
  {
    id: 'mobile-mcp',
    name: 'Mobile MCP',
    description:
      'Scalable mobile automation and development through a platform-agnostic interface for iOS and Android devices',
    category: 'developer-tools',
    repository: 'https://github.com/mobile-next/mobile-mcp',
    stars: 2969,
    installationType: 'npx',
    npmPackage: '@mobilenext/mobile-mcp',
    installConfig: {
      command: 'npx',
      args: ['-y', '@mobilenext/mobile-mcp@latest'],
    },
    tags: [
      'mobile',
      'ios',
      'android',
      'automation',
      'simulator',
      'emulator',
      'testing',
    ],
  },
  {
    id: 'ros-mcp-server',
    name: 'ROS MCP Server',
    description:
      'Connect LLMs with ROS/ROS2 robots for natural language robot commanding and real-time sensor visibility',
    category: 'other',
    repository: 'https://github.com/robotmcp/ros-mcp-server',
    stars: 913,
    installationType: 'pip',
    pipPackage: 'ros-mcp-server',
    official: true,
    installConfig: {
      command: 'python',
      args: ['-m', 'ros_mcp_server.server'],
    },
    tags: ['ros', 'ros2', 'robotics', 'robots', 'sensors'],
  },
  {
    id: 'mcp-knowledge-graph',
    name: 'MCP Knowledge Graph',
    description:
      'Persistent memory for AI models through a local knowledge graph with entities, relations, and observations',
    category: 'developer-tools',
    repository: 'https://github.com/shaneholloman/mcp-knowledge-graph',
    stars: 764,
    installationType: 'npx',
    npmPackage: 'mcp-knowledge-graph',
    installConfig: {
      command: 'npx',
      args: ['-y', 'mcp-knowledge-graph', '--memory-path', '${MEMORY_PATH}'],
    },
    requiredEnvVars: [
      {
        name: 'MEMORY_PATH',
        description: 'Directory path where memory files are stored',
        example: '/Users/username/.aim',
      },
    ],
    tags: ['memory', 'knowledge-graph', 'persistence', 'context', 'entities'],
  },
  {
    id: 'kubectl-mcp',
    name: 'kubectl MCP',
    description:
      'Kubernetes cluster management for AI assistants with pod, service, Helm, and monitoring capabilities',
    category: 'cloud-platform',
    repository: 'https://github.com/rohitg00/kubectl-mcp-server',
    stars: 750,
    installationType: 'pip',
    pipPackage: 'kubectl-mcp-tool',
    installConfig: {
      command: 'python',
      args: ['-m', 'kubectl_mcp_tool.mcp_server'],
    },
    requiredEnvVars: [
      {
        name: 'KUBECONFIG',
        description: 'Path to Kubernetes config file',
        example: '~/.kube/config',
      },
    ],
    tags: ['kubernetes', 'k8s', 'kubectl', 'helm', 'cloud', 'devops'],
  },
  {
    id: 'apple-docs-mcp',
    name: 'Apple Docs MCP',
    description:
      'Access Apple developer documentation, frameworks, APIs, SwiftUI, UIKit, and WWDC videos through MCP',
    category: 'developer-tools',
    repository: 'https://github.com/kimsungwhee/apple-docs-mcp',
    website: 'https://developer.apple.com',
    stars: 697,
    installationType: 'npx',
    npmPackage: '@kimsungwhee/apple-docs-mcp',
    installConfig: {
      command: 'npx',
      args: ['-y', '@kimsungwhee/apple-docs-mcp'],
    },
    tags: [
      'apple',
      'documentation',
      'swiftui',
      'uikit',
      'wwdc',
      'ios',
      'macos',
    ],
  },
  {
    id: 'linkedin-mcp',
    name: 'LinkedIn MCP',
    description:
      'LinkedIn integration for AI assistants to scrape profiles, companies, and search jobs',
    category: 'social-media',
    repository: 'https://github.com/stickerdaniel/linkedin-mcp-server',
    website: 'https://linkedin.com',
    stars: 685,
    installationType: 'docker',
    dockerImage: 'stickerdaniel/linkedin-mcp-server:latest',
    installConfig: {
      command: 'docker',
      args: [
        'run',
        '--rm',
        '-i',
        '-e',
        'LINKEDIN_COOKIE',
        'stickerdaniel/linkedin-mcp-server:latest',
      ],
    },
    requiredEnvVars: [
      {
        name: 'LINKEDIN_COOKIE',
        description: 'LinkedIn session cookie for authentication',
        example: 'li_at=YOUR_COOKIE_VALUE',
      },
    ],
    tags: ['linkedin', 'social', 'jobs', 'profiles', 'scraping'],
  },
  {
    id: 'mcp-proxy',
    name: 'MCP Proxy',
    description:
      'Aggregates multiple MCP servers behind a single HTTP entrypoint with SSE and streamable HTTP support',
    category: 'aggregator',
    repository: 'https://github.com/TBXark/mcp-proxy',
    stars: 612,
    installationType: 'docker',
    dockerImage: 'ghcr.io/tbxark/mcp-proxy:latest',
    installConfig: {
      command: 'docker',
      args: [
        'run',
        '-i',
        '--rm',
        '-p',
        '9090:9090',
        '-v',
        '${MCP_PROXY_CONFIG}:/config/config.json',
        'ghcr.io/tbxark/mcp-proxy:latest',
      ],
    },
    requiredEnvVars: [
      {
        name: 'MCP_PROXY_CONFIG',
        description: 'Path to MCP proxy configuration JSON file',
        example: '/path/to/config.json',
      },
    ],
    tags: ['proxy', 'aggregator', 'sse', 'http', 'multi-server'],
  },
  {
    id: 'iterm-mcp',
    name: 'iTerm MCP',
    description:
      'Execute commands in active iTerm sessions with REPL interactions and CLI assistance',
    category: 'developer-tools',
    repository: 'https://github.com/ferrislucas/iterm-mcp',
    stars: 505,
    installationType: 'npx',
    npmPackage: 'iterm-mcp',
    installConfig: {
      command: 'npx',
      args: ['-y', 'iterm-mcp'],
    },
    tags: ['iterm', 'terminal', 'macos', 'repl', 'cli'],
  },
  {
    id: 'genai-toolbox-mcp',
    name: 'GenAI Toolbox MCP',
    description:
      "Google's official MCP Toolbox for Databases - Postgres, MySQL, MongoDB, Redis, BigQuery, Spanner, and more",
    category: 'database',
    repository: 'https://github.com/googleapis/genai-toolbox',
    website: 'https://googleapis.github.io/genai-toolbox/',
    stars: 12234,
    installationType: 'pip',
    pipPackage: 'toolbox-core',
    official: true,
    installConfig: {
      command: 'uvx',
      args: ['toolbox-langchain'],
    },
    tags: [
      'google',
      'database',
      'postgres',
      'mysql',
      'mongodb',
      'redis',
      'bigquery',
      'spanner',
      'official',
    ],
  },
  {
    id: 'fastapi-mcp',
    name: 'FastAPI MCP',
    description:
      'Expose FastAPI endpoints as MCP tools with authentication and authorization support',
    category: 'developer-tools',
    repository: 'https://github.com/tadata-org/fastapi_mcp',
    stars: 11346,
    installationType: 'pip',
    pipPackage: 'fastapi-mcp',
    installConfig: {
      command: 'pip',
      args: ['install', 'fastapi-mcp'],
    },
    tags: ['fastapi', 'api', 'rest', 'python', 'authentication', 'framework'],
  },
  {
    id: 'claude-flow-mcp',
    name: 'Claude Flow MCP',
    description:
      'Agent orchestration platform for Claude - deploy multi-agent swarms and coordinate autonomous workflows',
    category: 'coding-agents',
    repository: 'https://github.com/ruvnet/claude-flow',
    stars: 11184,
    installationType: 'npx',
    npmPackage: 'claude-flow',
    installConfig: {
      command: 'npx',
      args: ['-y', 'claude-flow@alpha', 'init'],
    },
    tags: [
      'claude',
      'multi-agent',
      'swarm',
      'orchestration',
      'workflow',
      'autonomous',
    ],
  },
  {
    id: 'pal-mcp-server',
    name: 'PAL MCP Server',
    description:
      'Multi-model support for Claude Code - use Gemini, OpenAI, OpenRouter, Azure, Grok, Ollama together',
    category: 'ai-services',
    repository: 'https://github.com/BeehiveInnovations/pal-mcp-server',
    stars: 10758,
    installationType: 'source',
    installConfig: {
      command: 'python',
      args: ['-m', 'pal_mcp_server'],
    },
    requiredEnvVars: [
      {
        name: 'OPENROUTER_API_KEY',
        description: 'OpenRouter API key (optional)',
        example: 'sk-or-xxxxx',
      },
    ],
    tags: [
      'multi-model',
      'gemini',
      'openai',
      'ollama',
      'azure',
      'grok',
      'openrouter',
    ],
  },
  {
    id: 'mcp-playwright-ea',
    name: 'Playwright MCP (ExecuteAutomation)',
    description:
      'Alternative Playwright MCP server for browser and API automation in Claude, Cursor, and more',
    category: 'browser-automation',
    repository: 'https://github.com/executeautomation/mcp-playwright',
    stars: 5116,
    installationType: 'npx',
    npmPackage: '@executeautomation/playwright-mcp-server',
    installConfig: {
      command: 'npx',
      args: ['-y', '@executeautomation/playwright-mcp-server'],
    },
    tags: ['playwright', 'browser', 'automation', 'testing', 'api'],
  },
  {
    id: 'claude-context-mcp',
    name: 'Claude Context MCP',
    description:
      'Code search MCP for Claude Code using vector database - make entire codebase the context',
    category: 'developer-tools',
    repository: 'https://github.com/zilliztech/claude-context',
    stars: 4955,
    installationType: 'npx',
    npmPackage: '@anthropic/claude-context',
    installConfig: {
      command: 'npx',
      args: ['-y', '@anthropic/claude-context'],
    },
    tags: [
      'code-search',
      'vector-db',
      'context',
      'semantic-search',
      'zilliz',
      'milvus',
    ],
  },
  {
    id: 'ida-pro-mcp',
    name: 'IDA Pro MCP',
    description:
      'AI-powered reverse engineering assistant bridging IDA Pro with language models',
    category: 'security',
    repository: 'https://github.com/mrexodia/ida-pro-mcp',
    stars: 4906,
    installationType: 'pip',
    pipPackage: 'ida-pro-mcp',
    installConfig: {
      command: 'pip',
      args: ['install', 'ida-pro-mcp'],
    },
    tags: [
      'ida-pro',
      'reverse-engineering',
      'binary-analysis',
      'security',
      'malware',
    ],
  },
  {
    id: 'deep-research-mcp',
    name: 'Deep Research MCP',
    description:
      'Use any LLM for deep research with SSE API and MCP server support',
    category: 'search-web',
    repository: 'https://github.com/u14app/deep-research',
    stars: 4302,
    installationType: 'pip',
    pipPackage: 'deep-research',
    installConfig: {
      command: 'uvx',
      args: ['deep-research'],
    },
    requiredEnvVars: [
      {
        name: 'OPENAI_API_KEY',
        description: 'OpenAI API key for LLM',
      },
    ],
    tags: ['research', 'deep-research', 'llm', 'sse', 'web-search'],
  },
  {
    id: 'cipher-mcp',
    name: 'Cipher MCP',
    description:
      'Memory layer for coding agents - persistent context for Cursor, Claude Code, Windsurf, and Cline',
    category: 'developer-tools',
    repository: 'https://github.com/campfirein/cipher',
    stars: 3399,
    installationType: 'npx',
    npmPackage: '@byterover/cipher-mcp',
    installConfig: {
      command: 'npx',
      args: ['-y', '@byterover/cipher-mcp'],
    },
    tags: ['memory', 'context', 'persistence', 'coding-agents', 'byterover'],
  },
  {
    id: 'metorial-mcp',
    name: 'Metorial MCP',
    description:
      'Connect any AI model to 600+ integrations powered by MCP with containerized security',
    category: 'aggregator',
    repository: 'https://github.com/metorial/metorial',
    stars: 3201,
    installationType: 'docker',
    dockerImage: 'metorial/metorial:latest',
    installConfig: {
      command: 'docker',
      args: ['run', '-i', '--rm', 'metorial/metorial:latest'],
    },
    tags: ['integrations', 'aggregator', 'docker', 'multi-tool', 'security'],
  },
  {
    id: 'osaurus-mcp',
    name: 'Osaurus MCP',
    description:
      'macOS LLM server - run local or cloud models with OpenAI/Anthropic compatible APIs',
    category: 'ai-services',
    repository: 'https://github.com/dinoki-ai/osaurus',
    stars: 2962,
    installationType: 'source',
    installConfig: {
      command: 'osaurus',
      args: ['serve'],
    },
    tags: [
      'macos',
      'local-llm',
      'mlx',
      'apple-silicon',
      'openai-compatible',
      'swift',
    ],
  },
  {
    id: 'shadcn-ui-mcp',
    name: 'shadcn/ui MCP',
    description:
      'Context about shadcn/ui components for LLMs - React, Vue, Svelte, and React Native support',
    category: 'developer-tools',
    repository: 'https://github.com/Jpisnice/shadcn-ui-mcp-server',
    stars: 2582,
    installationType: 'npx',
    npmPackage: 'shadcn-ui-mcp',
    installConfig: {
      command: 'npx',
      args: ['-y', 'shadcn-ui-mcp'],
    },
    tags: [
      'shadcn',
      'ui',
      'react',
      'vue',
      'svelte',
      'react-native',
      'tailwind',
    ],
  },
  {
    id: 'shippie-mcp',
    name: 'Shippie MCP',
    description:
      'Extendable code review and QA agent for CI/CD pipelines and GitHub',
    category: 'developer-tools',
    repository: 'https://github.com/mattzcarey/shippie',
    stars: 2317,
    installationType: 'npx',
    npmPackage: 'shippie',
    installConfig: {
      command: 'npx',
      args: ['-y', 'shippie'],
    },
    tags: ['code-review', 'qa', 'ci-cd', 'github', 'agent'],
  },
  {
    id: 'devdocs-mcp',
    name: 'DevDocs MCP',
    description:
      'Free, private, UI-based tech documentation MCP server for coders and developers',
    category: 'developer-tools',
    repository: 'https://github.com/cyberagiinc/DevDocs',
    stars: 2003,
    installationType: 'source',
    installConfig: {
      command: 'python',
      args: ['-m', 'devdocs'],
    },
    tags: ['documentation', 'docs', 'reference', 'api-docs', 'crawler'],
  },
  {
    id: 'brightdata-mcp',
    name: 'Bright Data MCP',
    description:
      'Official Bright Data MCP for web scraping, data extraction, and anti-bot bypass',
    category: 'browser-automation',
    repository: 'https://github.com/brightdata/brightdata-mcp',
    website: 'https://brightdata.com',
    stars: 1845,
    installationType: 'npx',
    npmPackage: '@anthropic/brightdata-mcp',
    official: true,
    installConfig: {
      command: 'npx',
      args: ['-y', '@anthropic/brightdata-mcp'],
    },
    requiredEnvVars: [
      {
        name: 'BRIGHTDATA_API_KEY',
        description: 'Bright Data API key',
      },
    ],
    tags: [
      'scraping',
      'web-data',
      'anti-bot',
      'proxy',
      'data-extraction',
      'official',
    ],
  },
  {
    id: 'mcpjam-inspector',
    name: 'MCP Inspector',
    description:
      'Build ChatGPT Apps and MCP servers locally with inspection and debugging tools',
    category: 'developer-tools',
    repository: 'https://github.com/MCPJam/inspector',
    stars: 1545,
    installationType: 'npx',
    npmPackage: '@mcpjam/inspector',
    installConfig: {
      command: 'npx',
      args: ['-y', '@mcpjam/inspector'],
    },
    tags: ['inspector', 'debugging', 'chatgpt', 'development', 'testing'],
  },
  {
    id: 'mcptools',
    name: 'MCP Tools CLI',
    description:
      'Command-line interface for interacting with MCP servers via stdio and HTTP transport',
    category: 'developer-tools',
    repository: 'https://github.com/f/mcptools',
    stars: 1426,
    installationType: 'npx',
    npmPackage: 'mcptools',
    installConfig: {
      command: 'npx',
      args: ['-y', 'mcptools'],
    },
    tags: ['cli', 'tools', 'stdio', 'http', 'debugging'],
  },
  {
    id: 'mcp-scan',
    name: 'MCP Scan',
    description:
      'Security vulnerability scanner for MCP connections - constrain, log, and audit',
    category: 'security',
    repository: 'https://github.com/invariantlabs-ai/mcp-scan',
    stars: 1381,
    installationType: 'pip',
    pipPackage: 'mcp-scan',
    installConfig: {
      command: 'uvx',
      args: ['mcp-scan'],
    },
    tags: ['security', 'vulnerability', 'audit', 'scanning', 'compliance'],
  },
  {
    id: 'web-eval-agent',
    name: 'Web Eval Agent MCP',
    description:
      'Autonomous web application evaluation and testing with Playwright integration',
    category: 'developer-tools',
    repository: 'https://github.com/refreshdotdev/web-eval-agent',
    stars: 1231,
    installationType: 'npx',
    npmPackage: '@refreshdotdev/web-eval-agent',
    installConfig: {
      command: 'npx',
      args: ['-y', '@refreshdotdev/web-eval-agent'],
    },
    tags: ['testing', 'qa', 'evaluation', 'playwright', 'autonomous'],
  },
  {
    id: 'mcp-unity-alt',
    name: 'MCP Unity (CoderGamester)',
    description:
      'Unity Editor MCP plugin for OpenAI, Gemini, Claude, Deepseek, and Grok integration',
    category: 'developer-tools',
    repository: 'https://github.com/CoderGamester/mcp-unity',
    stars: 1193,
    installationType: 'npx',
    npmPackage: 'mcp-unity',
    installConfig: {
      command: 'npx',
      args: ['-y', 'mcp-unity'],
    },
    tags: ['unity', 'game-dev', 'editor', 'c-sharp', 'multi-model'],
  },
  {
    id: 'xmcp-framework',
    name: 'xMCP Framework',
    description:
      'TypeScript MCP framework for building MCP servers with type safety and DX focus',
    category: 'developer-tools',
    repository: 'https://github.com/basementstudio/xmcp',
    stars: 1168,
    installationType: 'npm',
    npmPackage: 'xmcp',
    installConfig: {
      command: 'npm',
      args: ['install', 'xmcp'],
    },
    tags: ['framework', 'typescript', 'sdk', 'type-safe', 'developer-tools'],
  },
];

export function getMCPsByCategory(category: MCPCategory): MCPRegistryEntry[] {
  return MCP_REGISTRY.filter(mcp => mcp.category === category);
}

export function searchMCPs(query: string): MCPRegistryEntry[] {
  const lowerQuery = query.toLowerCase();
  return MCP_REGISTRY.filter(
    mcp =>
      mcp.name.toLowerCase().includes(lowerQuery) ||
      mcp.description.toLowerCase().includes(lowerQuery) ||
      mcp.tags?.some(tag => tag.toLowerCase().includes(lowerQuery))
  );
}

export function getAllCategories(): MCPCategory[] {
  return Array.from(new Set(MCP_REGISTRY.map(mcp => mcp.category)));
}

export function getAllTags(): string[] {
  const tagCounts = new Map<string, number>();
  MCP_REGISTRY.forEach(mcp => {
    mcp.tags?.forEach(tag => {
      tagCounts.set(tag, (tagCounts.get(tag) || 0) + 1);
    });
  });

  return Array.from(tagCounts.keys()).sort((a, b) => {
    const countDiff = (tagCounts.get(b) || 0) - (tagCounts.get(a) || 0);
    return countDiff !== 0 ? countDiff : a.localeCompare(b);
  });
}

export function getMCPsByTag(tag: string): MCPRegistryEntry[] {
  return MCP_REGISTRY.filter(mcp => mcp.tags?.includes(tag));
}

export function getTagCount(tag: string): number {
  return MCP_REGISTRY.filter(mcp => mcp.tags?.includes(tag)).length;
}
