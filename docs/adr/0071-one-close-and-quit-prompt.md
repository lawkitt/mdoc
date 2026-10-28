# One close and quit prompt

Status: Accepted, 2026-10-27. Amends [ADR 0041](0041-cases-and-ai-response-restoration.md)
("The unsaved-mapping question is asked on tab close, not on quit").

## Problem

Closing a pseudonymized tab that has unsaved changes asks twice: "Keep the
replacement mapping?" (Save as Case, Close, Cancel), then "Save changes?"
(Save, Discard, Cancel). Quitting asks only about unsaved changes and drops
every unsaved mapping without a word
([#119](https://github.com/lawkitt/mdoc/issues/119)). `session.json` stores
paths only, so after a quit the saved file keeps its aliases and nothing can
restore AI replies to it.

## Decision

- **At-risk tab.** A tab is at risk when it has unsaved changes, an unsaved
  mapping (applied aliases and no case, or a case whose save failed), or
  both. The mapping half counts only while Settings → Saved cases → "Ask
  before closing an unsaved mapping" is on; one switch covers close and quit.
  While quitting, a case whose save failed is not asked about here: the
  quit's final case saves already offer Retry for it.
- **One prompt per tab.** A tab with both: "Save “<tab title>” and keep its
  mapping?" with Save as Case (default, Enter: saves the file, then creates
  the case), Save (saves the file, forgets the mapping), Don't Save (⌘D /
  Alt+D: drops both) and Cancel (Esc). A tab with only one keeps today's
  prompt for it. All use the shared in-app confirm dialog
  ([ADR 0064](0064-dialog-and-role-semantics.md)).
- **Quit asks about mappings too.** With one at-risk tab, quit shows that
  tab's prompt. With two or more, a summary comes first: "<N> documents have
  unsaved changes or mappings" with Review… (default: each at-risk tab's
  prompt in turn, sidebar order), Discard All and Cancel. Cancel at any step
  cancels the quit, and so does Not now in the Keychain explainer when Save as
  Case needs it. Discard All asks no tab again; a failed save of an existing
  case still offers Retry, since that mapping was meant to be kept.
- **No mapping survives quit without a case.** Creating a case stays the
  consent to store originals (ADR 0041); unsaved mappings are not written to
  the session.
- **Untitled tabs are numbered.** The first new tab is `Untitled.md`, the next
  ones take the lowest free number (`Untitled 2.md`, `Untitled 3.md`), counted
  across open tabs, restored tabs included. A closed tab frees its number. A
  restored unsaved tab keeps its name, and Save As suggests it.
- **Save as Case on a never-saved document** opens Save As first, as today
  (#75); cancelling it cancels the close or quit.

## Consequences

- Done when #119 closes: ⌘W on a dirty, pseudonymized tab asks once; ⌘Q with
  an unsaved mapping asks; Discard All quits without asking each tab.
- Untitled tabs have distinct names in the sidebar, the prompts and
  VoiceOver (roadmap 0.3.0 item 1).
- The acceptance checklist's close and quit rows gain the combined prompt and
  the summary.
