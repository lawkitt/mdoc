# Old case file fixture

`0123456789abcdef0123456789abcdef.case` is a saved case written by an
earlier mdoc with aes-gcm 0.10.3 under the test key `[7; 32]`: `MDOCCASE1`,
a 12-byte nonce, then AES-256-GCM over the case JSON, authenticated with the
case id.

Builds with the Vault no longer open saved cases, and never touch the old
cases folder (#1091 Decision 2). The headless runner
(`apps/desktop/src/workspace/headless_renders/vault/old_cases.rs`) puts this file in its
sandbox profile's old cases folder and fails if any code path changes it.
Never regenerate it.
