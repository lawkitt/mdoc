# App architecture and code ownership

Status: Accepted, 2026-10-25.

## Why

A feature change should touch one obvious place, and every state machine
should have an explicit owner; file size alone never justifies a split. New
workspace crates are cut by dependency weight, for a GUI-free boundary
(`mdoc-store`, `mdoc-import`, `mdoc-detect`), so the phones and tests reach
the logic without GPUI. In the app each owner has a folder and declares its
own actions. A state machine is a plain struct whose synchronous, GPUI-free
transitions return the owner's effect enum, and its GPUI view carries the
effects out. New state goes into an owner, not onto `Workspace` or
`DocumentView`. A document reaches its window only through `TabEvent`s and
holds no handle to it. A refactor changes no user-observable behaviour and
compares `just perf` and the headless renders before and after.

## Rejected

- **A service container, a trait hierarchy, dependency injection or a
  framework**: indirection that every feature change would have to cross.
- **Manager, Controller or Service owners**: an owner is named for what it
  owns.
- **A child GPUI entity per owner**: only a region that renders and takes
  focus on its own gets one.

## Revisit when

A feature change still needs edits across several owners.
