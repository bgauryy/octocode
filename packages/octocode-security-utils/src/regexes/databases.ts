import type { SensitiveDataPattern } from './types.js';

export const databasePatterns: SensitiveDataPattern[] = [
  // SQL Databases
  {
    name: 'postgresqlConnectionString',
    description: 'PostgreSQL connection string with credentials',
    regex: /\bpostgresql:\/\/[^:]+:[^@]+@[^/\s]+\/[^?\s]+\b/gi,
    matchAccuracy: 'high',
  },
  {
    name: 'mysqlConnectionString',
    description: 'MySQL connection string with credentials',
    regex: /\bmysql:\/\/[^:]+:[^@]+@[^/\s]+\/[^?\s]+\b/gi,
    matchAccuracy: 'high',
  },
  {
    name: 'jdbcConnectionStringWithCredentials',
    description: 'JDBC connection string with embedded credentials',
    regex: /\bjdbc:(?:postgresql|mysql):\/\/[^:]+:[^@]+@[^/\s]+\b/gi,
    matchAccuracy: 'medium',
    fileContext: /(?:\.env|config|settings|secrets)/i,
  },

  // NoSQL Databases
  {
    name: 'mongodbConnectionString',
    description:
      'MongoDB connection string with credentials (incl. mongodb+srv://)',
    regex:
      /\bmongodb(?:\+srv)?:\/\/[a-zA-Z0-9._%-]+:[a-zA-Z0-9._%-]+@[a-zA-Z0-9._-]+(?::[0-9]+)?(?:\/[a-zA-Z0-9._-]*)?\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'redisConnectionString',
    description:
      'Redis connection string with credentials (incl. rediss:// TLS)',
    regex:
      /\brediss?:\/\/[a-zA-Z0-9._%-]+:[a-zA-Z0-9._%-]+@[a-zA-Z0-9._-]+:[0-9]+\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'redisAuthPassword',
    description: 'Redis AUTH password command',
    regex: /\bAUTH\s+[a-zA-Z0-9_-]{8,}\b/gi,
    matchAccuracy: 'medium',
  },

  // Search & Analytics
  {
    name: 'elasticsearchCredentials',
    description: 'Elasticsearch credentials in URL',
    regex: /\bhttps?:\/\/[^:]+:[^@]+@[^/\s]+:9200\b/gi,
    matchAccuracy: 'high',
  },

  // Document Databases
  {
    name: 'couchdbCredentials',
    description: 'CouchDB credentials in URL',
    regex: /\bhttp[s]?:\/\/[^:]+:[^@]+@[^/\s]+:5984\b/gi,
    matchAccuracy: 'high',
  },

  // Graph Databases
  {
    name: 'neo4jCredentials',
    description: 'Neo4j database credentials in URL',
    regex: /\bbolt[s]?:\/\/[^:]+:[^@]+@[^/\s]+:7687\b/gi,
    matchAccuracy: 'high',
  },

  // Time Series Databases
  {
    name: 'timescaledbConnectionString',
    description: 'TimescaleDB connection string with credentials',
    regex: /\btimescaledb:\/\/[^:]+:[^@]+@[^/\s]+\/[^?\s]+\b/gi,
    matchAccuracy: 'high',
  },

  // Column-Oriented Databases
  {
    name: 'clickhouseCredentials',
    description: 'ClickHouse connection string with credentials',
    regex: /\bclickhouse:\/\/[^:]+:[^@]+@[^/\s]+:8123\b/gi,
    matchAccuracy: 'high',
  },
  {
    name: 'cassandraConnectionString',
    description: 'Cassandra connection string with credentials',
    regex: /\bcassandra:\/\/[^:]+:[^@]+@[^/\s]+:9042\b/gi,
    matchAccuracy: 'high',
  },

  // Cloud Database Services
  {
    name: 'faunadbKey',
    description: 'FaunaDB secret key',
    regex: /\bfn[a-zA-Z0-9]{40}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'databricksApiToken',
    description: 'Databricks API token',
    regex: /\bdapi[a-f0-9]{32}(?:-\d)?\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'pineconeApiKey',
    description: 'Pinecone API key',
    regex:
      /\bpinecone[\s\w]*(?:api|key|env)[\s:=]*["']?[a-zA-Z0-9_-]{32}["']?\b/gi,
    matchAccuracy: 'medium',
  },

  // Generic Database Patterns
  {
    name: 'databaseUrlWithCredentials',
    description: 'Generic database URL with embedded credentials',
    regex: /\b(?:postgres|mysql|mongodb|redis):\/\/[^:]+:[^@]+@[^/\s]+\b/gi,
    matchAccuracy: 'medium',
  },
  // ClickHouse Cloud API Secret Key
  {
    name: 'clickhouseCloudApiKey',
    description: 'ClickHouse Cloud API secret key',
    regex: /\b4b1d[A-Za-z0-9]{38}\b/g,
    matchAccuracy: 'high',
  },
  // Neon Database Connection String
  {
    name: 'neonDatabaseConnectionString',
    description: 'Neon database connection string',
    regex: /\bpostgres:\/\/[^:]+:[^@]+@[^/\s]*neon\.tech[^?\s]*\b/gi,
    matchAccuracy: 'high',
  },
  // Turso Database Token
  {
    name: 'tursoDatabaseToken',
    description: 'Turso database auth token',
    regex:
      /\b['"]?(?:turso|libsql)(?:[\s\w.-]{0,20})(?:token|auth)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9._-]{50,}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Upstash Redis Token
  {
    name: 'upstashRedisToken',
    description: 'Upstash Redis REST token',
    regex:
      /\b['"]?(?:upstash)(?:[\s\w.-]{0,20})(?:token|key)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9=]{40,}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Supabase JWT (anon/service_role keys are JWTs starting with eyJ)
  {
    name: 'supabaseJwtKey',
    description: 'Supabase anon or service_role key (JWT format)',
    regex:
      /\b['"]?(?:SUPABASE|supabase)_?(?:ANON|SERVICE_ROLE|anon|service_role)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?(eyJ[a-zA-Z0-9_-]{100,})['"]?\b/g,
    matchAccuracy: 'high',
  },
  // CockroachDB connection string
  {
    name: 'cockroachdbConnectionString',
    description: 'CockroachDB connection string with credentials',
    regex:
      /\bpostgresql:\/\/[^:]+:[^@]+@[^/\s]*cockroachlabs\.cloud[^?\s]*\b/gi,
    matchAccuracy: 'high',
  },
];
