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
`zorite-editor` and `zorite-markdown` to `mdoc-editor` and `mdoc-markdown`,
and later merging the retained `mdoc-markdown` recognition code into
`mdoc-editor`, does not change their licenses or upstream authorship.

mdoc removes the original notebook/database model and focuses on local Markdown
files, a PDF pane, and document import. Its Git history starts independently;
that does not remove upstream attribution.

Dependency licenses are collected in [THIRD-PARTY-LICENSES.html](THIRD-PARTY-LICENSES.html).
Regenerate that file with `cargo about generate about.hbs -o THIRD-PARTY-LICENSES.html`.
The temporary [vendored GPUI Linux renderer](vendor/gpui-pre-wgpu/ZORITE-PATCH.md)
retains Zed's Apache-2.0 license and Zorite's combining-mark correction. Its
upstream identity, patch and removal condition are recorded beside the source.
Imported test fixtures retain their separate notices in
[tests/fixtures/import/README.md](tests/fixtures/import/README.md) and
[LICENSE.anydoc](tests/fixtures/import/LICENSE.anydoc).

Local OCR uses the MIT-licensed pdf-inspector fork at
<https://github.com/lawkitt/pdf-inspector>, OAR/PaddleOCR models under Apache-2.0,
and separately downloaded PDFium and ONNX Runtime libraries. Setup installs
the runtime licenses and third-party notices beside the libraries under the
application's `mdoc/ocr/v1/licenses` directory. Model provenance and qualification
are recorded in [the OCR fixture notes](tests/fixtures/ocr-qualification/README.md).
Pinned runtime identities/digests live in [src/ocr.rs](src/ocr.rs); model identities
live in the pinned pdf-inspector manifest referenced by Cargo.toml and Cargo.lock.
The synthetic English/Russian OCR qualification fixtures were created for mdoc;
their text and raster PDFs may be used under CC0-1.0.

Spellcheck bundles unmodified Hunspell dictionaries from LibreOffice: en_US
(SCOWL by Kevin Atkinson; affix file by Geoff Kuenning, BSD-style), en_GB (LGPL;
David Bartlett, Brian Kelk, Andrew Brown and Marco A.G. Pinto) and ru_RU (BSD-style,
© 1997–2008 Alexander I. Lebedev). Their notices, provenance and digests are in
[crates/mdoc-spell/dictionaries/](crates/mdoc-spell/dictionaries/README.md);
mdoc's legal word lists there are separate supplements. The engine is spellbook
(MPL-2.0), listed with the other dependencies.

Experimental inline pseudonymization uses gliner2-rs 0.9.6 by Dario Finardi,
published by Jugaad s.r.l. (Apache-2.0). Its license and NOTICE are retained in
[resources/gliner2-license.txt](resources/gliner2-license.txt) and
[resources/gliner2-notice.txt](resources/gliner2-notice.txt). Optional setup
installs the pinned Jugaad/Fastino GLiNER2 privacy PII model (Apache-2.0), its
model card, lineage attribution and Microsoft MIT terms alongside the model;
shared ONNX Runtime notices remain beside the runtime. Model identities/digests
live in the pinned [FP16 manifest](resources/pseudonymization-model.json) and
[FP32 manifest](resources/pseudonymization-fp32-model.json), at the same export revision.
No model weights are included in source or default packaging.

The structured PII adapter uses presidio-analyzer 0.1.11, a Rust Presidio port
by the presidio-rust contributors (MIT). Its complete license is retained in
[resources/presidio-license.txt](resources/presidio-license.txt). Only selected
email/INN/SNILS pattern configuration and the SNILS checksum validator are used;
mdoc retains its Unicode patterns, strict context and boundary policy, INN
checksum, review decisions and replacement/provenance implementation. Optional
gazetteer and ONNX features are disabled. See the
[reuse decision](docs/adr/0017-selected-rust-presidio-reuse.md), including its
historical research and verification reference.
