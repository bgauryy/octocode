/**
 * Tests for ignoredPathFilter security
 */

import { describe, it, expect } from 'vitest';
import {
  shouldIgnorePath,
  shouldIgnoreFile,
  shouldIgnore,
} from '../src/ignoredPathFilter.js';

describe('ignoredPathFilter', () => {
  describe('shouldIgnorePath', () => {
    it('should return true for empty path', () => {
      expect(shouldIgnorePath('')).toBe(true);
    });

    it('should return true for whitespace-only path', () => {
      expect(shouldIgnorePath('   ')).toBe(true);
    });

    it('should return true for .ssh directory', () => {
      expect(shouldIgnorePath('.ssh')).toBe(true);
      expect(shouldIgnorePath('/project/.ssh')).toBe(true);
      expect(shouldIgnorePath('/project/.ssh/keys')).toBe(true);
    });

    it('should return true for .aws directory', () => {
      expect(shouldIgnorePath('.aws')).toBe(true);
      expect(shouldIgnorePath('/home/user/.aws')).toBe(true);
      expect(shouldIgnorePath('/home/user/.aws/credentials')).toBe(true);
    });

    it('should return true for secrets directory', () => {
      expect(shouldIgnorePath('secrets')).toBe(true);
      expect(shouldIgnorePath('/project/secrets')).toBe(true);
      expect(shouldIgnorePath('/project/secrets/api-key')).toBe(true);
    });

    it('should return false for normal paths', () => {
      expect(shouldIgnorePath('/project/src')).toBe(false);
      expect(shouldIgnorePath('/project/lib')).toBe(false);
      expect(shouldIgnorePath('src/utils')).toBe(false);
    });

    it('should handle backslash paths (Windows-style converted to forward slash)', () => {
      // The function normalizes backslashes to forward slashes
      expect(shouldIgnorePath('project/.ssh')).toBe(true);
    });
  });

  describe('shouldIgnoreFile', () => {
    it('should return true for empty filename', () => {
      expect(shouldIgnoreFile('')).toBe(true);
    });

    it('should return true for whitespace-only filename', () => {
      expect(shouldIgnoreFile('   ')).toBe(true);
    });

    it('should return true for .env files', () => {
      expect(shouldIgnoreFile('.env')).toBe(true);
      expect(shouldIgnoreFile('.env.local')).toBe(true);
      expect(shouldIgnoreFile('.env.production')).toBe(true);
    });

    it('should return true for key files', () => {
      expect(shouldIgnoreFile('private.key')).toBe(true);
      expect(shouldIgnoreFile('server.pem')).toBe(true);
      expect(shouldIgnoreFile('id_rsa')).toBe(true);
    });

    it('should return true for credentials', () => {
      expect(shouldIgnoreFile('credentials')).toBe(true);
      expect(shouldIgnoreFile('.credentials')).toBe(true);
    });

    it('should return false for normal files', () => {
      expect(shouldIgnoreFile('index.ts')).toBe(false);
      expect(shouldIgnoreFile('package.json')).toBe(false);
      expect(shouldIgnoreFile('README.md')).toBe(false);
    });

    it('should check full path for .ssh file patterns', () => {
      expect(shouldIgnoreFile('.ssh/id_rsa')).toBe(true);
    });

    // Backup files are NOW ALLOWED for code exploration
    it('should return false for backup files (allowed for diff analysis)', () => {
      expect(shouldIgnoreFile('config.bak')).toBe(false);
      expect(shouldIgnoreFile('settings.old')).toBe(false);
    });

    // Log files are NOW ALLOWED for code exploration
    it('should return false for log files (allowed for debugging)', () => {
      expect(shouldIgnoreFile('app.log')).toBe(false);
      expect(shouldIgnoreFile('error.log')).toBe(false);
    });

    // Database files are NOW ALLOWED for code exploration (content sanitized)
    it('should return false for database files (allowed, content sanitized)', () => {
      expect(shouldIgnoreFile('data.db')).toBe(false);
      expect(shouldIgnoreFile('users.sqlite')).toBe(false);
      expect(shouldIgnoreFile('dump.sql')).toBe(false);
    });
  });

  describe('shouldIgnore', () => {
    it('should return true if path should be ignored', () => {
      expect(shouldIgnore('.ssh')).toBe(true);
      expect(shouldIgnore('/project/.aws')).toBe(true);
    });

    it('should return true if file should be ignored', () => {
      expect(shouldIgnore('.env')).toBe(true);
      expect(shouldIgnore('server.key')).toBe(true);
    });

    // Log files are NOW ALLOWED
    it('should return false for log files (allowed for debugging)', () => {
      expect(shouldIgnore('/project/config/app.log')).toBe(false);
    });

    it('should return false for normal paths and files', () => {
      expect(shouldIgnore('/project/src/index.ts')).toBe(false);
    });
  });

  describe('shouldIgnorePath - full path pattern matching', () => {
    it('should match .config/gcloud path via full path pattern', () => {
      expect(shouldIgnorePath('.config/gcloud')).toBe(true);
    });

    it('should match nested .config/gcloud path via full path pattern', () => {
      expect(shouldIgnorePath('.config/gcloud/credentials')).toBe(true);
    });
  });

  describe('shouldIgnoreFile - full path pattern matching', () => {
    it('should match .docker/config.json via full path file pattern', () => {
      expect(shouldIgnoreFile('.docker/config.json')).toBe(true);
    });

    it('should match .aws/credentials via full path file pattern', () => {
      expect(shouldIgnoreFile('.aws/credentials')).toBe(true);
    });

    it('should match .kube/config via full path file pattern', () => {
      expect(shouldIgnoreFile('.kube/config')).toBe(true);
    });

    it('should match .pip/pip.conf via full path file pattern', () => {
      expect(shouldIgnoreFile('.pip/pip.conf')).toBe(true);
    });
  });

  describe('cryptocurrency wallet files', () => {
    it('should block wallet.dat', () => {
      expect(shouldIgnoreFile('wallet.dat')).toBe(true);
    });

    it('should block ethereum keystore paths', () => {
      expect(shouldIgnoreFile('.ethereum/keystore/UTC--key')).toBe(true);
    });

    it('should block electrum wallet paths', () => {
      expect(shouldIgnoreFile('.electrum/wallets/default_wallet')).toBe(true);
    });

    it('should block bitcoin wallet directory via path', () => {
      expect(shouldIgnorePath('.bitcoin')).toBe(true);
      expect(shouldIgnorePath('/home/user/.bitcoin')).toBe(true);
    });

    it('should block ethereum directory via path', () => {
      expect(shouldIgnorePath('.ethereum')).toBe(true);
    });

    it('should block electrum directory via path', () => {
      expect(shouldIgnorePath('.electrum')).toBe(true);
    });
  });

  describe('browser credential stores', () => {
    it('should block Login Data files', () => {
      expect(shouldIgnoreFile('Login Data')).toBe(true);
    });

    it('should block Cookies file', () => {
      expect(shouldIgnoreFile('Cookies')).toBe(true);
    });

    it('should block Firefox logins.json', () => {
      expect(shouldIgnoreFile('.mozilla/firefox/profile/logins.json')).toBe(
        true
      );
    });

    it('should block Firefox key database', () => {
      expect(shouldIgnoreFile('.mozilla/firefox/profile/key4.db')).toBe(true);
    });

    it('should block Chrome paths', () => {
      expect(shouldIgnorePath('.config/google-chrome/Default/something')).toBe(
        true
      );
    });

    it('should block Chromium paths', () => {
      expect(shouldIgnorePath('.config/chromium/Default/data')).toBe(true);
    });
  });

  describe('password manager databases', () => {
    it('should block KeePass .kdbx files', () => {
      expect(shouldIgnoreFile('passwords.kdbx')).toBe(true);
      expect(shouldIgnoreFile('keepass.kdbx')).toBe(true);
    });

    it('should block KeePass .kdb (legacy)', () => {
      expect(shouldIgnoreFile('secrets.kdb')).toBe(true);
    });

    it('should block 1Password sqlite', () => {
      expect(shouldIgnoreFile('1Password.sqlite')).toBe(true);
    });

    it('should block password-store directory via path', () => {
      expect(shouldIgnorePath('.password-store')).toBe(true);
      expect(shouldIgnorePath('/home/user/.password-store')).toBe(true);
    });
  });

  describe('VPN configuration files', () => {
    it('should block OpenVPN .ovpn files', () => {
      expect(shouldIgnoreFile('client.ovpn')).toBe(true);
    });

    it('should block WireGuard configs', () => {
      expect(shouldIgnoreFile('wireguard.conf')).toBe(true);
      expect(shouldIgnoreFile('wg0.conf')).toBe(true);
    });
  });

  describe('cloud provider directories', () => {
    it('should block .azure directory', () => {
      expect(shouldIgnorePath('.azure')).toBe(true);
      expect(shouldIgnorePath('/home/user/.azure')).toBe(true);
    });

    it('should block .kube directory', () => {
      expect(shouldIgnorePath('.kube')).toBe(true);
      expect(shouldIgnorePath('/home/user/.kube/config')).toBe(true);
    });

    it('should block .docker directory', () => {
      expect(shouldIgnorePath('.docker')).toBe(true);
    });

    it('should block .terraform directory', () => {
      expect(shouldIgnorePath('.terraform')).toBe(true);
      expect(shouldIgnorePath('/project/.terraform/plugins')).toBe(true);
    });

    it('should block cloud credential files', () => {
      expect(shouldIgnoreFile('service-account.json')).toBe(true);
      expect(shouldIgnoreFile('service_account_key.json')).toBe(true);
      expect(shouldIgnoreFile('application_default_credentials.json')).toBe(
        true
      );
    });
  });

  describe('core dumps and crash files', () => {
    it('should block core dump files', () => {
      expect(shouldIgnoreFile('core')).toBe(true);
      expect(shouldIgnoreFile('core.12345')).toBe(true);
    });

    it('should block Windows dump files', () => {
      expect(shouldIgnoreFile('crash.dmp')).toBe(true);
      expect(shouldIgnoreFile('mini.mdmp')).toBe(true);
    });
  });

  describe('macOS and Windows credential files', () => {
    it('should block macOS keychain files', () => {
      expect(shouldIgnoreFile('login.keychain')).toBe(true);
      expect(shouldIgnoreFile('login.keychain-db')).toBe(true);
    });

    it('should block macOS Keychain directory via path', () => {
      expect(shouldIgnorePath('Library/Keychains/login')).toBe(true);
    });

    it('should block Windows credential files', () => {
      expect(shouldIgnoreFile('NTUSER.DAT')).toBe(true);
      expect(shouldIgnoreFile('SAM')).toBe(true);
    });
  });

  describe('shell and database history', () => {
    it('should block shell history files', () => {
      expect(shouldIgnoreFile('.bash_history')).toBe(true);
      expect(shouldIgnoreFile('.zsh_history')).toBe(true);
    });

    it('should block database history files', () => {
      expect(shouldIgnoreFile('.mysql_history')).toBe(true);
      expect(shouldIgnoreFile('.psql_history')).toBe(true);
      expect(shouldIgnoreFile('.redis_history')).toBe(true);
      expect(shouldIgnoreFile('.mongo_history')).toBe(true);
    });
  });

  describe('terraform state and variables', () => {
    it('should block terraform state files', () => {
      expect(shouldIgnoreFile('terraform.tfstate')).toBe(true);
      expect(shouldIgnoreFile('terraform.tfstate.backup')).toBe(true);
    });

    it('should block terraform variable files', () => {
      expect(shouldIgnoreFile('terraform.tfvars')).toBe(true);
    });
  });

  describe('code signing and GPG keys', () => {
    it('should block GPG key files', () => {
      expect(shouldIgnoreFile('signing.gpg')).toBe(true);
      expect(shouldIgnoreFile('key.asc')).toBe(true);
    });

    it('should block certificate signing requests', () => {
      expect(shouldIgnoreFile('server.csr')).toBe(true);
    });
  });

  describe('RDP and session files', () => {
    it('should block RDP files', () => {
      expect(shouldIgnoreFile('server.rdp')).toBe(true);
    });

    it('should block session and cookie files', () => {
      expect(shouldIgnoreFile('cookies.txt')).toBe(true);
      expect(shouldIgnoreFile('.cookies')).toBe(true);
    });
  });
});
