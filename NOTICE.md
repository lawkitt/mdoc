# License and attribution

mdoc is a modified version of [Zorite](https://github.com/packetThrower/zorite),
originally developed by packetThrower (Will Lehnertz) and contributors.

Copyright © 2026 packetThrower / Zorite contributors.
Copyright © 2026 lawkitt / mdoc contributors for modifications.

The application is licensed under the GNU General Public License, version 3
or (at your option) any later version. The complete license is in [LICENSE](LICENSE).
It is distributed without warranty, as described in that license.

The reusable crates in `crates/` retain their MIT licenses, including the
original copyright notices in each crate's `LICENSE` file. Renaming
`zorite-editor` and `zorite-markdown` to `mdoc-editor` and `mdoc-markdown`
does not change their licenses or upstream authorship.

mdoc removes the original notebook/database model and focuses on local Markdown
files, a PDF pane, and document import. Its Git history starts independently;
that does not remove upstream attribution.

Dependency licenses are collected in [THIRD-PARTY-LICENSES.html](THIRD-PARTY-LICENSES.html).
Regenerate that file with `cargo about generate about.hbs -o THIRD-PARTY-LICENSES.html`.
Imported test fixtures retain their separate notices in
[tests/fixtures/import/README.md](tests/fixtures/import/README.md) and
[LICENSE.anydoc](tests/fixtures/import/LICENSE.anydoc).
