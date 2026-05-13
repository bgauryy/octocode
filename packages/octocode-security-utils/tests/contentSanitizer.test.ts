import { describe, it, expect } from 'vitest';
import { ContentSanitizer } from '../src/contentSanitizer';

describe('ContentSanitizer', () => {
  describe('validateInputParameters', () => {
    describe('Array Parameter Handling', () => {
      it('should preserve arrays as arrays, not convert to strings', () => {
        const params = {
          owner: ['microsoft', 'facebook'],
          repo: ['react', 'vue'],
          keywordsToSearch: ['useState', 'useEffect'],
        };

        const result = ContentSanitizer.validateInputParameters(params);

        expect(result.isValid).toBe(true);
        expect(Array.isArray(result.sanitizedParams.owner)).toBe(true);
        expect(Array.isArray(result.sanitizedParams.repo)).toBe(true);
        expect(Array.isArray(result.sanitizedParams.keywordsToSearch)).toBe(
          true
        );
        expect(result.sanitizedParams.owner).toEqual(['microsoft', 'facebook']);
        expect(result.sanitizedParams.repo).toEqual(['react', 'vue']);
        expect(result.sanitizedParams.keywordsToSearch).toEqual([
          'useState',
          'useEffect',
        ]);
      });

      it('should not stringify arrays or add commas', () => {
        const params = {
          owner: ['microsoft', 'facebook', 'google'],
          language: ['typescript', 'javascript'],
        };

        const result = ContentSanitizer.validateInputParameters(params);

        expect(result.sanitizedParams.owner).toEqual([
          'microsoft',
          'facebook',
          'google',
        ]);
        expect(result.sanitizedParams.language).toEqual([
          'typescript',
          'javascript',
        ]);
      });

      it('should handle mixed string and array parameters correctly', () => {
        const params = {
          keywordsToSearch: ['function', 'useState'],
          owner: ['microsoft', 'facebook'],
          limit: 10,
          extension: 'ts',
        };

        const result = ContentSanitizer.validateInputParameters(params);

        expect(result.sanitizedParams).toEqual({
          keywordsToSearch: ['function', 'useState'],
          owner: ['microsoft', 'facebook'],
          limit: 10,
          extension: 'ts',
        });
      });

      it('should handle empty arrays correctly', () => {
        const params = {
          owner: [],
          keywordsToSearch: ['test'],
        };

        const result = ContentSanitizer.validateInputParameters(params);

        expect(result.sanitizedParams).toEqual({
          owner: [],
          keywordsToSearch: ['test'],
        });
      });

      it('should handle single-element arrays correctly', () => {
        const params = {
          owner: ['microsoft'],
          keywordsToSearch: ['useState'],
        };

        const result = ContentSanitizer.validateInputParameters(params);

        expect(result.sanitizedParams).toEqual({
          owner: ['microsoft'],
          keywordsToSearch: ['useState'],
        });
      });
    });

    describe('CLI Command Compatibility', () => {
      it('should preserve safe CLI characters in arrays', () => {
        const params = {
          owner: ['microsoft-corp', 'facebook.inc'],
          keywordsToSearch: ['use-state', 'use_effect', 'use.memo'],
          size: '>1000',
          filename: 'test-file.js',
        };

        const result = ContentSanitizer.validateInputParameters(params);

        expect(result.isValid).toBe(true);
        // Safe characters should be preserved
        expect(result.sanitizedParams.owner).toEqual([
          'microsoft-corp',
          'facebook.inc',
        ]);
        expect(result.sanitizedParams.keywordsToSearch).toEqual([
          'use-state',
          'use_effect',
          'use.memo',
        ]);
        expect(result.sanitizedParams.size).toBe('>1000');
        expect(result.sanitizedParams.filename).toBe('test-file.js');
      });

      it('should not break GitHub CLI owner flag format', () => {
        const params = {
          owner: ['microsoft', 'facebook', 'google'],
        };

        const result = ContentSanitizer.validateInputParameters(params);

        // Verify format that buildGitHubCliArgs expects
        expect(Array.isArray(result.sanitizedParams.owner)).toBe(true);
        expect(result.sanitizedParams.owner).toEqual([
          'microsoft',
          'facebook',
          'google',
        ]);

        // Should be ready for: owners.forEach(owner => args.push(`--owner=${owner}`))
        const mockCliArgs: string[] = [];
        (result.sanitizedParams.owner as string[]).forEach((owner: string) => {
          mockCliArgs.push(`--owner=${owner}`);
        });

        expect(mockCliArgs).toEqual([
          '--owner=microsoft',
          '--owner=facebook',
          '--owner=google',
        ]);
      });

      it('should not break GitHub CLI repo flag format', () => {
        const params = {
          owner: 'microsoft',
          repo: ['react', 'vue', 'angular'],
        };

        const result = ContentSanitizer.validateInputParameters(params);

        // Verify format for combined owner/repo
        expect(Array.isArray(result.sanitizedParams.repo)).toBe(true);
        expect(result.sanitizedParams.repo).toEqual([
          'react',
          'vue',
          'angular',
        ]);

        // Should be ready for: repos.forEach(repo => args.push(`--repo=${owner}/${repo}`))
        const mockCliArgs: string[] = [];
        (result.sanitizedParams.repo as string[]).forEach((repo: string) => {
          mockCliArgs.push(`--repo=${result.sanitizedParams.owner}/${repo}`);
        });

        expect(mockCliArgs).toEqual([
          '--repo=microsoft/react',
          '--repo=microsoft/vue',
          '--repo=microsoft/angular',
        ]);
      });
    });

    describe('Non-Array Parameter Handling (Regression Tests)', () => {
      it('should still handle string parameters correctly', () => {
        const params = {
          keywordsToSearch: ['function', 'useState'],
          language: 'typescript',
          extension: 'ts',
          filename: 'hooks.ts',
        };

        const result = ContentSanitizer.validateInputParameters(params);

        expect(result.sanitizedParams).toEqual({
          keywordsToSearch: ['function', 'useState'],
          language: 'typescript',
          extension: 'ts',
          filename: 'hooks.ts',
        });
      });

      it('should still handle non-string parameters correctly', () => {
        const params = {
          limit: 10,
          cache: true,
          timeout: 5000,
        };

        const result = ContentSanitizer.validateInputParameters(params);

        expect(result.sanitizedParams).toEqual({
          limit: 10,
          cache: true,
          timeout: 5000,
        });
      });

      it('should handle null and undefined values', () => {
        const params = {
          owner: null,
          repo: undefined,
          keywordsToSearch: ['useState'],
        };

        const result = ContentSanitizer.validateInputParameters(params);

        expect(result.sanitizedParams).toEqual({
          owner: null,
          repo: undefined,
          keywordsToSearch: ['useState'],
        });
      });
    });

    describe('Edge Cases', () => {
      it('should handle nested arrays (flatten or preserve structure)', () => {
        const params = {
          owner: [['microsoft'], ['facebook']],
        };

        const result = ContentSanitizer.validateInputParameters(params);

        expect(result.isValid).toBe(true);
        // Should preserve nested structure as-is (non-string elements pass through)
        expect(result.sanitizedParams.owner).toEqual([
          ['microsoft'],
          ['facebook'],
        ]);
      });

      it('should handle arrays with mixed data types', () => {
        const params = {
          owner: ['microsoft', 123, true, null, 'facebook'],
        };

        const result = ContentSanitizer.validateInputParameters(params);

        expect(result.isValid).toBe(true);
        // Only strings should be sanitized, others pass through
        expect(result.sanitizedParams.owner).toEqual([
          'microsoft',
          123,
          true,
          null,
          'facebook',
        ]);
      });

      it('should handle very large arrays', () => {
        const largeArray = Array.from({ length: 100 }, (_, i) => `org${i}`);
        const params = {
          owner: largeArray,
        };

        const result = ContentSanitizer.validateInputParameters(params);

        expect(result.sanitizedParams.owner).toEqual(largeArray);
      });

      it('should handle arrays with extremely long strings', () => {
        const longString = 'a'.repeat(2000);
        const params = {
          owner: ['microsoft', longString, 'facebook'],
        };

        const result = ContentSanitizer.validateInputParameters(params);

        expect(result.sanitizedParams.owner).toEqual([
          'microsoft',
          longString,
          'facebook',
        ]);
      });
    });
  });

  describe('Integration with CLI Command Building', () => {
    it('should produce output that works with GitHub CLI argument building', () => {
      const params = {
        keywordsToSearch: ['class', 'extends', 'React.Component'],
        owner: ['microsoft', 'facebook'],
        repo: ['react', 'vue'],
        language: 'javascript',
        limit: 5,
      };

      const result = ContentSanitizer.validateInputParameters(params);
      expect(result.isValid).toBe(true);

      // Simulate what buildGitHubCliArgs does
      const args: string[] = ['code'];

      // Add exact query (join terms as typically done in CLI)
      if (result.sanitizedParams.keywordsToSearch) {
        args.push(
          (result.sanitizedParams.keywordsToSearch as string[]).join(' ')
        );
      }

      // Add language
      args.push(`--language=${result.sanitizedParams.language}`);

      // Add repos with owners
      (result.sanitizedParams.repo as string[]).forEach((repo: string) => {
        (result.sanitizedParams.owner as string[]).forEach((owner: string) => {
          args.push(`--repo=${owner}/${repo}`);
        });
      });

      // Add limit
      args.push(`--limit=${result.sanitizedParams.limit}`);

      // Add JSON format
      args.push('--json=repository,path,textMatches,sha,url');

      const repoArgs = args.filter(arg => arg.startsWith('--repo='));
      expect(repoArgs.sort()).toEqual([
        '--repo=facebook/react',
        '--repo=facebook/vue',
        '--repo=microsoft/react',
        '--repo=microsoft/vue',
      ]);
    });
  });

  describe('sanitizeContent', () => {
    describe('GitHub Token Sanitization', () => {
      it('should sanitize GitHub personal access tokens', () => {
        const content =
          'Using token ghp_1234567890abcdefghijklmnopqrstuvwxyz123456 in CI';
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: 'Using token [REDACTED-GITHUBTOKENS] in CI',

          hasSecrets: true,

          secretsDetected: ['githubTokens'],
          warnings: ['1 secret(s) redacted'],
        });
      });

      it('should sanitize GitHub OAuth access tokens', () => {
        const content =
          'OAuth token: gho_1234567890abcdefghijklmnopqrstuvwxyz123456';
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: 'OAuth token: [REDACTED-GITHUBTOKENS]',

          hasSecrets: true,

          secretsDetected: ['githubTokens'],
          warnings: ['1 secret(s) redacted'],
        });
      });

      it('should sanitize GitHub app installation tokens', () => {
        const content =
          'Installation token: ghs_1234567890abcdefghijklmnopqrstuvwxyz123456';
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: 'Installation token: [REDACTED-GITHUBTOKENS]',

          hasSecrets: true,

          secretsDetected: ['githubTokens'],
          warnings: ['1 secret(s) redacted'],
        });
      });

      it('should sanitize GitHub refresh tokens', () => {
        const content =
          'Refresh token: ghr_1234567890abcdefghijklmnopqrstuvwxyz123456';
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: 'Refresh token: [REDACTED-GITHUBTOKENS]',

          hasSecrets: true,

          secretsDetected: ['githubTokens'],
          warnings: ['1 secret(s) redacted'],
        });
      });

      it('should sanitize multiple GitHub tokens in single content', () => {
        const content = `
          const tokens = {
            personal: "ghp_1234567890abcdefghijklmnopqrstuvwxyz123456",
            oauth: "gho_1234567890abcdefghijklmnopqrstuvwxyz123456"
          };
        `;
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: `
          const tokens = {
            personal: "[REDACTED-GITHUBTOKENS]",
            oauth: "[REDACTED-GITHUBTOKENS]"
          };
        `,

          hasSecrets: true,

          secretsDetected: ['githubTokens'],
          warnings: ['1 secret(s) redacted'],
        });
      });
    });

    describe('AI Provider API Key Sanitization', () => {
      it('should sanitize OpenAI API keys', () => {
        const content =
          'OpenAI key: sk-1234567890abcdefghijklmnopqrstuvwxyzT3BlbkFJABCDEFGHIJKLMNO';
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: 'OpenAI key: [REDACTED-OPENAIAPIKEYLEGACY]',

          hasSecrets: true,

          secretsDetected: ['openaiApiKeyLegacy'],
          warnings: ['1 secret(s) redacted'],
        });
      });

      it('should sanitize Groq API keys', () => {
        const content =
          'Groq key: gsk_1234567890abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOP';
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: 'Groq key: [REDACTED-GROQAPIKEY]',

          hasSecrets: true,

          secretsDetected: ['groqApiKey'],
          warnings: ['1 secret(s) redacted'],
        });
      });

      it('should sanitize OpenAI organization IDs', () => {
        const content = 'Organization: org-1234567890abcdefghij';
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: 'Organization: [REDACTED-OPENAIORGID]',

          hasSecrets: true,

          secretsDetected: ['openaiOrgId'],
          warnings: ['1 secret(s) redacted'],
        });
      });
    });

    describe('AWS Credentials Sanitization', () => {
      it('should sanitize AWS access key IDs', () => {
        const content = 'AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE';
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: 'AWS_ACCESS_KEY_ID=[REDACTED-AWSACCESSKEYID]',

          hasSecrets: true,

          secretsDetected: ['awsAccessKeyId'],
          warnings: ['1 secret(s) redacted'],
        });
      });

      it('should sanitize AWS secret access keys', () => {
        const content =
          'AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY';
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: '[REDACTED-AWSSECRETACCESSKEY]',

          hasSecrets: true,

          secretsDetected: ['awsSecretAccessKey'],
          warnings: ['1 secret(s) redacted'],
        });
      });
    });

    describe('Database Connection String Sanitization', () => {
      it('should sanitize PostgreSQL connection strings', () => {
        const content = 'postgresql://user:password@localhost:5432/mydb';
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: '[REDACTED-POSTGRESQLCONNECTIONSTRING]',

          hasSecrets: true,

          secretsDetected: ['postgresqlConnectionString'],
          warnings: ['1 secret(s) redacted'],
        });
      });

      it('should sanitize MongoDB connection strings', () => {
        const content =
          'mongodb://admin:secret@cluster0.mongodb.net:27017/myapp';
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: '[REDACTED-MONGODBCONNECTIONSTRING]',

          hasSecrets: true,

          secretsDetected: ['mongodbConnectionString'],
          warnings: ['1 secret(s) redacted'],
        });
      });
    });

    describe('Private Key Sanitization', () => {
      it('should sanitize RSA private keys', () => {
        const content = `
          -----BEGIN RSA PRIVATE KEY-----
          MIIEpAIBAAKCAQEA7YQnm/eSVyv24Bn5p7vSpJLPWdNw5MzQs1sVJQ==
          -----END RSA PRIVATE KEY-----
        `;
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: `
          [REDACTED-RSAPRIVATEKEY]
        `,

          hasSecrets: true,

          secretsDetected: ['rsaPrivateKey'],
          warnings: ['1 secret(s) redacted'],
        });
      });

      it('should sanitize OpenSSH private keys', () => {
        const content = `
          -----BEGIN OPENSSH PRIVATE KEY-----
          b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAAB
          -----END OPENSSH PRIVATE KEY-----
        `;
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: `
          [REDACTED-OPENSSHPRIVATEKEY]
        `,

          hasSecrets: true,

          secretsDetected: ['opensshPrivateKey'],
          warnings: ['1 secret(s) redacted'],
        });
      });
    });

    describe('Mixed Content Sanitization', () => {
      it('should sanitize multiple different secret types in single content', () => {
        const content = `
          # Configuration file
          GITHUB_TOKEN=ghp_1234567890abcdefghijklmnopqrstuvwxyz123456
          OPENAI_API_KEY=sk-1234567890abcdefghijklmnopqrstuvwxyzT3BlbkFJABCDEFGHIJKLMNO
          AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE
          DATABASE_URL=postgresql://user:pass@localhost:5432/db
        `;
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: `
          # Configuration file
          GITHUB_TOKEN=[REDACTED-GITHUBTOKENS]
          OPENAI_API_KEY=[REDACTED-OPENAIAPIKEYLEGACY]
          AWS_ACCESS_KEY_ID=[REDACTED-AWSACCESSKEYID]
          DATABASE_URL=[REDACTED-POSTGRESQLCONNECTIONSTRING]
        `,

          hasSecrets: true,

          secretsDetected: [
            'openaiApiKeyLegacy',
            'awsAccessKeyId',
            'postgresqlConnectionString',
            'githubTokens',
          ],
          warnings: ['4 secret(s) redacted'],
        });
      });
    });

    describe('Clean Content Handling', () => {
      it('should handle content with no secrets', () => {
        const content = `
          const config = {
            apiUrl: "https://api.example.com",
            version: "1.0.0",
            timeout: 5000
          };
        `;
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: `
          const config = {
            apiUrl: "https://api.example.com",
            version: "1.0.0",
            timeout: 5000
          };
        `,

          hasSecrets: false,

          secretsDetected: [],
          warnings: [],
        });
      });

      it('should preserve regular URLs and non-secret data', () => {
        const content =
          'Visit https://github.com/user/repo and check the README.md file';
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content:
            'Visit https://github.com/user/repo and check the README.md file',

          hasSecrets: false,

          secretsDetected: [],
          warnings: [],
        });
      });
    });

    describe('Edge Cases', () => {
      it('should handle empty content', () => {
        const result = ContentSanitizer.sanitizeContent('');

        expect(result).toEqual({
          content: '',

          hasSecrets: false,

          secretsDetected: [],
          warnings: [],
        });
      });

      it('should handle content with only whitespace', () => {
        const content = '   \n\t  \n  ';
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: '   \n\t  \n  ',

          hasSecrets: false,

          secretsDetected: [],
          warnings: [],
        });
      });

      it('should handle content with partial token patterns', () => {
        const content = 'This looks like ghp_ but is not a complete token';
        const result = ContentSanitizer.sanitizeContent(content);

        expect(result).toEqual({
          content: 'This looks like ghp_ but is not a complete token',

          hasSecrets: false,

          secretsDetected: [],
          warnings: [],
        });
      });
    });
  });

  describe('Array Length Validation', () => {
    it('should truncate arrays exceeding maximum length (100 items)', () => {
      const largeArray = Array.from({ length: 150 }, (_, i) => `item${i}`);
      const params = {
        keywords: largeArray,
      };

      const result = ContentSanitizer.validateInputParameters(params);

      expect(result.isValid).toBe(true);
      expect(Array.isArray(result.sanitizedParams.keywords)).toBe(true);
      expect((result.sanitizedParams.keywords as string[]).length).toBe(100);
      expect(result.warnings).toContain(
        'Parameter keywords array exceeds maximum length (100 items)'
      );
      expect((result.sanitizedParams.keywords as string[])[0]).toBe('item0');
      expect((result.sanitizedParams.keywords as string[])[99]).toBe('item99');
    });

    it('should not truncate arrays at or below maximum length', () => {
      const exactArray = Array.from({ length: 100 }, (_, i) => `item${i}`);
      const smallArray = Array.from({ length: 50 }, (_, i) => `item${i}`);

      const result1 = ContentSanitizer.validateInputParameters({
        keywords: exactArray,
      });
      const result2 = ContentSanitizer.validateInputParameters({
        keywords: smallArray,
      });

      expect(result1.isValid).toBe(true);
      expect((result1.sanitizedParams.keywords as string[]).length).toBe(100);
      expect(result1.warnings.length).toBe(0);

      expect(result2.isValid).toBe(true);
      expect((result2.sanitizedParams.keywords as string[]).length).toBe(50);
      expect(result2.warnings.length).toBe(0);
    });

    it('should handle multiple large arrays', () => {
      const largeArray1 = Array.from({ length: 120 }, (_, i) => `item${i}`);
      const largeArray2 = Array.from({ length: 110 }, (_, i) => `val${i}`);

      const result = ContentSanitizer.validateInputParameters({
        keywords1: largeArray1,
        keywords2: largeArray2,
      });

      expect(result.isValid).toBe(true);
      expect((result.sanitizedParams.keywords1 as string[]).length).toBe(100);
      expect((result.sanitizedParams.keywords2 as string[]).length).toBe(100);
      expect(result.warnings).toContain(
        'Parameter keywords1 array exceeds maximum length (100 items)'
      );
      expect(result.warnings).toContain(
        'Parameter keywords2 array exceeds maximum length (100 items)'
      );
    });
  });

  describe('Error Handling', () => {
    it('should handle regex errors gracefully in detectSecrets', () => {
      // Create content that would normally match but we'll mock an error
      const content = 'ghp_1234567890abcdefghijklmnopqrstuvwxyz123456';

      // We can't easily force a regex error in the current implementation,
      // but we can test the fallback behavior
      const result = ContentSanitizer.sanitizeContent(content);

      // Should still process the content
      expect(result).toBeDefined();
      expect(result.content).toBeDefined();
    });

    it('should handle null params in validateInputParameters', () => {
      const result = ContentSanitizer.validateInputParameters(
        null as unknown as Record<string, unknown>
      );

      expect(result.isValid).toBe(false);
      expect(result.warnings).toContain(
        'Invalid parameters: must be an object'
      );
      expect(result.sanitizedParams).toEqual({});
    });

    it('should handle undefined params in validateInputParameters', () => {
      const result = ContentSanitizer.validateInputParameters(
        undefined as unknown as Record<string, unknown>
      );

      expect(result.isValid).toBe(false);
      expect(result.warnings).toContain(
        'Invalid parameters: must be an object'
      );
      expect(result.sanitizedParams).toEqual({});
    });

    it('should handle non-object params in validateInputParameters', () => {
      const result = ContentSanitizer.validateInputParameters(
        'not an object' as unknown as Record<string, unknown>
      );

      expect(result.isValid).toBe(false);
      expect(result.warnings).toContain(
        'Invalid parameters: must be an object'
      );
    });

    it('should reject invalid parameter keys (empty string)', () => {
      const params = {
        '': 'value',
        validKey: 'validValue',
      };

      const result = ContentSanitizer.validateInputParameters(params);

      expect(result.isValid).toBe(false);
      expect(result.warnings.length).toBeGreaterThan(0);
      expect(
        result.warnings.some(w => w.includes('Invalid parameter key'))
      ).toBe(true);
      expect(result.sanitizedParams['']).toBeUndefined();
      expect(result.sanitizedParams.validKey).toBe('validValue');
    });

    it('should reject invalid parameter keys (whitespace only)', () => {
      const params = {
        '   ': 'value',
        validKey: 'validValue',
      };

      const result = ContentSanitizer.validateInputParameters(params);

      expect(result.isValid).toBe(false);
      expect(
        result.warnings.some(w => w.includes('Invalid parameter key'))
      ).toBe(true);
    });

    it('should truncate excessively long strings', () => {
      const longString = 'a'.repeat(15000);
      const params = {
        longValue: longString,
        normalValue: 'normal',
      };

      const result = ContentSanitizer.validateInputParameters(params);

      expect(result.isValid).toBe(true);
      expect((result.sanitizedParams.longValue as string).length).toBe(10000);
      expect(result.warnings).toContain(
        'Parameter longValue exceeds maximum length (10,000 characters)'
      );
      expect(result.sanitizedParams.normalValue).toBe('normal');
    });

    it('should handle string at exactly 10000 characters', () => {
      const exactString = 'a'.repeat(10000);
      const params = {
        value: exactString,
      };

      const result = ContentSanitizer.validateInputParameters(params);

      expect(result.isValid).toBe(true);
      expect((result.sanitizedParams.value as string).length).toBe(10000);
      expect(result.warnings.length).toBe(0);
    });

    it('should handle string at 10001 characters (just over limit)', () => {
      const overLimit = 'a'.repeat(10001);
      const params = {
        value: overLimit,
      };

      const result = ContentSanitizer.validateInputParameters(params);

      expect(result.isValid).toBe(true);
      expect((result.sanitizedParams.value as string).length).toBe(10000);
      expect(result.warnings).toContain(
        'Parameter value exceeds maximum length (10,000 characters)'
      );
    });
  });

  describe('Dangerous Parameter Keys', () => {
    it('should block __proto__ key', () => {
      // Create params with __proto__ as an actual property using Object.defineProperty
      const params: Record<string, unknown> = { normal: 'safe' };
      Object.defineProperty(params, '__proto__', {
        value: 'dangerous',
        enumerable: true,
        configurable: true,
        writable: true,
      });

      const result = ContentSanitizer.validateInputParameters(params);

      expect(result.isValid).toBe(false);
      expect(result.warnings.length).toBeGreaterThan(0);
      expect(
        result.warnings.some(w =>
          w.includes('Dangerous parameter key blocked: __proto__')
        )
      ).toBe(true);
      // __proto__ should not be in sanitized params
      expect(
        Object.prototype.hasOwnProperty.call(
          result.sanitizedParams,
          '__proto__'
        )
      ).toBe(false);
      expect(result.sanitizedParams.normal).toBe('safe');
    });

    it('should block constructor key', () => {
      const params = {
        constructor: 'dangerous',
        normal: 'safe',
      };

      const result = ContentSanitizer.validateInputParameters(params);

      expect(result.isValid).toBe(false);
      expect(
        result.warnings.some(w =>
          w.includes('Dangerous parameter key blocked: constructor')
        )
      ).toBe(true);
      expect(result.sanitizedParams.normal).toBe('safe');
    });

    it('should block prototype key', () => {
      const params = {
        prototype: 'dangerous',
        normal: 'safe',
      };

      const result = ContentSanitizer.validateInputParameters(params);

      expect(result.isValid).toBe(false);
      expect(
        result.warnings.some(w =>
          w.includes('Dangerous parameter key blocked: prototype')
        )
      ).toBe(true);
      expect(result.sanitizedParams.normal).toBe('safe');
    });

    it('should block all dangerous keys together', () => {
      const params = {
        __proto__: 'dangerous1',
        constructor: 'dangerous2',
        prototype: 'dangerous3',
        normal: 'safe',
      };

      const result = ContentSanitizer.validateInputParameters(params);

      expect(result.isValid).toBe(false);
      // Should have warnings for dangerous keys
      expect(result.warnings.length).toBeGreaterThan(0);
      expect(result.sanitizedParams.normal).toBe('safe');
      // Dangerous keys should not be in sanitized params own properties
      expect(
        Object.prototype.hasOwnProperty.call(
          result.sanitizedParams,
          '__proto__'
        )
      ).toBe(false);
      expect(
        Object.prototype.hasOwnProperty.call(
          result.sanitizedParams,
          'constructor'
        )
      ).toBe(false);
      expect(
        Object.prototype.hasOwnProperty.call(
          result.sanitizedParams,
          'prototype'
        )
      ).toBe(false);
    });
  });

  describe('Nested Object Validation', () => {
    it('should validate nested objects recursively', () => {
      const params = {
        search: {
          owner: 'microsoft',
          repo: 'vscode',
          keywords: ['typescript', 'javascript'],
        },
        filters: {
          language: 'typescript',
          stars: 1000,
        },
      };

      const result = ContentSanitizer.validateInputParameters(params);

      expect(result.isValid).toBe(true);
      expect(result.sanitizedParams).toEqual(params);
      expect(result.warnings.length).toBe(0);
    });

    it('should block dangerous keys in nested objects', () => {
      const params = {
        search: {
          constructor: 'blocked',
          prototype: 'also blocked',
          owner: 'microsoft',
        },
      };

      const result = ContentSanitizer.validateInputParameters(params);

      // Should detect invalid nested object and block it
      expect(result.isValid).toBe(false);
      expect(result.warnings.length).toBeGreaterThan(0);
      expect(
        result.warnings.some(w => w.includes('Invalid nested object'))
      ).toBe(true);
    });

    it('should handle deeply nested objects', () => {
      const params = {
        level1: {
          level2: {
            level3: {
              value: 'deep',
            },
          },
        },
      };

      const result = ContentSanitizer.validateInputParameters(params);

      expect(result.isValid).toBe(true);
      expect(result.sanitizedParams).toEqual(params);
    });

    it('should propagate hasSecrets from nested objects', () => {
      // This test ensures the hasSecrets flag is propagated correctly
      // Note: Current implementation doesn't check for secrets in validation,
      // but the structure is in place
      const params = {
        search: {
          query: 'test',
          owner: 'microsoft',
        },
        config: {
          apiKey: 'test-key',
        },
      };

      const result = ContentSanitizer.validateInputParameters(params);

      expect(result.isValid).toBe(true);
      expect(result.hasSecrets).toBe(false); // No actual secret detection in validation
    });

    it('should handle nested objects with invalid parameters', () => {
      const params = {
        valid: 'value',
        nested: {
          constructor: 'blocked',
          valid: 'allowed',
        },
      };

      const result = ContentSanitizer.validateInputParameters(params);

      expect(result.isValid).toBe(false);
      expect(result.warnings.length).toBeGreaterThan(0);
      expect(
        result.warnings.some(
          w =>
            w.includes('Invalid nested object') ||
            w.includes('Dangerous parameter key')
        )
      ).toBe(true);
    });

    it('should handle nested arrays within objects', () => {
      const params = {
        search: {
          keywords: ['react', 'vue', 'angular'],
          filters: {
            languages: ['typescript', 'javascript'],
          },
        },
      };

      const result = ContentSanitizer.validateInputParameters(params);

      expect(result.isValid).toBe(true);
      expect(result.sanitizedParams).toEqual(params);
    });

    it('should truncate large arrays in nested objects', () => {
      const largeArray = Array.from({ length: 150 }, (_, i) => `item${i}`);
      const params = {
        search: {
          keywords: largeArray,
          owner: 'microsoft',
        },
      };

      const result = ContentSanitizer.validateInputParameters(params);

      expect(result.isValid).toBe(true);
      const searchParams = result.sanitizedParams.search as Record<
        string,
        unknown
      >;
      // Array should be truncated to 100 items
      expect((searchParams.keywords as string[]).length).toBe(100);
      // Verify the sanitized params contain the nested structure
      expect(searchParams.owner).toBe('microsoft');
    });
  });

  describe('nesting depth boundary', () => {
    function buildNested(levels: number): Record<string, unknown> {
      let obj: Record<string, unknown> = { value: 'bottom' };
      for (let i = 0; i < levels; i++) obj = { nested: obj };
      return obj;
    }

    it('should allow shallow nesting (depth 3)', () => {
      const result = ContentSanitizer.validateInputParameters(buildNested(3));
      expect(result.isValid).toBe(true);
    });

    it('should allow nesting at exactly depth 20 (boundary — last allowed)', () => {
      // buildNested(20) creates 20 recursive levels; the root call is _depth=0,
      // so the deepest call reaches _depth=20 (20 > 20 is false → allowed)
      const result = ContentSanitizer.validateInputParameters(buildNested(20));
      expect(result.isValid).toBe(true);
      expect(result.warnings).toHaveLength(0);
    });

    it('should block nesting at depth 21 (first rejected level)', () => {
      // At _depth=21 the check (_depth > 20) fires → isValid: false
      const result = ContentSanitizer.validateInputParameters(buildNested(21));
      expect(result.isValid).toBe(false);
      expect(
        result.warnings.some(w => w.includes('depth') || w.includes('nesting'))
      ).toBe(true);
    });

    it('should block deeply nested objects (depth 50)', () => {
      const result = ContentSanitizer.validateInputParameters(buildNested(50));
      expect(result.isValid).toBe(false);
    });
  });

  describe('numeric parameter passthrough', () => {
    it('should pass numeric values through without modification', () => {
      const result = ContentSanitizer.validateInputParameters({
        depth: 5,
        threads: 4,
        maxFiles: 100,
      });
      expect(result.isValid).toBe(true);
      expect(result.sanitizedParams.depth).toBe(5);
      expect(result.sanitizedParams.threads).toBe(4);
      expect(result.sanitizedParams.maxFiles).toBe(100);
    });

    it('should pass negative numbers through (no range enforcement)', () => {
      const result = ContentSanitizer.validateInputParameters({ depth: -1 });
      expect(result.isValid).toBe(true);
      expect(result.sanitizedParams.depth).toBe(-1);
    });

    it('should pass zero through', () => {
      const result = ContentSanitizer.validateInputParameters({ count: 0 });
      expect(result.isValid).toBe(true);
      expect(result.sanitizedParams.count).toBe(0);
    });

    it('should pass very large numbers through', () => {
      const result = ContentSanitizer.validateInputParameters({
        threads: 99999,
      });
      expect(result.isValid).toBe(true);
      expect(result.sanitizedParams.threads).toBe(99999);
    });

    it('should pass boolean values through', () => {
      const result = ContentSanitizer.validateInputParameters({
        hidden: true,
        recursive: false,
      });
      expect(result.isValid).toBe(true);
      expect(result.sanitizedParams.hidden).toBe(true);
      expect(result.sanitizedParams.recursive).toBe(false);
    });
  });
});
