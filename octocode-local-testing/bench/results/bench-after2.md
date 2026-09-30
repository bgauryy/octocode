# bench after2 (2026-09-29T18:21:44.661Z)

## pr suite

| task | arm | chars | ≈tokens | calls | ms | recall | read P | files read | correct |
|---|---|--:|--:|--:|--:|--:|--:|--:|:-:|
| ts-51387 | gh-lean | 3.8k | 942 | 2 | 4.8k | 1 | 1 | 2 | ✓ |
| ts-51387 | octocode | 40k | 10k | 4 | 7.8k | 1 | 1 | 2 | ✓ |
| ts-51387 | octocode-direct | 4.7k | 1.2k | 1 | 1.5k | 1 | 1 | 2 | ✓ |
| rust-157558 | gh-lean | 1.2k | 311 | 2 | 3.3k | 1 | 1 | 3 | ✓ |
| rust-157558 | octocode | 32k | 7.9k | 4 | 6.6k | 1 | 1 | 3 | ✓ |
| rust-157558 | octocode-direct | 4.2k | 1.1k | 2 | 3.9k | 1 | 1 | 3 | ✓ |
| ts-61986 | gh-lean | 40k | 9.9k | 3 | 2.0k | 1 | 0.667 | 6 | ✓ |
| ts-61986 | octocode | 18k | 4.4k | 4 | 4.8k | 1 | 0.667 | 6 | ✓ |
| ts-61986 | octocode-direct | 14k | 3.6k | 3 | 2.6k | 1 | 0.667 | 6 | ✓ |
| tokio-8156 | gh-lean | 725 | 181 | 2 | 985 | 1 | 1 | 3 | ✓ |
| tokio-8156 | octocode | 9.3k | 2.3k | 3 | 3.5k | 1 | 1 | 3 | ✓ |
| tokio-8156 | octocode-direct | 3.6k | 892 | 1 | 1.0k | 1 | 1 | 3 | ✓ |

| arm | correct | chars | ≈tokens | calls | ms | mean recall | files read |
|---|--:|--:|--:|--:|--:|--:|--:|
| gh-lean | 4/4 | 45k | 11k | 9 | 11k | 1 | 14 |
| octocode | 4/4 | 99k | 25k | 15 | 23k | 1 | 14 |
| octocode-direct | 4/4 | 27k | 6.7k | 7 | 8.9k | 1 | 14 |
