# Outside sets

Public document sets from outside mdoc, used to benchmark and test it
([decisions: lawkitt/mdoc#916](https://github.com/lawkitt/mdoc/issues/916)).
The manifests and the fetch script are a roadmap item; this checklist
already applies.

## The record

Per set, `<name>.toml`: source URL, pinned revision (commit, Hugging Face
sha, or Zenodo record and version), licence, class (`real`, `synthetic`,
`specimen`), languages, role (`tune`, `held-out`, `regression`) and location
(`repo`, `holdout`, `on-demand`). `<name>.sha256` lists each file's digest.
The fetch script verifies every digest into `~/.cache/mdoc-sets/<name>@<rev>`.
A refresh is a PR that bumps the revision and lock and posts the baseline
delta. A private set keeps its manifest inside `lawkitt/mdoc-holdout`, so its
file names stay out of this tree.

## Adding a set

1. **Licence** recorded. Only a permissive licence allows files in this
   repository (GPL-3.0-or-later), with its licence and attribution beside
   them. Share-alike, none or restricted: on demand or holdout only.
2. **Class** recorded. Real documents with personal data go to the holdout
   only, and only aggregates are posted.
3. **Languages, kinds, scans and ground truth** recorded.
4. **Role** declared before the first run. A held-out set never chooses a
   threshold, prompt or rule; reports flag public sets as possibly in a
   model's training data.
5. **Manifest and lock** committed, and the digests verify.
6. **Maintainer's OK first** for a download over 1 GB, a push to the holdout,
   or using a set without a licence.
7. **ID and passport images**, specimens included, are never posted.
8. **No hosted runner time**: runs happen on fleet machines.
