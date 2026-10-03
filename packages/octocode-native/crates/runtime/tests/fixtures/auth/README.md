# Main credential fixture

`main-credentials.enc` and `main-key.hex` contain synthetic test data, not a usable
GitHub login. The key is 32 bytes of `07`; the IV is 16 bytes of `03`.

Generated with Node's `createCipheriv('aes-256-gcm', key, iv)`, matching main's
`credentialEncryption.ts`. The file contains `ivHex:tagHex:ciphertextHex`.
The decrypted JSON has `version: 1` and one `credentials['127.0.0.1']` entry:

```json
{
  "hostname": "127.0.0.1",
  "username": "legacy-home-user",
  "token": {
    "token": "synthetic-legacy-home-token",
    "tokenType": "oauth",
    "refreshToken": "synthetic-legacy-refresh",
    "scopes": ["repo"]
  },
  "gitProtocol": "https",
  "createdAt": "2026-01-01T00:00:00Z",
  "updatedAt": "2026-01-01T00:00:00Z"
}
```
