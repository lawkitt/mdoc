# Pseudonymization measurements

Generated from raw reports. Exact source-byte span and category scoring; long and stress fixtures are separate from the short-text baseline.

## gliner2-pii-fp16 — threshold 0.3

Process success: true; profile: release; platform: macos-aarch64; machine: "Apple M4; 24 GiB; macOS 15.7.7; rustc 1.98.1".

Verify 310.9 ms; load 859.6 ms; drop 140.2 ms; peak RSS 2315.5 MiB; pinned setup 631516494 bytes.

| Language | Category | Gold | TP | Misses | FP | Precision | Recall |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| en | ADDRESS | 2 | 2 | 0 | 2 | 50.0% | 100.0% |
| en | ALL | 31 | 27 | 4 | 8 | 77.1% | 87.1% |
| en | BANK | 2 | 2 | 0 | 2 | 50.0% | 100.0% |
| en | EMAIL | 5 | 5 | 0 | 0 | 100.0% | 100.0% |
| en | IDENTITY | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| en | ORG | 3 | 2 | 1 | 3 | 40.0% | 66.7% |
| en | PERSON | 13 | 10 | 3 | 1 | 90.9% | 76.9% |
| en | PHONE | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| en | TAX | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| mixed | ADDRESS | 0 | 0 | 0 | 0 | — | — |
| mixed | ALL | 5 | 4 | 1 | 1 | 80.0% | 80.0% |
| mixed | BANK | 0 | 0 | 0 | 0 | — | — |
| mixed | EMAIL | 1 | 1 | 0 | 0 | 100.0% | 100.0% |
| mixed | IDENTITY | 0 | 0 | 0 | 0 | — | — |
| mixed | ORG | 1 | 0 | 1 | 1 | 0.0% | 0.0% |
| mixed | PERSON | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| mixed | PHONE | 1 | 1 | 0 | 0 | 100.0% | 100.0% |
| mixed | TAX | 0 | 0 | 0 | 0 | — | — |
| ru | ADDRESS | 2 | 0 | 2 | 1 | 0.0% | 0.0% |
| ru | ALL | 32 | 22 | 10 | 10 | 68.8% | 68.8% |
| ru | BANK | 3 | 3 | 0 | 1 | 75.0% | 100.0% |
| ru | EMAIL | 3 | 3 | 0 | 0 | 100.0% | 100.0% |
| ru | IDENTITY | 3 | 2 | 1 | 2 | 50.0% | 66.7% |
| ru | ORG | 4 | 2 | 2 | 3 | 40.0% | 50.0% |
| ru | PERSON | 12 | 10 | 2 | 0 | 100.0% | 83.3% |
| ru | PHONE | 2 | 2 | 0 | 1 | 66.7% | 100.0% |
| ru | TAX | 3 | 0 | 3 | 2 | 0.0% | 0.0% |

| Fixture | Bytes | First ms | Warm median ms | TP | Misses | FP | Invalid offsets | Deterministic | Error |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| en-contract | 268 | 138.4 | 163.3 | 6 | 0 | 0 | 0 | true |  |
| ru-contract | 509 | 157.6 | 161.2 | 4 | 2 | 1 | 0 | true |  |
| en-identifiers | 190 | 123.1 | 141.8 | 4 | 0 | 1 | 0 | true |  |
| ru-identifiers | 268 | 126.0 | 144.5 | 4 | 2 | 4 | 0 | true |  |
| en-variants | 129 | 104.6 | 133.4 | 4 | 1 | 1 | 0 | true |  |
| ru-variants | 334 | 119.6 | 124.2 | 4 | 1 | 1 | 0 | true |  |
| en-ocr | 272 | 142.3 | 142.8 | 7 | 1 | 2 | 0 | true |  |
| ru-ocr | 405 | 141.1 | 141.7 | 6 | 2 | 1 | 0 | true |  |
| en-markdown | 331 | 181.8 | 156.1 | 6 | 2 | 0 | 0 | true |  |
| ru-markdown | 355 | 162.4 | 162.3 | 4 | 3 | 1 | 0 | true |  |
| mixed-unicode | 186 | 115.3 | 105.9 | 4 | 1 | 1 | 0 | true |  |
| en-negative | 258 | 121.2 | 129.2 | 0 | 0 | 4 | 0 | true |  |
| ru-negative | 375 | 115.8 | 132.1 | 0 | 0 | 2 | 0 | true |  |
| en-long | 24512 | 7685.1 | — | 192 | 0 | 17 | 0 | true |  |
| ru-long | 48960 | 8859.9 | — | 128 | 64 | 68 | 0 | true |  |
| en-subwords | 5584 | 1613.9 | 1576.1 | 2 | 0 | 0 | 0 | true |  |
| ru-subwords | 10797 | 1170.5 | 1215.2 | 2 | 0 | 0 | 0 | true |  |

## gliner2-pii-fp16 — threshold 0.5

Process success: true; profile: release; platform: macos-aarch64; machine: "Apple M4; 24 GiB; macOS 15.7.7; rustc 1.98.1".

Verify 312.8 ms; load 946.9 ms; drop 142.9 ms; peak RSS 2266.0 MiB; pinned setup 631516494 bytes.

| Language | Category | Gold | TP | Misses | FP | Precision | Recall |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| en | ADDRESS | 2 | 2 | 0 | 1 | 66.7% | 100.0% |
| en | ALL | 31 | 26 | 5 | 6 | 81.2% | 83.9% |
| en | BANK | 2 | 2 | 0 | 1 | 66.7% | 100.0% |
| en | EMAIL | 5 | 5 | 0 | 0 | 100.0% | 100.0% |
| en | IDENTITY | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| en | ORG | 3 | 2 | 1 | 3 | 40.0% | 66.7% |
| en | PERSON | 13 | 9 | 4 | 1 | 90.0% | 69.2% |
| en | PHONE | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| en | TAX | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| mixed | ADDRESS | 0 | 0 | 0 | 0 | — | — |
| mixed | ALL | 5 | 4 | 1 | 1 | 80.0% | 80.0% |
| mixed | BANK | 0 | 0 | 0 | 0 | — | — |
| mixed | EMAIL | 1 | 1 | 0 | 0 | 100.0% | 100.0% |
| mixed | IDENTITY | 0 | 0 | 0 | 0 | — | — |
| mixed | ORG | 1 | 0 | 1 | 1 | 0.0% | 0.0% |
| mixed | PERSON | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| mixed | PHONE | 1 | 1 | 0 | 0 | 100.0% | 100.0% |
| mixed | TAX | 0 | 0 | 0 | 0 | — | — |
| ru | ADDRESS | 2 | 0 | 2 | 1 | 0.0% | 0.0% |
| ru | ALL | 32 | 22 | 10 | 8 | 73.3% | 68.8% |
| ru | BANK | 3 | 3 | 0 | 1 | 75.0% | 100.0% |
| ru | EMAIL | 3 | 3 | 0 | 0 | 100.0% | 100.0% |
| ru | IDENTITY | 3 | 2 | 1 | 2 | 50.0% | 66.7% |
| ru | ORG | 4 | 2 | 2 | 3 | 40.0% | 50.0% |
| ru | PERSON | 12 | 10 | 2 | 0 | 100.0% | 83.3% |
| ru | PHONE | 2 | 2 | 0 | 1 | 66.7% | 100.0% |
| ru | TAX | 3 | 0 | 3 | 0 | — | 0.0% |

| Fixture | Bytes | First ms | Warm median ms | TP | Misses | FP | Invalid offsets | Deterministic | Error |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| en-contract | 268 | 179.2 | 143.0 | 6 | 0 | 0 | 0 | true |  |
| ru-contract | 509 | 152.0 | 145.0 | 4 | 2 | 1 | 0 | true |  |
| en-identifiers | 190 | 113.2 | 114.7 | 4 | 0 | 1 | 0 | true |  |
| ru-identifiers | 268 | 127.9 | 153.1 | 4 | 2 | 2 | 0 | true |  |
| en-variants | 129 | 116.3 | 129.8 | 3 | 2 | 1 | 0 | true |  |
| ru-variants | 334 | 132.9 | 127.5 | 4 | 1 | 1 | 0 | true |  |
| en-ocr | 272 | 122.5 | 132.3 | 7 | 1 | 1 | 0 | true |  |
| ru-ocr | 405 | 143.6 | 140.0 | 6 | 2 | 1 | 0 | true |  |
| en-markdown | 331 | 151.5 | 151.9 | 6 | 2 | 0 | 0 | true |  |
| ru-markdown | 355 | 158.2 | 156.9 | 4 | 3 | 1 | 0 | true |  |
| mixed-unicode | 186 | 117.2 | 117.1 | 4 | 1 | 1 | 0 | true |  |
| en-negative | 258 | 123.3 | 125.6 | 0 | 0 | 3 | 0 | true |  |
| ru-negative | 375 | 133.0 | 122.9 | 0 | 0 | 2 | 0 | true |  |
| en-long | 24512 | 7909.7 | — | 192 | 0 | 1 | 0 | true |  |
| ru-long | 48960 | 8738.1 | — | 128 | 64 | 64 | 0 | true |  |
| en-subwords | 5584 | 1703.5 | 1546.6 | 2 | 0 | 0 | 0 | true |  |
| ru-subwords | 10797 | 1248.1 | 1193.4 | 2 | 0 | 0 | 0 | true |  |

## multi-v2.1-q8 — threshold 0.3

Process success: true; profile: release; platform: macos-aarch64; machine: "Apple M4; 24 GiB; macOS 15.7.7; rustc 1.98.1".

Verify 183.3 ms; load 671.8 ms; drop 111.3 ms; peak RSS 1898.4 MiB; pinned setup 365454146 bytes.

| Language | Category | Gold | TP | Misses | FP | Precision | Recall |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| en | ADDRESS | 2 | 0 | 2 | 6 | 0.0% | 0.0% |
| en | ALL | 31 | 17 | 14 | 18 | 48.6% | 54.8% |
| en | BANK | 2 | 1 | 1 | 2 | 33.3% | 50.0% |
| en | EMAIL | 5 | 0 | 5 | 2 | 0.0% | 0.0% |
| en | IDENTITY | 2 | 0 | 2 | 0 | — | 0.0% |
| en | ORG | 3 | 3 | 0 | 1 | 75.0% | 100.0% |
| en | PERSON | 13 | 11 | 2 | 5 | 68.8% | 84.6% |
| en | PHONE | 2 | 2 | 0 | 1 | 66.7% | 100.0% |
| en | TAX | 2 | 0 | 2 | 1 | 0.0% | 0.0% |
| mixed | ADDRESS | 0 | 0 | 0 | 0 | — | — |
| mixed | ALL | 5 | 3 | 2 | 3 | 50.0% | 60.0% |
| mixed | BANK | 0 | 0 | 0 | 0 | — | — |
| mixed | EMAIL | 1 | 0 | 1 | 3 | 0.0% | 0.0% |
| mixed | IDENTITY | 0 | 0 | 0 | 0 | — | — |
| mixed | ORG | 1 | 1 | 0 | 0 | 100.0% | 100.0% |
| mixed | PERSON | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| mixed | PHONE | 1 | 0 | 1 | 0 | — | 0.0% |
| mixed | TAX | 0 | 0 | 0 | 0 | — | — |
| ru | ADDRESS | 2 | 0 | 2 | 2 | 0.0% | 0.0% |
| ru | ALL | 32 | 18 | 14 | 13 | 58.1% | 56.2% |
| ru | BANK | 3 | 2 | 1 | 5 | 28.6% | 66.7% |
| ru | EMAIL | 3 | 0 | 3 | 2 | 0.0% | 0.0% |
| ru | IDENTITY | 3 | 0 | 3 | 0 | — | 0.0% |
| ru | ORG | 4 | 3 | 1 | 1 | 75.0% | 75.0% |
| ru | PERSON | 12 | 11 | 1 | 1 | 91.7% | 91.7% |
| ru | PHONE | 2 | 1 | 1 | 0 | 100.0% | 50.0% |
| ru | TAX | 3 | 1 | 2 | 2 | 33.3% | 33.3% |

| Fixture | Bytes | First ms | Warm median ms | TP | Misses | FP | Invalid offsets | Deterministic | Error |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| en-contract | 268 | 34.5 | 33.5 | 4 | 2 | 4 | 0 | true |  |
| ru-contract | 509 | 44.4 | 41.5 | 4 | 2 | 1 | 0 | true |  |
| en-identifiers | 190 | 26.0 | 25.4 | 2 | 2 | 2 | 0 | true |  |
| ru-identifiers | 268 | 31.7 | 31.3 | 3 | 3 | 6 | 0 | true |  |
| en-variants | 129 | 19.6 | 19.1 | 3 | 2 | 0 | 0 | true |  |
| ru-variants | 334 | 25.9 | 24.7 | 4 | 1 | 0 | 0 | true |  |
| en-ocr | 272 | 37.5 | 36.4 | 3 | 5 | 7 | 0 | true |  |
| ru-ocr | 405 | 43.4 | 43.7 | 2 | 6 | 1 | 0 | true |  |
| en-markdown | 331 | 48.2 | 47.6 | 5 | 3 | 3 | 0 | true |  |
| ru-markdown | 355 | 42.5 | 42.1 | 5 | 2 | 4 | 0 | true |  |
| mixed-unicode | 186 | 23.4 | 22.7 | 3 | 2 | 3 | 0 | true |  |
| en-negative | 258 | 29.4 | 28.7 | 0 | 0 | 2 | 0 | true |  |
| ru-negative | 375 | 33.0 | 32.6 | 0 | 0 | 1 | 0 | true |  |
| en-long | 24512 | 2775.8 | — | 128 | 64 | 41 | 0 | true |  |
| ru-long | 48960 | 3781.9 | — | 83 | 109 | 81 | 0 | true |  |
| en-subwords | 5584 | 1136.0 | 1133.4 | 1 | 1 | 0 | 0 | true |  |
| ru-subwords | 10797 | 707.5 | 702.5 | 1 | 1 | 1 | 0 | true |  |

## multi-v2.1-q8 — threshold 0.5

Process success: true; profile: release; platform: macos-aarch64; machine: "Apple M4; 24 GiB; macOS 15.7.7; rustc 1.98.1".

Verify 216.9 ms; load 718.2 ms; drop 119.9 ms; peak RSS 1912.9 MiB; pinned setup 365454146 bytes.

| Language | Category | Gold | TP | Misses | FP | Precision | Recall |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| en | ADDRESS | 2 | 0 | 2 | 3 | 0.0% | 0.0% |
| en | ALL | 31 | 16 | 15 | 6 | 72.7% | 51.6% |
| en | BANK | 2 | 1 | 1 | 0 | 100.0% | 50.0% |
| en | EMAIL | 5 | 0 | 5 | 0 | — | 0.0% |
| en | IDENTITY | 2 | 0 | 2 | 0 | — | 0.0% |
| en | ORG | 3 | 2 | 1 | 1 | 66.7% | 66.7% |
| en | PERSON | 13 | 11 | 2 | 2 | 84.6% | 84.6% |
| en | PHONE | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| en | TAX | 2 | 0 | 2 | 0 | — | 0.0% |
| mixed | ADDRESS | 0 | 0 | 0 | 0 | — | — |
| mixed | ALL | 5 | 3 | 2 | 2 | 60.0% | 60.0% |
| mixed | BANK | 0 | 0 | 0 | 0 | — | — |
| mixed | EMAIL | 1 | 0 | 1 | 2 | 0.0% | 0.0% |
| mixed | IDENTITY | 0 | 0 | 0 | 0 | — | — |
| mixed | ORG | 1 | 1 | 0 | 0 | 100.0% | 100.0% |
| mixed | PERSON | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| mixed | PHONE | 1 | 0 | 1 | 0 | — | 0.0% |
| mixed | TAX | 0 | 0 | 0 | 0 | — | — |
| ru | ADDRESS | 2 | 0 | 2 | 0 | — | 0.0% |
| ru | ALL | 32 | 11 | 21 | 3 | 78.6% | 34.4% |
| ru | BANK | 3 | 1 | 2 | 2 | 33.3% | 33.3% |
| ru | EMAIL | 3 | 0 | 3 | 0 | — | 0.0% |
| ru | IDENTITY | 3 | 0 | 3 | 0 | — | 0.0% |
| ru | ORG | 4 | 1 | 3 | 0 | 100.0% | 25.0% |
| ru | PERSON | 12 | 9 | 3 | 0 | 100.0% | 75.0% |
| ru | PHONE | 2 | 0 | 2 | 0 | — | 0.0% |
| ru | TAX | 3 | 0 | 3 | 1 | 0.0% | 0.0% |

| Fixture | Bytes | First ms | Warm median ms | TP | Misses | FP | Invalid offsets | Deterministic | Error |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| en-contract | 268 | 35.0 | 31.7 | 4 | 2 | 2 | 0 | true |  |
| ru-contract | 509 | 42.5 | 41.0 | 2 | 4 | 0 | 0 | true |  |
| en-identifiers | 190 | 25.4 | 26.5 | 2 | 2 | 0 | 0 | true |  |
| ru-identifiers | 268 | 30.8 | 30.5 | 2 | 4 | 2 | 0 | true |  |
| en-variants | 129 | 19.0 | 18.7 | 3 | 2 | 0 | 0 | true |  |
| ru-variants | 334 | 25.4 | 24.8 | 3 | 2 | 0 | 0 | true |  |
| en-ocr | 272 | 37.1 | 36.5 | 2 | 6 | 1 | 0 | true |  |
| ru-ocr | 405 | 43.1 | 43.8 | 1 | 7 | 0 | 0 | true |  |
| en-markdown | 331 | 47.7 | 64.2 | 5 | 3 | 1 | 0 | true |  |
| ru-markdown | 355 | 42.0 | 42.1 | 3 | 4 | 1 | 0 | true |  |
| mixed-unicode | 186 | 23.4 | 22.7 | 3 | 2 | 2 | 0 | true |  |
| en-negative | 258 | 29.3 | 28.9 | 0 | 0 | 2 | 0 | true |  |
| ru-negative | 375 | 33.2 | 34.0 | 0 | 0 | 0 | 0 | true |  |
| en-long | 24512 | 2827.5 | — | 128 | 64 | 4 | 0 | true |  |
| ru-long | 48960 | 3291.2 | — | 74 | 118 | 34 | 0 | true |  |
| en-subwords | 5584 | 1135.0 | 1115.5 | 1 | 1 | 0 | 0 | true |  |
| ru-subwords | 10797 | 768.7 | 748.6 | 1 | 1 | 0 | 0 | true |  |

## pii-base-q8 — threshold 0.3

Process success: true; profile: release; platform: macos-aarch64; machine: "Apple M4; 24 GiB; macOS 15.7.7; rustc 1.98.1".

Verify 109.7 ms; load 328.6 ms; drop 82.4 ms; peak RSS 3451.7 MiB; pinned setup 205426204 bytes.

| Language | Category | Gold | TP | Misses | FP | Precision | Recall |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| en | ADDRESS | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| en | ALL | 31 | 25 | 6 | 6 | 80.6% | 80.6% |
| en | BANK | 2 | 1 | 1 | 3 | 25.0% | 50.0% |
| en | EMAIL | 5 | 3 | 2 | 0 | 100.0% | 60.0% |
| en | IDENTITY | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| en | ORG | 3 | 2 | 1 | 2 | 50.0% | 66.7% |
| en | PERSON | 13 | 11 | 2 | 0 | 100.0% | 84.6% |
| en | PHONE | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| en | TAX | 2 | 2 | 0 | 1 | 66.7% | 100.0% |
| mixed | ADDRESS | 0 | 0 | 0 | 0 | — | — |
| mixed | ALL | 5 | 4 | 1 | 1 | 80.0% | 80.0% |
| mixed | BANK | 0 | 0 | 0 | 0 | — | — |
| mixed | EMAIL | 1 | 1 | 0 | 0 | 100.0% | 100.0% |
| mixed | IDENTITY | 0 | 0 | 0 | 0 | — | — |
| mixed | ORG | 1 | 0 | 1 | 1 | 0.0% | 0.0% |
| mixed | PERSON | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| mixed | PHONE | 1 | 1 | 0 | 0 | 100.0% | 100.0% |
| mixed | TAX | 0 | 0 | 0 | 0 | — | — |
| ru | ADDRESS | 2 | 0 | 2 | 3 | 0.0% | 0.0% |
| ru | ALL | 32 | 16 | 16 | 17 | 48.5% | 50.0% |
| ru | BANK | 3 | 3 | 0 | 7 | 30.0% | 100.0% |
| ru | EMAIL | 3 | 2 | 1 | 1 | 66.7% | 66.7% |
| ru | IDENTITY | 3 | 0 | 3 | 1 | 0.0% | 0.0% |
| ru | ORG | 4 | 0 | 4 | 4 | 0.0% | 0.0% |
| ru | PERSON | 12 | 9 | 3 | 1 | 90.0% | 75.0% |
| ru | PHONE | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| ru | TAX | 3 | 0 | 3 | 0 | — | 0.0% |

| Fixture | Bytes | First ms | Warm median ms | TP | Misses | FP | Invalid offsets | Deterministic | Error |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| en-contract | 268 | 22.9 | 22.3 | 5 | 1 | 0 | 0 | true |  |
| ru-contract | 509 | 34.3 | 33.3 | 4 | 2 | 2 | 0 | true |  |
| en-identifiers | 190 | 16.6 | 16.4 | 4 | 0 | 2 | 0 | true |  |
| ru-identifiers | 268 | 27.4 | 34.2 | 3 | 3 | 4 | 0 | true |  |
| en-variants | 129 | 22.8 | 12.0 | 2 | 3 | 1 | 0 | true |  |
| ru-variants | 334 | 20.3 | 20.7 | 3 | 2 | 1 | 0 | true |  |
| en-ocr | 272 | 26.1 | 25.8 | 7 | 1 | 2 | 0 | true |  |
| ru-ocr | 405 | 33.3 | 33.5 | 2 | 6 | 7 | 0 | true |  |
| en-markdown | 331 | 35.0 | 34.8 | 7 | 1 | 0 | 0 | true |  |
| ru-markdown | 355 | 32.6 | 32.7 | 4 | 3 | 2 | 0 | true |  |
| mixed-unicode | 186 | 18.2 | 17.6 | 4 | 1 | 1 | 0 | true |  |
| en-negative | 258 | 21.7 | 21.3 | 0 | 0 | 1 | 0 | true |  |
| ru-negative | 375 | 27.0 | 26.6 | 0 | 0 | 1 | 0 | true |  |
| en-long | 24512 | 1759.4 | — | 143 | 49 | 3 | 0 | true |  |
| ru-long | 48960 | 3419.8 | — | 122 | 70 | 95 | 0 | true |  |
| en-subwords | 5584 | 436.3 | 425.8 | 2 | 0 | 0 | 0 | true |  |
| ru-subwords | 10797 | 1340.4 | 1367.6 | 1 | 1 | 1 | 0 | true |  |

## pii-base-q8 — threshold 0.5

Process success: true; profile: release; platform: macos-aarch64; machine: "Apple M4; 24 GiB; macOS 15.7.7; rustc 1.98.1".

Verify 112.4 ms; load 331.6 ms; drop 96.7 ms; peak RSS 2826.2 MiB; pinned setup 205426204 bytes.

| Language | Category | Gold | TP | Misses | FP | Precision | Recall |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| en | ADDRESS | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| en | ALL | 31 | 20 | 11 | 1 | 95.2% | 64.5% |
| en | BANK | 2 | 1 | 1 | 0 | 100.0% | 50.0% |
| en | EMAIL | 5 | 0 | 5 | 0 | — | 0.0% |
| en | IDENTITY | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| en | ORG | 3 | 2 | 1 | 1 | 66.7% | 66.7% |
| en | PERSON | 13 | 9 | 4 | 0 | 100.0% | 69.2% |
| en | PHONE | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| en | TAX | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| mixed | ADDRESS | 0 | 0 | 0 | 0 | — | — |
| mixed | ALL | 5 | 4 | 1 | 0 | 100.0% | 80.0% |
| mixed | BANK | 0 | 0 | 0 | 0 | — | — |
| mixed | EMAIL | 1 | 1 | 0 | 0 | 100.0% | 100.0% |
| mixed | IDENTITY | 0 | 0 | 0 | 0 | — | — |
| mixed | ORG | 1 | 0 | 1 | 0 | — | 0.0% |
| mixed | PERSON | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| mixed | PHONE | 1 | 1 | 0 | 0 | 100.0% | 100.0% |
| mixed | TAX | 0 | 0 | 0 | 0 | — | — |
| ru | ADDRESS | 2 | 0 | 2 | 0 | — | 0.0% |
| ru | ALL | 32 | 12 | 20 | 6 | 66.7% | 37.5% |
| ru | BANK | 3 | 2 | 1 | 1 | 66.7% | 66.7% |
| ru | EMAIL | 3 | 1 | 2 | 0 | 100.0% | 33.3% |
| ru | IDENTITY | 3 | 0 | 3 | 0 | — | 0.0% |
| ru | ORG | 4 | 0 | 4 | 4 | 0.0% | 0.0% |
| ru | PERSON | 12 | 7 | 5 | 1 | 87.5% | 58.3% |
| ru | PHONE | 2 | 2 | 0 | 0 | 100.0% | 100.0% |
| ru | TAX | 3 | 0 | 3 | 0 | — | 0.0% |

| Fixture | Bytes | First ms | Warm median ms | TP | Misses | FP | Invalid offsets | Deterministic | Error |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| en-contract | 268 | 24.7 | 22.5 | 5 | 1 | 0 | 0 | true |  |
| ru-contract | 509 | 33.9 | 33.0 | 2 | 4 | 1 | 0 | true |  |
| en-identifiers | 190 | 16.5 | 16.3 | 4 | 0 | 0 | 0 | true |  |
| ru-identifiers | 268 | 20.8 | 20.7 | 2 | 4 | 0 | 0 | true |  |
| en-variants | 129 | 12.2 | 12.1 | 2 | 3 | 0 | 0 | true |  |
| ru-variants | 334 | 20.1 | 20.2 | 3 | 2 | 1 | 0 | true |  |
| en-ocr | 272 | 25.9 | 26.0 | 6 | 2 | 0 | 0 | true |  |
| ru-ocr | 405 | 32.3 | 32.5 | 2 | 6 | 2 | 0 | true |  |
| en-markdown | 331 | 34.9 | 34.3 | 3 | 5 | 0 | 0 | true |  |
| ru-markdown | 355 | 32.1 | 32.4 | 3 | 4 | 2 | 0 | true |  |
| mixed-unicode | 186 | 17.7 | 17.4 | 4 | 1 | 0 | 0 | true |  |
| en-negative | 258 | 21.4 | 21.0 | 0 | 0 | 1 | 0 | true |  |
| ru-negative | 375 | 27.1 | 47.0 | 0 | 0 | 0 | 0 | true |  |
| en-long | 24512 | 1768.1 | — | 97 | 95 | 0 | 0 | true |  |
| ru-long | 48960 | 3418.1 | — | 97 | 95 | 74 | 0 | true |  |
| en-subwords | 5584 | 435.6 | 456.7 | 2 | 0 | 0 | 0 | true |  |
| ru-subwords | 10797 | 1404.0 | 1506.3 | 1 | 1 | 0 | 0 | true |  |
