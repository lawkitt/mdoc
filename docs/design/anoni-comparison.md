# Anoni comparison and default key storage — design interview

Status: complete, 2026-10-14. Accepted decisions are in
[ADR 0046](../adr/0046-workspace-polish-from-anoni-review.md),
[ADR 0047](../adr/0047-crash-reports.md) and an amendment to
[ADR 0045](../adr/0045-prompt-free-case-key.md).

## Request

Study Anoni (anoni.dev, 0.81.1, macOS) and decide what mdoc borrows or does
better. Consider Anoni's key handling as mdoc's default, with Keychain as an
opt-in that asks for access only when chosen. Fix the "Detect in …" popup bug.

## Findings

### Anoni key storage (inspected on disk, 2026-10-14)

App data: `~/Library/Application Support/fr.anoni.app/`.

- `.recents-key`: 64 hex chars (a random 256-bit key) in a **plaintext file**,
  mode 0600, next to the data it protects (`recents.enc`, base64 ciphertext).
  The binary also names `.recents-key.unreadable` (a fallback rename when the
  file cannot be read) and `recents-cache-key-v1`.
- `custom-terms.enc` is encrypted the same way.
- No Anoni item in the login keychain (`security find-generic-password -s
  fr.anoni.app` finds nothing), though the `keyring` crate is linked.
- Optional passphrase **vault**: Tauri commands `open_vault`/`seal_vault`
  (`passphrase`), Argon2 parameters, errors "wrong passphrase or corrupted
  vault", `passphrase_required`, `install_key_mismatch`.
- `.anon-token`, `.device-id` are plaintext 0600 files too.
- So: no system prompts ever; protection at rest equals "file permissions of
  the user account". Anyone or anything that can read the user's files (backup,
  sync, malware as the user) gets key and ciphertext together.

### Anoni UI (screenshots + live app)

- Home: greeting, big drop zone, Paste text / Multiple files, **"100% local"**
  badge with explainer, Recent list with status ("anonymized").
- Crash banner with **"See what would be sent"**: shows the report with paths
  replaced by `<chemin>` before **Send**.
- Document flow is a 3-step stepper **Drop → Check → Export**.
- Check: per-category coloured underlines in the text; right panel "To mask"
  grouped by category with counts (`ORGANIZATION 7 (20)` = entities (mentions)),
  checkbox per entity, category change dropdown, jump-to arrow;
  **Re-identification risk** indicator; Continue (⌘↩).
- Export: **Pseudonymize** (Tokens | Realistic) vs **Redact** (irreversible);
  floating pill **Before / After / Side by side / Reply** + zoom; inline **AI
  reply** box with "5 values restored · 1 unrecognised token", Copy/Save reply,
  **Share the key…**; Pseudonymized Word/PDF, Copy for AI, **Compliance report**.
- Tools: Decode (reply from a document or pasted text + "the key", masked,
  Change key), Watermark, PDF → Word. Pro: Journal, Watched folders.
- Weaknesses seen: token style is inconsistent in its own reply (`ORGANIZATION·1`,
  `ORGANIZATION 1`, `organization·1`); detection of a template finds boilerplate
  ("Lender", "Borrower", arbitration bodies) as organisations, joined place
  strings ("New York ☒ United King…"); 29 "items" in an empty template; a Python
  sidecar on 127.0.0.1:8765 that crashed (`RecvError`).

### mdoc issues seen in the owner's screenshots

- "Detect in “<case>”" popup (`src/workspace_cases.rs:1341`): the title is a
  `flex_1` child without `min_w_0`/truncation, so a long name without spaces
  overflows the 320 px panel and pushes × outside it.
- Table cells show literal `<br>` from DOCX conversion.
- Panel footer shows "In “case”, not in this document (0)" when the count is 0.

## Decisions

### Round 1 (2026-10-14)

1. **Key storage: no plaintext key file.** Signed builds move to the data
   protection keychain (ADR 0045 step 3) once the owner creates the
   provisioning profile; it never prompts and survives updates. Anoni's
   key-next-to-ciphertext scheme is rejected: it removes prompts only by
   removing the protection. ADR 0041's "no plaintext fallback" stands.
2. **Popup title:** "Categories to detect" (the menu item's wording) as the
   title, the case name as a muted, single-line, ellipsized second line, × pinned
   right. Check "Move to case “…”" menu rows for the same overflow.
3. **Adopt from Anoni:** restore summary after Paste AI Response; category
   grouping with counts in the replacements panel; "see what would be sent"
   before any report; a "100% local" note on the empty page; a floating view
   switcher. **Not adopted:** per-category colours (not chosen), realistic
   surrogates (collide with real text and lenient restore), re-identification
   risk score, compliance report, Drop → Check → Export stepper (fights the
   editor-first workspace, ADR 0020).
4. **Small fixes:** `<br>` inside a table cell renders as a line break (narrow
   exception to ADR 0030); the "In “case”, not in this document" footer is
   hidden when its count is 0.

### Round 2 (2026-10-14)

5. **Data protection keychain is the beta.9 release blocker.** The code and
   signing pipeline are in place (`scripts/macos-sign.sh`,
   `scripts/ci-apple-credentials.sh`, `release.yml`); only the owner's Apple
   setup is missing (checklist below). Until then release builds keep the login
   keychain with the ADR 0043 explainer; existing users see one dialog on their
   next Unlock, which migrates the key.
6. **Optional case passphrase: deferred** to the roadmap. With the data
   protection keychain it only adds protection against malware running as the
   user, at the cost of forgotten-passphrase and per-launch unlock UX.
7. **Case sharing ("Share the key…"): deferred** to the roadmap; needs an
   export format, a passphrase for the file, and import/merge rules.
8. **Restore summary.** The tab made by Paste AI Response shows a notice bar:
   "Restored N names · M unrecognised tokens" with **Next unrecognised**
   (jumps to and selects the next one) and ×. With M = 0: "Restored N names",
   fading on its own. With N = 0 and M = 0: "No placeholders found in this
   reply." Counts are a snapshot taken at paste time.
9. **Category grouping.** The replacements panel always groups rows by
   category under headers "Category names (mentions)", e.g. `Person 3 (7)`.
   Fixed category order, empty groups hidden, rows by first appearance inside
   a group. Headers are not collapsible and not focusable: arrow keys skip
   them. Filter and search apply within groups. No toggle.
10. **Crash reports** (own ADR). A panic hook writes a local report (version,
    OS, backtrace, recent log lines). The next launch shows "mdoc quit
    unexpectedly" with **See what would be sent** / Dismiss. The preview masks
    the home folder and file paths and never contains document text. **Send**
    opens a prefilled GitHub issue; copy to clipboard as a fallback. No server
    endpoint, nothing sent automatically.
11. **Local note.** Empty page: lock icon + "Documents stay on this Mac"
    ("this computer" on Windows/Linux). Popover: "Detection, conversion and
    saved cases run on this Mac. mdoc goes online only to check for updates and
    to download models you choose."
12. **View switcher.** A floating segmented pill, bottom centre of the document,
    shown only while replacements are applied: **Edit · Original ·
    Pseudonymized · Side by side**. Original and Pseudonymized are single-column
    read-only views; Side by side is the ADR 0042 view. The toolbar icon and ⌘\
    stay and toggle Edit ↔ Side by side. No Reply mode (a restored reply is
    its own tab), no zoom.

## Owner checklist: data protection keychain

1. developer.apple.com → Certificates, IDs & Profiles → Identifiers → **+** →
   App IDs → App. Bundle ID (explicit) `io.github.lawkitt.mdoc`, team
   F33DLM8K53. Capabilities: **Keychain Sharing**. (If the ID exists, edit it
   and enable Keychain Sharing.)
2. Profiles → **+** → Distribution → **Developer ID**. App ID
   `io.github.lawkitt.mdoc`, certificate: the Developer ID Application
   certificate used by CI. Name it `mdoc Developer ID`; download.
3. Check it: `security cms -D -i mdoc_Developer_ID.provisionprofile` must list
   `keychain-access-groups` containing `F33DLM8K53.io.github.lawkitt.mdoc` or
   `F33DLM8K53.*`.
4. `base64 -i mdoc_Developer_ID.provisionprofile | pbcopy`, then GitHub →
   lawkitt/mdoc → Settings → Secrets → Actions → **New secret**
   `APPLE_PROVISIONING_PROFILE_BASE64`.
5. Release beta.9. The signing log must not print "no
   APPLE_PROVISIONING_PROFILE". Then the ADR 0045 verification: Unlock → one
   dialog → relaunch → no locked bar, no dialog.

### Round 3 (2026-10-14)

13. ADR 0046 (workspace polish), ADR 0047 (crash reports), an amendment note in
    ADR 0045; passphrase and case sharing on the ROADMAP deferred list.
14. Order: bug fixes; restore summary; category grouping; local note; view
    switcher; crash reports. One commit each on a branch, one PR, no release.

Found while implementing: the restore summary bar already existed (live
unrecognized count, Next unrecognized, ×). Only its zero case changes; it does
not fade, because it carries the apply-before-copying instruction.
