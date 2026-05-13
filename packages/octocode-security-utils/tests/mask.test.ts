import { describe, it, expect, beforeEach } from 'vitest';
import { maskSensitiveData } from '../src/mask';

describe('maskSensitiveData', () => {
  beforeEach(() => {
    // Clear any cached regex before each test
    // Force fresh regex creation by accessing private cache
    (maskSensitiveData as unknown as { combinedRegex: null }).combinedRegex =
      null;
  });

  describe('Basic Functionality', () => {
    it('should return empty string for empty input', () => {
      expect(maskSensitiveData('')).toBe('');
    });

    it('should return input unchanged when no sensitive data detected', () => {
      const text = 'This is just normal text without any secrets.';
      expect(maskSensitiveData(text)).toBe(text);
    });

    it('should handle null and undefined inputs gracefully', () => {
      expect(maskSensitiveData(null as unknown as string)).toBe(null);
      expect(maskSensitiveData(undefined as unknown as string)).toBe(undefined);
    });

    it('should mask sensitive data with alternating pattern', () => {
      const text = 'API key: sk-1234567890abcdefT3BlbkFJ1234567890abcdef';
      const result = maskSensitiveData(text);

      expect(result).toEqual(
        'API key: *k*1*3*5*7*9*a*c*e*T*B*b*F*1*3*5*7*9*a*c*e*'
      );
    });
  });

  describe('Pattern Detection', () => {
    it('should detect and mask ghp GitHub tokens', () => {
      const text = 'GitHub token: ghp_1234567890123456789012345678901234567890';
      const result = maskSensitiveData(text);
      expect(result).toEqual(
        'GitHub token: *h*_*2*4*6*8*0*2*4*6*8*0*2*4*6*8*0*2*4*6*8*0'
      );
    });

    it('should detect and mask gho GitHub tokens', () => {
      const text = 'GitHub token: gho_1234567890123456789012345678901234567890';
      const result = maskSensitiveData(text);
      expect(result).toEqual(
        'GitHub token: *h*_*2*4*6*8*0*2*4*6*8*0*2*4*6*8*0*2*4*6*8*0'
      );
    });

    it('should detect and mask ghu GitHub tokens', () => {
      const text = 'GitHub token: ghu_1234567890123456789012345678901234567890';
      const result = maskSensitiveData(text);
      expect(result).toEqual(
        'GitHub token: *h*_*2*4*6*8*0*2*4*6*8*0*2*4*6*8*0*2*4*6*8*0'
      );
    });

    it('should detect and mask ghs GitHub tokens', () => {
      const text = 'GitHub token: ghs_1234567890123456789012345678901234567890';
      const result = maskSensitiveData(text);
      expect(result).toEqual(
        'GitHub token: *h*_*2*4*6*8*0*2*4*6*8*0*2*4*6*8*0*2*4*6*8*0'
      );
    });

    it('should detect and mask ghr GitHub tokens', () => {
      const text = 'GitHub token: ghr_1234567890123456789012345678901234567890';
      const result = maskSensitiveData(text);
      expect(result).toEqual(
        'GitHub token: *h*_*2*4*6*8*0*2*4*6*8*0*2*4*6*8*0*2*4*6*8*0'
      );
    });

    it('should detect and mask OpenAI API keys', () => {
      const text =
        'OpenAI API Key: sk-1234567890abcdefT3BlbkFJ1234567890abcdef';
      const result = maskSensitiveData(text);

      expect(result).toEqual(
        'OpenAI API Key: *k*1*3*5*7*9*a*c*e*T*B*b*F*1*3*5*7*9*a*c*e*'
      );
    });

    it('should detect and mask AWS access keys', () => {
      const text = `AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE
AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY`;
      const result = maskSensitiveData(text);

      expect(result).toEqual(`AWS_ACCESS_KEY_ID=*K*A*O*F*D*N*E*A*P*E
*W*_*E*R*T*A*C*S*_*E*=*J*l*X*t*F*M*/*7*D*N*/*P*R*i*Y*X*M*L*K*Y`);
    });

    it('should detect and mask jwt_secret environment variable', () => {
      const text = 'jwt_secret="super_secret_jwt_token_123456789"';
      const result = maskSensitiveData(text);
      expect(result).toEqual('*w*_*e*r*t*"*u*e*_*e*r*t*j*t*t*k*n*1*3*5*7*9*');
    });

    it('should detect and mask SECRET_token environment variable', () => {
      const text = 'SECRET_token="very_long_secret_value_abcdef123456789"';
      const result = maskSensitiveData(text);
      expect(result).toEqual(
        '*E*R*T*t*k*n*"*e*y*l*n*_*e*r*t*v*l*e*a*c*e*1*3*5*7*9*'
      );
    });

    it('should detect and mask password environment variable', () => {
      const text = 'password="complex_password_with_enough_length_12345"';
      const result = maskSensitiveData(text);
      expect(result).toEqual(
        '*a*s*o*d*"*o*p*e*_*a*s*o*d*w*t*_*n*u*h*l*n*t*_*2*4*"'
      );
    });

    it('should detect and mask key environment variable', () => {
      const text =
        'key="base64_encoded_secret_value_abcdef1234567890abcdef123456"';
      const result = maskSensitiveData(text);
      expect(result).toEqual(
        '*e*=*b*s*6*_*n*o*e*_*e*r*t*v*l*e*a*c*e*1*3*5*7*9*a*c*e*1*3*5*"'
      );
    });

    it('should detect and mask JWT tokens', () => {
      const text =
        'Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c';
      const result = maskSensitiveData(text);

      expect(result).toEqual(
        'Bearer *y*h*G*i*i*I*z*1*i*s*n*5*C*6*k*X*C*9*e*J*d*I*O*I*M*M*N*Y*O*k*I*w*b*F*Z*I*I*p*a*4*R*9*I*w*a*F*I*o*N*E*M*M*M*I*f*.*f*K*w*J*M*K*F*Q*4*w*M*J*3*P*k*y*V*a*Q*s*5*'
      );
    });

    it('should detect and mask private keys', () => {
      const text = `-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC...
-----END PRIVATE KEY-----`;
      const result = maskSensitiveData(text);

      expect(result).toEqual(`*-*-*B*G*N*P*I*A*E*K*Y*-*-*
*I*E*Q*B*D*N*g*q*k*G*w*B*Q*F*A*C*K*w*g*j*g*A*o*B*Q*.*.*-*-*-*N* *R*V*T* *E*-*-*-`);
    });
  });

  describe('Masking Pattern', () => {
    it('should mask every second character correctly', () => {
      const text = 'SECRET="1234567890123456"';
      const result = maskSensitiveData(text);

      expect(result).toEqual('*E*R*T*"*2*4*6*8*0*2*4*6*');
    });

    it('should preserve text structure around masked content', () => {
      const text = 'Before SECRET="mysecret123456789" After';
      const result = maskSensitiveData(text);

      expect(result).toEqual('Before *E*R*T*"*y*e*r*t*2*4*6*8*" After');
    });
  });

  describe('Multiple Matches', () => {
    it('should handle multiple sensitive patterns in same text', () => {
      const text = `
        GitHub token: ghp_1234567890123456789012345678901234567890
        OpenAI key: sk-1234567890abcdefT3BlbkFJ1234567890abcdef
        AWS key: AKIAIOSFODNN7EXAMPLE
      `;
      const result = maskSensitiveData(text);

      expect(result).toEqual(`
        GitHub token: *h*_*2*4*6*8*0*2*4*6*8*0*2*4*6*8*0*2*4*6*8*0
        OpenAI key: *k*1*3*5*7*9*a*c*e*T*B*b*F*1*3*5*7*9*a*c*e*
        AWS key: *K*A*O*F*D*N*E*A*P*E
      `);
    });

    it('should handle overlapping matches correctly', () => {
      const text =
        'SECRET_API_KEY="sk-1234567890abcdefT3BlbkFJ" SECRET_TOKEN="abc1234567890123"';
      const result = maskSensitiveData(text);

      expect(result).toEqual(
        '*E*R*T*A*I*K*Y*"*k*1*3*5*7*9*a*c*e*T*B*b*F*" *E*R*T*T*K*N*"*b*1*3*5*7*9*1*3*'
      );
    });

    it('should maintain order when processing multiple matches', () => {
      const text =
        'First: SECRET_ONE="abc1234567890123" Second: SECRET_TWO="def4567890123456" Third: SECRET_THREE="ghi7890123456789"';
      const result = maskSensitiveData(text);

      expect(result).toEqual(
        'First: *E*R*T*O*E*"*b*1*3*5*7*9*1*3* Second: *E*R*T*T*O*"*e*4*6*8*0*2*4*6* Third: *E*R*T*T*R*E*"*h*7*9*1*3*5*7*9*'
      );
    });
  });

  describe('Edge Cases', () => {
    it('should handle very long strings', () => {
      const longString =
        'x'.repeat(10000) + 'SECRET="mysecret12345678"' + 'y'.repeat(10000);
      const result = maskSensitiveData(longString);

      expect(typeof result).toEqual('string');
      expect(result.length).toEqual(20025);
    });

    it('should handle strings with special characters', () => {
      const text = `Special chars: !@#$%^&*(){}[]|\\:";'<>?,./
SECRET="mysecret12345678"	After`;
      const result = maskSensitiveData(text);

      expect(result).toEqual(`Special chars: !@#$%^&*(){}[]|\\:";'<>?,./
*E*R*T*"*y*e*r*t*2*4*6*8*	After`);
    });

    it('should handle zero-length matches gracefully', () => {
      const text = 'Test string with potential zero-length match issues';
      const result = maskSensitiveData(text);

      expect(result).toEqual(
        'Test string with potential zero-length match issues'
      );
    });

    it('should handle newlines and multiline content', () => {
      const text = `Line 1: Normal text
Line 2: SECRET="mysecret12345678"
Line 3: More normal text
Line 4: key="anotherkey456789"`;
      const result = maskSensitiveData(text);

      expect(result).toEqual(`Line 1: Normal text
Line 2: *E*R*T*"*y*e*r*t*2*4*6*8*
Line 3: More normal text
Line 4: *e*=*a*o*h*r*e*4*6*8*"`);
    });

    it('should handle empty matches array', () => {
      const text = 'No sensitive content here at all';
      const result = maskSensitiveData(text);

      expect(result).toBe(text);
    });
  });

  describe('Performance and Caching', () => {
    it('should handle repeated calls efficiently', () => {
      const text = 'SECRET="mysecret12345678"';
      const result1 = maskSensitiveData(text);
      const result2 = maskSensitiveData(text);
      const result3 = maskSensitiveData(text);

      expect(result1).toEqual('*E*R*T*"*y*e*r*t*2*4*6*8*');
      expect(result2).toEqual('*E*R*T*"*y*e*r*t*2*4*6*8*');
      expect(result3).toEqual('*E*R*T*"*y*e*r*t*2*4*6*8*');
    });

    it('should work with SECRET input string', () => {
      const text = 'SECRET="abc1234567890123"';
      const result = maskSensitiveData(text);
      expect(result).toEqual('*E*R*T*"*b*1*3*5*7*9*1*3*');
    });

    it('should work with key input string', () => {
      const text = 'key="def4567890123456"';
      const result = maskSensitiveData(text);
      expect(result).toEqual('*e*=*d*f*5*7*9*1*3*5*"');
    });

    it('should work with TOKEN input string', () => {
      const text = 'TOKEN="ghi7890123456789"';
      const result = maskSensitiveData(text);
      expect(result).toEqual('*O*E*=*g*i*8*0*2*4*6*8*"');
    });

    it('should work with PASSWORD input string', () => {
      const text = 'PASSWORD="jkl0123456789012"';
      const result = maskSensitiveData(text);
      expect(result).toEqual('*A*S*O*D*"*k*0*2*4*6*8*0*2*');
    });
  });

  describe('Regex Edge Cases', () => {
    it('should handle SECRET==doubleequals', () => {
      const text = 'SECRET==doubleequals';
      const result = maskSensitiveData(text);
      expect(result).toEqual('SECRET==doubleequals');
    });

    it('should handle SECRET=', () => {
      const text = 'SECRET=';
      const result = maskSensitiveData(text);
      expect(result).toEqual('SECRET=');
    });

    it('should handle =SECRET', () => {
      const text = '=SECRET';
      const result = maskSensitiveData(text);
      expect(result).toEqual('=SECRET');
    });

    it('should handle SECRET===triple', () => {
      const text = 'SECRET===triple';
      const result = maskSensitiveData(text);
      expect(result).toEqual('SECRET===triple');
    });

    it('should handle SECRET=with newline', () => {
      const text = `SECRET=with
newline`;
      const result = maskSensitiveData(text);
      expect(result).toEqual(`SECRET=with
newline`);
    });

    it('should handle SECRET=with tab', () => {
      const text = `SECRET=with	tab`;
      const result = maskSensitiveData(text);
      expect(result).toEqual(`SECRET=with	tab`);
    });

    it('should handle Unicode characters', () => {
      const text = 'SECRET="密码1234567890123" API_KEY="токен45678901234"';
      const result = maskSensitiveData(text);

      expect(result).toEqual(
        'SECRET="密码1234567890123" *P*_*E*=*т*к*н*5*7*9*1*3*"'
      );
    });
  });
});
