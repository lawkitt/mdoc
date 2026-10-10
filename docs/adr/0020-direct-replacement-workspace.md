# Direct replacement workspace

Status: Accepted and implemented, 2026-10-08. The user confirmed the settled
contract and explicitly authorized implementation. See the
[historical design interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/pseudonymization-direct-interaction.md).

## Problem

The current Replacements panel and document popup duplicate editors. Routine
linking is hidden behind menus, entering an existing alias produces a collision
error without a direct resolution, and selecting a panel item does not retain
an open contextual popup at a distinctly highlighted word.

## Agreed direction

The panel and popup appear together and complement each other as one workspace.
The panel remains a searchable overview; the word popup is the routine editor.
Both panel and document clicks reveal the active occurrence and open the same
controls. Specific mention clicks target that occurrence; group clicks remember
the last valid occurrence, otherwise starting at the first in document order.
Provide compact previous/next navigation and a distinct active-word highlight.

Using the same alias means the mentions refer to one entity. "Belongs to" remains
a separate secondary ownership relationship with separate aliases. Linking
defaults to all occurrences of the selected original wording with a visible count
and a direct single-mention alternative. Joining other original variants requires
a deliberate broader operation.

Remove Save alias. Selecting an existing alias or confirming a new alias with
Enter records the mapping. Pending originals await explicit batch Apply; applied
tracked corrections update text immediately through undoable operations. Typing
alone remains an unconfirmed draft.

The single alias chooser searches aliases and original variants, offers likely
existing entities directly with original names, and supports new aliases inline.
Entering an existing alias offers a deliberate linking choice rather than a dead
collision error. A new alias affects the selected scope and separates mentions
when needed; an explicit secondary action renames the whole entity. If selected
scope covers the entire entity, simply rename it. Show affected counts.

The active panel row expands inline to readable original variants and mention
context, emphasizing the relevant word and omitting Markdown delimiters from
excerpts only. No detail-page navigation remains. Deliberate cross-category
linking adopts the destination category for affected assignments, while mentions
left in the original group retain their category.

Keep/Restore state their own scope: this mention plus a direct matching-value
alternative when relevant. A multi-mention linking selection never silently
broadens restoration; restoration follows recorded prior values.

Category choices and secondary identity-wide ownership selection appear inline
in the popup. The overview stays vertical at the right edge, including narrow
windows, with the popup in the document area beside it. Both surfaces remain
available with the active word unobscured.
Hidden-source occurrences show their containing passage and labelled exact value;
they do not claim a different rendered word is the occurrence.

Opening uses neutral popup focus. Escape dismisses suggestions, then cancels a
draft, then closes the popup while retaining the panel. Selecting another item
switches the popup directly and discards unconfirmed typing without a dialog.
Enter or the inline alias choice confirms a draft. Apply may confirm a valid
visible new-alias draft and apply proposals in one undoable operation; an existing
alias still needs explicit target selection. Invalid input is explained beside
the field. Closing the workspace closes both surfaces.

Linking, renaming and applying preserve the active occurrence and popup. After
application, a compact applied count and Undo replace the disabled Apply button;
the experimental caution remains. Keep/Restore closes a popup whose replacement
was removed while retaining panel and document position.

Same-wording scope includes only matching originals currently assigned to the
active entity, respecting separated homonyms and Keep exclusions. Offer automatic
inline new-alias creation with the next available token alongside custom input.
Direct scope choices cover this mention, same wording and (when broader) the
entire entity with actual affected counts. Category correction retains custom
aliases, replaces generated wrong-category tokens, previews the result and keeps
unrelated aliases stable.

This direction revises ADR 0019's selected-item panel editor and contextual-menu
placement. Stable aliases, occurrence provenance, live-tab originals, undo/redo,
Keep and restoration scope, syntax protection, stale-result guards, exact raw
Copy Markdown and experimental status remain the baseline. No detector expansion
or persistent mapping storage is selected.

## Acceptance and confirmation

The linked interview contains the consolidated interaction and acceptance
contract: live word/popup/panel QA in both themes at wide and narrow sizes;
alias/assignment/ownership/Keep/Restore scopes; drafts and focus; mixed
pending/applied text; stale guards, one-step undo/redo and exact Copy Markdown;
and existing virtualization/provenance/lifetime safeguards. Validation must state
remaining platform, IME/accessibility and detector-quality boundaries.

The user confirmed the empty decision frontier and explicitly requested
implementation. The accepted interaction is implemented. Verification and
remaining platform/tool boundaries are recorded in the linked design document.


## User refinement — 2026-10-08

The user requested a vertical panel to reduce scrolling. This supersedes the
compact-bottom placement selected in Q12. The overview now uses the full document
area height, compact 48 px rows and an adaptive 260–340 px width. The document and
Original preview share the remaining width; their existing pane switching applies
when that area becomes narrow. Selecting an occurrence reveals Markdown when
Original is selected. Alias/popup, mapping, scope and history behavior are unchanged.
