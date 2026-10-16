# Case file fixture

`0123456789abcdef0123456789abcdef.case` is a saved case
([ADR 0041](../../../docs/adr/0041-cases-and-ai-response-restoration.md)) written
by mdoc with aes-gcm 0.10.3 under the test key `[7; 32]`: `MDOCCASE1`, a 12-byte
nonce, then AES-256-GCM over the case JSON, authenticated with the case id. Its
contents are `fixture_case()` in `src/cases.rs`.

`a_case_written_by_aes_gcm_0_10_still_opens` proves that existing users' case
files still decrypt after a crypto dependency bump. Never regenerate it with a
newer aes-gcm; add another fixture instead.
