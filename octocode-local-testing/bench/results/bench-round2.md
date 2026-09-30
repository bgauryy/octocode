# bench round2 (2026-09-29T20:44:59.265Z)

## pr suite

| task | arm | chars | ≈tokens | calls | ms | recall | read P | files read | correct |
|---|---|--:|--:|--:|--:|--:|--:|--:|:-:|
| ts-51387 | gh-typical | 6854k | 1714k | 3 | 6.1k | 1 | 0.005 | 414 | ✓ |
| ts-51387 | gh-lean | 3.8k | 942 | 2 | 4.2k | 1 | 1 | 2 | ✓ |
| ts-51387 | octocode | 40k | 9.9k | 4 | 8.1k | 1 | 1 | 2 | ✓ |
| ts-51387 | octocode-direct | 1.1k | 284 | 1 | 1.8k | 1 | 1 | 2 | ✓ |
| ts-51387 | octocode+clasify | 78k | 19k | 7 | 37k | 1 | 1 | 2 | ✓ |
| rust-157558 | gh-typical | 1417k | 354k | 3 | 5.3k | 1 | 0.007 | 425 | ✓ |
| rust-157558 | gh-lean | 1.2k | 311 | 2 | 5.0k | 1 | 1 | 3 | ✓ |
| rust-157558 | octocode | 31k | 7.7k | 4 | 7.0k | 1 | 1 | 3 | ✓ |
| rust-157558 | octocode-direct | 1.8k | 453 | 2 | 2.8k | 1 | 1 | 3 | ✓ |
| rust-157558 | octocode+clasify | 36k | 9.0k | 5 | 16k | 1 | 1 | 3 | ✓ |
| ts-61986 | gh-typical | 7269k | 1817k | 2 | 2.9k | 1 | 0.333 | 12 | ✓ |
| ts-61986 | gh-lean | 40k | 9.9k | 3 | 2.8k | 1 | 0.667 | 6 | ✓ |
| ts-61986 | octocode | 17k | 4.2k | 4 | 4.9k | 1 | 0.667 | 6 | ✓ |
| ts-61986 | octocode-direct | 6.3k | 1.6k | 3 | 2.3k | 1 | 0.667 | 6 | ✓ |
| ts-61986 | octocode+clasify | 20k | 4.9k | 7 | 11k | 1 | 1 | 4 | ✓ |
| tokio-8156 | gh-typical | 49k | 12k | 2 | 2.0k | 1 | 0.081 | 37 | ✓ |
| tokio-8156 | gh-lean | 725 | 181 | 2 | 1.1k | 1 | 1 | 3 | ✓ |
| tokio-8156 | octocode | 8.5k | 2.1k | 3 | 2.9k | 1 | 1 | 3 | ✓ |
| tokio-8156 | octocode-direct | 1.2k | 304 | 1 | 954 | 1 | 1 | 3 | ✓ |
| tokio-8156 | octocode+clasify | 34k | 8.5k | 5 | 14k | 1 | 0.273 | 11 | ✓ |

| arm | correct | chars | ≈tokens | calls | ms | mean recall | files read |
|---|--:|--:|--:|--:|--:|--:|--:|
| gh-typical | 4/4 | 15590k | 3898k | 10 | 16k | 1 | 888 |
| gh-lean | 4/4 | 45k | 11k | 9 | 13k | 1 | 14 |
| octocode | 4/4 | 96k | 24k | 15 | 23k | 1 | 14 |
| octocode-direct | 4/4 | 10k | 2.6k | 7 | 7.8k | 1 | 14 |
| octocode+clasify | 4/4 | 168k | 42k | 24 | 78k | 1 | 20 |

## local suite

| task | arm | chars | ≈tokens | calls | ms | recall | read P | files read | correct |
|---|---|--:|--:|--:|--:|--:|--:|--:|:-:|
| ts-symbol | rg/sed | 1.5k | 365 | 2 | 73 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| ts-symbol | octocode | 2.1k | 519 | 1 | 127 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| ts-symbol | octocode+clasify | 12k | 2.9k | 3 | 1.7k | 0 (usages R 1 / P 1) | 0 | 1 | ✗ |
| ts-how | rg/sed | 6.2k | 1.6k | 2 | 40 | 1 | 0.667 | 3 | ✓ |
| ts-how | octocode | 8.0k | 2.0k | 2 | 115 | 1 | 0.667 | 3 | ✓ |
| ts-how | octocode+clasify | 8.5k | 2.1k | 2 | 721 | 0.5 | 0.5 | 2 | ✗ |
| ts-how | octocode+clasify(files) | 7.5k | 1.9k | 3 | 789 | 1 | 0.5 | 2 | ✓ |
| ts-unknown | rg/sed | 5.9k | 1.5k | 2 | 43 | 1 | 0.333 | 3 | ✓ |
| ts-unknown | octocode | 7.8k | 2.0k | 2 | 217 | 1 | 0.333 | 3 | ✓ |
| ts-unknown | octocode+clasify | 3.5k | 887 | 2 | 489 | 1 | 1 | 1 | ✓ |
| ts-unknown | octocode+clasify(files) | 6.4k | 1.6k | 3 | 947 | 1 | 1 | 1 | ✓ |
| rust-symbol | rg/sed | 1.7k | 414 | 2 | 38 | 1 (usages R 1 / P 1) | 0.167 | 6 | ✓ |
| rust-symbol | octocode | 2.2k | 542 | 1 | 98 | 1 (usages R 1 / P 1) | 0.167 | 6 | ✓ |
| rust-symbol | octocode+clasify | 8.7k | 2.2k | 3 | 925 | 1 (usages R 1 / P 1) | 0.5 | 2 | ✓ |
| rust-how | rg/sed | 8.4k | 2.1k | 2 | 24 | 1 | 0.667 | 3 | ✓ |
| rust-how | octocode | 12k | 3.1k | 2 | 150 | 1 | 0.667 | 3 | ✓ |
| rust-how | octocode+clasify | 10k | 2.5k | 2 | 814 | 1 | 1 | 2 | ✓ |
| rust-how | octocode+clasify(files) | 13k | 3.4k | 3 | 796 | 1 | 1 | 2 | ✓ |
| rust-unknown | rg/sed | 9.6k | 2.4k | 2 | 28 | 1 | 0.333 | 3 | ✓ |
| rust-unknown | octocode | 11k | 2.8k | 2 | 97 | 1 | 0.333 | 3 | ✓ |
| rust-unknown | octocode+clasify | 6.8k | 1.7k | 2 | 743 | 1 | 1 | 1 | ✓ |
| rust-unknown | octocode+clasify(files) | 6.5k | 1.6k | 3 | 637 | 1 | 1 | 1 | ✓ |
| go-symbol | rg/sed | 208 | 52 | 2 | 83 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| go-symbol | octocode | 785 | 196 | 1 | 189 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| go-symbol | octocode+clasify | 5.7k | 1.4k | 3 | 986 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| go-how | rg/sed | 10k | 2.6k | 2 | 24 | 1 | 0.667 | 3 | ✓ |
| go-how | octocode | 11k | 2.9k | 2 | 154 | 1 | 0.667 | 3 | ✓ |
| go-how | octocode+clasify | 9.4k | 2.4k | 2 | 881 | 1 | 1 | 2 | ✓ |
| go-how | octocode+clasify(files) | 11k | 2.8k | 3 | 885 | 1 | 1 | 2 | ✓ |
| go-unknown | rg/sed | 11k | 2.8k | 2 | 25 | 1 | 0.333 | 3 | ✓ |
| go-unknown | octocode | 11k | 2.7k | 2 | 161 | 1 | 0.333 | 3 | ✓ |
| go-unknown | octocode+clasify | 8.4k | 2.1k | 2 | 1.2k | 1 | 1 | 1 | ✓ |
| go-unknown | octocode+clasify(files) | 6.3k | 1.6k | 3 | 891 | 1 | 1 | 1 | ✓ |
| py-symbol | rg/sed | 521 | 130 | 2 | 368 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| py-symbol | octocode | 1.2k | 296 | 1 | 488 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| py-symbol | octocode+clasify | 5.8k | 1.5k | 3 | 902 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| py-how | rg/sed | 12k | 2.9k | 2 | 25 | 1 | 0.667 | 3 | ✓ |
| py-how | octocode | 15k | 3.8k | 2 | 323 | 1 | 0.667 | 3 | ✓ |
| py-how | octocode+clasify | 11k | 2.8k | 2 | 907 | 1 | 0.667 | 3 | ✓ |
| py-how | octocode+clasify(files) | 20k | 5.1k | 4 | 2.2k | 1 | 0.667 | 3 | ✓ |
| py-unknown | rg/sed | 9.4k | 2.4k | 2 | 131 | 1 | 1 | 3 | ✓ |
| py-unknown | octocode | 11k | 2.8k | 2 | 202 | 1 | 1 | 3 | ✓ |
| py-unknown | octocode+clasify | 7.9k | 2.0k | 2 | 598 | 0.67 | 1 | 2 | ✗ |
| py-unknown | octocode+clasify(files) | 8.7k | 2.2k | 3 | 795 | 1 | 1 | 2 | ✓ |
| java-symbol | rg/sed | 1.1k | 269 | 2 | 56 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| java-symbol | octocode | 1.3k | 326 | 1 | 98 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| java-symbol | octocode+clasify | 6.8k | 1.7k | 3 | 1.6k | 0 (usages R 1 / P 1) | 0 | 1 | ✗ |
| java-how | rg/sed | 14k | 3.6k | 2 | 23 | 0.5 | 0.667 | 3 | ✗ |
| java-how | octocode | 13k | 3.2k | 2 | 155 | 0.5 | 0.667 | 3 | ✗ |
| java-how | octocode+clasify | 12k | 2.9k | 2 | 892 | 0.5 | 0.667 | 3 | ✗ |
| java-how | octocode+clasify(files) | 13k | 3.2k | 3 | 1.1k | 1 | 1 | 2 | ✓ |
| java-unknown | rg/sed | 13k | 3.2k | 2 | 25 | 1 | 0.333 | 3 | ✓ |
| java-unknown | octocode | 13k | 3.2k | 2 | 154 | 1 | 0.333 | 3 | ✓ |
| java-unknown | octocode+clasify | 5.9k | 1.5k | 2 | 752 | 1 | 1 | 1 | ✓ |
| java-unknown | octocode+clasify(files) | 9.0k | 2.2k | 3 | 784 | 1 | 1 | 1 | ✓ |

| arm | correct | chars | ≈tokens | calls | ms | mean recall | files read |
|---|--:|--:|--:|--:|--:|--:|--:|
| rg/sed | 14/15 | 105k | 26k | 30 | 1.0k | 0.97 | 40 |
| octocode | 14/15 | 121k | 30k | 25 | 2.7k | 0.97 | 40 |
| octocode+clasify | 10/15 | 122k | 31k | 35 | 14k | 0.78 | 24 |
| octocode+clasify(files) | 10/10 | 102k | 26k | 31 | 9.8k | 1 | 17 |
