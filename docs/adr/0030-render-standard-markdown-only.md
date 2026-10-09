# Render standard Markdown only

Status: accepted and implemented, 2026-10-09. See the
[codebase cleanup interview](../design/codebase-cleanup.md), round 2.

## Problem

The editor inherited note-taking syntax from Zorite: `[[wiki]]` and `#tag`
links, `key:: value` property pill panels, `((id))` block references and
`![[embed]]` lines. mdoc has no pages to link to: clicking a wiki or tag link
emits an event the app ignores. Converted PDF/DOCX Markdown never produces this
syntax, so in legal text a match is more likely accidental than intended.

## Decision

Stop recognizing and rendering these constructs; they display as ordinary
Markdown text. Remove the recognizers, rendering, hit-testing and events
(`OpenWikiLink`, `LinkHit::Page`/`BlockRef`) that serve only them.

Standard links, images, GitHub alerts, tables, task lists, `==highlights==`
and `<mark>`/`<span>` styled tags are unaffected.

## Invariants

Saved bytes and Copy Markdown output are unchanged: this affects display only.
Search, pseudonymization syntax protection and caret mapping treat the former
constructs as plain text. Record the change in CHANGELOG.
