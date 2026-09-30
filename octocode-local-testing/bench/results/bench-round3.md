# bench round3 (2026-09-29T21:23:19.870Z)

## pr suite

| task | arm | chars | ≈tokens | calls | ms | recall | read P | files read | correct |
|---|---|--:|--:|--:|--:|--:|--:|--:|:-:|
| ts-51387 | gh-typical | 6854k | 1714k | 3 | 6.1k | 1 | 0.005 | 414 | ✓ |
| ts-51387 | gh-lean | 3.8k | 942 | 2 | 4.4k | 1 | 1 | 2 | ✓ |
| ts-51387 | octocode | 36k | 9.1k | 4 | 8.4k | 1 | 1 | 2 | ✓ |
| ts-51387 | octocode-direct | 978 | 245 | 1 | 1.7k | 1 | 1 | 2 | ✓ |
| ts-51387 | octocode+clasify | 74k | 19k | 7 | 39k | 1 | 1 | 2 | ✓ |
| rust-157558 | gh-typical | 1417k | 354k | 3 | 4.4k | 1 | 0.007 | 425 | ✓ |
| rust-157558 | gh-lean | 1.2k | 311 | 2 | 2.9k | 1 | 1 | 3 | ✓ |
| rust-157558 | octocode | 28k | 7.1k | 4 | 5.9k | 1 | 1 | 3 | ✓ |
| rust-157558 | octocode-direct | 1.6k | 388 | 2 | 3.5k | 1 | 1 | 3 | ✓ |
| rust-157558 | octocode+clasify | 34k | 8.4k | 5 | 15k | 1 | 1 | 3 | ✓ |
| ts-61986 | gh-typical | 7269k | 1817k | 2 | 2.8k | 1 | 0.333 | 12 | ✓ |
| ts-61986 | gh-lean | 40k | 9.9k | 3 | 3.1k | 1 | 0.667 | 6 | ✓ |
| ts-61986 | octocode | 16k | 4.0k | 4 | 5.1k | 1 | 0.667 | 6 | ✓ |
| ts-61986 | octocode-direct | 6.0k | 1.5k | 3 | 2.3k | 1 | 0.667 | 6 | ✓ |
| ts-61986 | octocode+clasify | 19k | 4.7k | 7 | 11k | 1 | 1 | 4 | ✓ |
| tokio-8156 | gh-typical | 49k | 12k | 2 | 2.1k | 1 | 0.081 | 37 | ✓ |
| tokio-8156 | gh-lean | 725 | 181 | 2 | 1.1k | 1 | 1 | 3 | ✓ |
| tokio-8156 | octocode | 7.5k | 1.9k | 3 | 2.5k | 1 | 1 | 3 | ✓ |
| tokio-8156 | octocode-direct | 1.1k | 263 | 1 | 908 | 1 | 1 | 3 | ✓ |
| tokio-8156 | octocode+clasify | 33k | 8.2k | 5 | 14k | 1 | 0.273 | 11 | ✓ |

| arm | correct | chars | ≈tokens | calls | ms | mean recall | files read |
|---|--:|--:|--:|--:|--:|--:|--:|
| gh-typical | 4/4 | 15590k | 3898k | 10 | 15k | 1 | 888 |
| gh-lean | 4/4 | 45k | 11k | 9 | 11k | 1 | 14 |
| octocode | 4/4 | 88k | 22k | 15 | 22k | 1 | 14 |
| octocode-direct | 4/4 | 9.6k | 2.4k | 7 | 8.3k | 1 | 14 |
| octocode+clasify | 4/4 | 159k | 40k | 24 | 79k | 1 | 20 |

## local suite

| task | arm | chars | ≈tokens | calls | ms | recall | read P | files read | correct |
|---|---|--:|--:|--:|--:|--:|--:|--:|:-:|
| ts-symbol | rg/sed | 1.5k | 365 | 2 | 70 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| ts-symbol | octocode | 1.9k | 471 | 1 | 109 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| ts-symbol | octocode+clasify | 10.0k | 2.5k | 3 | 1.3k | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| ts-how | rg/sed | 6.2k | 1.6k | 2 | 35 | 1 | 0.667 | 3 | ✓ |
| ts-how | octocode | 7.4k | 1.9k | 2 | 94 | 1 | 0.667 | 3 | ✓ |
| ts-how | octocode+clasify | 7.5k | 1.9k | 2 | 521 | 0.5 | 0.5 | 2 | ✗ |
| ts-how | octocode+clasify(files) | 8.2k | 2.1k | 3 | 653 | 1 | 0.667 | 3 | ✓ |
| ts-unknown | rg/sed | 5.9k | 1.5k | 2 | 37 | 1 | 0.333 | 3 | ✓ |
| ts-unknown | octocode | 7.2k | 1.8k | 2 | 173 | 1 | 0.333 | 3 | ✓ |
| ts-unknown | octocode+clasify | 3.2k | 795 | 2 | 441 | 1 | 1 | 1 | ✓ |
| ts-unknown | octocode+clasify(files) | 5.9k | 1.5k | 3 | 949 | 1 | 1 | 1 | ✓ |
| rust-symbol | rg/sed | 1.7k | 414 | 2 | 39 | 1 (usages R 1 / P 1) | 0.167 | 6 | ✓ |
| rust-symbol | octocode | 2.0k | 489 | 1 | 78 | 1 (usages R 1 / P 1) | 0.167 | 6 | ✓ |
| rust-symbol | octocode+clasify | 6.6k | 1.6k | 3 | 543 | 1 (usages R 1 / P 1) | 0.5 | 2 | ✓ |
| rust-how | rg/sed | 8.4k | 2.1k | 2 | 22 | 1 | 0.667 | 3 | ✓ |
| rust-how | octocode | 11k | 2.8k | 2 | 128 | 1 | 0.667 | 3 | ✓ |
| rust-how | octocode+clasify | 7.5k | 1.9k | 2 | 727 | 1 | 1 | 2 | ✓ |
| rust-how | octocode+clasify(files) | 13k | 3.1k | 3 | 824 | 1 | 1 | 2 | ✓ |
| rust-unknown | rg/sed | 9.6k | 2.4k | 2 | 24 | 1 | 0.333 | 3 | ✓ |
| rust-unknown | octocode | 10k | 2.6k | 2 | 79 | 1 | 0.333 | 3 | ✓ |
| rust-unknown | octocode+clasify | 6.6k | 1.6k | 2 | 716 | 1 | 1 | 1 | ✓ |
| rust-unknown | octocode+clasify(files) | 6.1k | 1.5k | 3 | 600 | 1 | 1 | 1 | ✓ |
| go-symbol | rg/sed | 208 | 52 | 2 | 89 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| go-symbol | octocode | 749 | 187 | 1 | 143 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| go-symbol | octocode+clasify | 5.5k | 1.4k | 3 | 878 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| go-how | rg/sed | 10k | 2.6k | 2 | 21 | 1 | 0.667 | 3 | ✓ |
| go-how | octocode | 11k | 2.7k | 2 | 125 | 1 | 0.667 | 3 | ✓ |
| go-how | octocode+clasify | 8.5k | 2.1k | 2 | 785 | 1 | 1 | 2 | ✓ |
| go-how | octocode+clasify(files) | 11k | 2.6k | 3 | 837 | 1 | 1 | 2 | ✓ |
| go-unknown | rg/sed | 11k | 2.8k | 2 | 26 | 1 | 0.333 | 3 | ✓ |
| go-unknown | octocode | 10k | 2.5k | 2 | 132 | 1 | 0.333 | 3 | ✓ |
| go-unknown | octocode+clasify | 7.5k | 1.9k | 2 | 857 | 1 | 1 | 1 | ✓ |
| go-unknown | octocode+clasify(files) | 5.9k | 1.5k | 3 | 824 | 1 | 1 | 1 | ✓ |
| py-symbol | rg/sed | 521 | 130 | 2 | 340 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| py-symbol | octocode | 1.1k | 275 | 1 | 478 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| py-symbol | octocode+clasify | 5.3k | 1.3k | 3 | 922 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| py-how | rg/sed | 12k | 2.9k | 2 | 26 | 1 | 0.667 | 3 | ✓ |
| py-how | octocode | 14k | 3.5k | 2 | 274 | 1 | 0.667 | 3 | ✓ |
| py-how | octocode+clasify | 10k | 2.6k | 2 | 790 | 1 | 0.667 | 3 | ✓ |
| py-how | octocode+clasify(files) | 19k | 4.8k | 4 | 2.1k | 1 | 0.667 | 3 | ✓ |
| py-unknown | rg/sed | 9.4k | 2.4k | 2 | 114 | 1 | 1 | 3 | ✓ |
| py-unknown | octocode | 11k | 2.7k | 2 | 180 | 1 | 1 | 3 | ✓ |
| py-unknown | octocode+clasify | 7.6k | 1.9k | 2 | 595 | 0.67 | 1 | 2 | ✗ |
| py-unknown | octocode+clasify(files) | 8.2k | 2.1k | 3 | 809 | 1 | 1 | 2 | ✓ |
| java-symbol | rg/sed | 1.1k | 269 | 2 | 50 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| java-symbol | octocode | 1.2k | 300 | 1 | 85 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| java-symbol | octocode+clasify | 6.7k | 1.7k | 3 | 751 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| java-how | rg/sed | 14k | 3.6k | 2 | 23 | 0.5 | 0.667 | 3 | ✗ |
| java-how | octocode | 12k | 3.0k | 2 | 132 | 0.5 | 0.667 | 3 | ✗ |
| java-how | octocode+clasify | 11k | 2.7k | 2 | 829 | 0.5 | 0.667 | 3 | ✗ |
| java-how | octocode+clasify(files) | 12k | 3.1k | 3 | 987 | 1 | 1 | 2 | ✓ |
| java-unknown | rg/sed | 13k | 3.2k | 2 | 22 | 1 | 0.333 | 3 | ✓ |
| java-unknown | octocode | 12k | 3.0k | 2 | 127 | 1 | 0.333 | 3 | ✓ |
| java-unknown | octocode+clasify | 5.7k | 1.4k | 2 | 707 | 1 | 1 | 1 | ✓ |
| java-unknown | octocode+clasify(files) | 8.4k | 2.1k | 3 | 682 | 1 | 1 | 1 | ✓ |

| arm | correct | chars | ≈tokens | calls | ms | mean recall | files read |
|---|--:|--:|--:|--:|--:|--:|--:|
| rg/sed | 14/15 | 105k | 26k | 30 | 938 | 0.97 | 40 |
| octocode | 14/15 | 113k | 28k | 25 | 2.3k | 0.97 | 40 |
| octocode+clasify | 12/15 | 109k | 27k | 35 | 11k | 0.91 | 24 |
| octocode+clasify(files) | 10/10 | 97k | 24k | 31 | 9.3k | 1 | 18 |
