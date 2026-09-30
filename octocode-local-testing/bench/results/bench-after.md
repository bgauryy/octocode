# bench after (2026-09-29T18:16:38.199Z)

## pr suite

| task | arm | chars | ≈tokens | calls | ms | recall | read P | files read | correct |
|---|---|--:|--:|--:|--:|--:|--:|--:|:-:|
| ts-51387 | gh-lean | 3.8k | 942 | 2 | 4.5k | 1 | 1 | 2 | ✓ |
| ts-51387 | octocode | 46k | 11k | 9 | 8.8k | 1 | 1 | 2 | ✓ |
| rust-157558 | gh-lean | 1.2k | 311 | 2 | 4.0k | 1 | 1 | 3 | ✓ |
| rust-157558 | octocode | 35k | 8.7k | 7 | 7.0k | 1 | 1 | 3 | ✓ |
| ts-61986 | gh-lean | 40k | 9.9k | 3 | 3.1k | 1 | 0.667 | 6 | ✓ |
| ts-61986 | octocode | 18k | 4.5k | 4 | 5.0k | 1 | 0.667 | 6 | ✓ |
| tokio-8156 | gh-lean | 725 | 181 | 2 | 1.1k | 1 | 1 | 3 | ✓ |
| tokio-8156 | octocode | 9.3k | 2.3k | 3 | 3.2k | 1 | 1 | 3 | ✓ |

| arm | correct | chars | ≈tokens | calls | ms | mean recall | files read |
|---|--:|--:|--:|--:|--:|--:|--:|
| gh-lean | 4/4 | 45k | 11k | 9 | 13k | 1 | 14 |
| octocode | 4/4 | 108k | 27k | 23 | 24k | 1 | 14 |
