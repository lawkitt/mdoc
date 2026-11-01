---
status: known-flaw
flaw: digit-false-hit
issue: 375
kind: contract
source: import
expect: false hit: PHONE "123456789012345678901234567890"
expect: false hit: PHONE "1234567890123456789012345"
---
Reference 123456789012345678901234567890. Invoice +1234567890123456789012345. Date 2026-10-07, section 12.3.4.
