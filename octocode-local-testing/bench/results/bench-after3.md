# bench after3 (2026-09-29T19:06:29.907Z)

## pr suite

| task | arm | chars | ≈tokens | calls | ms | recall | read P | files read | correct |
|---|---|--:|--:|--:|--:|--:|--:|--:|:-:|
| ts-51387 | gh-lean | 3.8k | 942 | 2 | 4.1k | 1 | 1 | 2 | ✓ |
| ts-51387 | octocode | 40k | 9.9k | 4 | 7.9k | 1 | 1 | 2 | ✓ |
| ts-51387 | octocode-direct | 1.1k | 284 | 1 | 2.0k | 1 | 1 | 2 | ✓ |
| rust-157558 | gh-lean | 1.2k | 311 | 2 | 2.8k | 1 | 1 | 3 | ✓ |
| rust-157558 | octocode | 31k | 7.7k | 4 | 5.8k | 1 | 1 | 3 | ✓ |
| rust-157558 | octocode-direct | 1.8k | 453 | 2 | 3.7k | 1 | 1 | 3 | ✓ |
| ts-61986 | gh-lean | 40k | 9.9k | 3 | 2.2k | 1 | 0.667 | 6 | ✓ |
| ts-61986 | octocode | 17k | 4.2k | 4 | 6.0k | 1 | 0.667 | 6 | ✓ |
| ts-61986 | octocode-direct | 6.3k | 1.6k | 3 | 2.4k | 1 | 0.667 | 6 | ✓ |
| tokio-8156 | gh-lean | 725 | 181 | 2 | 1.0k | 1 | 1 | 3 | ✓ |
| tokio-8156 | octocode | 8.5k | 2.1k | 3 | 2.7k | 1 | 1 | 3 | ✓ |
| tokio-8156 | octocode-direct | 1.2k | 304 | 1 | 944 | 1 | 1 | 3 | ✓ |

| arm | correct | chars | ≈tokens | calls | ms | mean recall | files read |
|---|--:|--:|--:|--:|--:|--:|--:|
| gh-lean | 4/4 | 45k | 11k | 9 | 10k | 1 | 14 |
| octocode | 4/4 | 96k | 24k | 15 | 22k | 1 | 14 |
| octocode-direct | 4/4 | 10k | 2.6k | 7 | 9.1k | 1 | 14 |
