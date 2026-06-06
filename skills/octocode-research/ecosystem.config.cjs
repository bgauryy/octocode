/**
 * PM2 Ecosystem Configuration for Octocode Research Server
 * 
 * @version 2.2.0
 * 
 * Restart Strategy:
 * - Idle-based restart: Server self-restarts after 30 minutes of inactivity (handled in server.ts)
 * - Memory guard: PM2 restarts if memory exceeds 500MB (safety net)
 * - NO cron restart: Removed in favor of idle-based restart
 * 
 * @see docs/SERVER_FLOWS.md for detailed flow documentation
 */


module.exports = {
  apps: [{
    name: 'octocode-research',
    script: './scripts/server.js',
    
    
    
    max_memory_restart: '500M',
    
    
    kill_timeout: 120000,
    
    wait_ready: true,
    
    listen_timeout: 15000,
    
    
    autorestart: true,
    
    max_restarts: 10,
    
    restart_delay: 1000,
    
    exp_backoff_restart_delay: 100,
    
    min_uptime: 5000,
    
    
    out_file: '/dev/null',
    error_file: '/dev/null',
    merge_logs: true,
    combine_logs: true,
    
    
    env: {
      NODE_ENV: 'production',
    },
    
    env_development: {
      NODE_ENV: 'development',
    },
  }]
};
