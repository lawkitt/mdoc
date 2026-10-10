# mdoc

A local document-preparation app for lawyers: convert PDF (including scans),
Word, Excel, PowerPoint, OpenDocument, RTF and other formats into editable
Markdown for AI tools such as ChatGPT or Claude. Built with Rust and GPUI.

Review and edit Markdown alongside the original PDF or approximate DOCX preview,
then save or **Copy Markdown** to hand it off. Copy includes the complete current
source, including unsaved edits. Documents remain ordinary local files.

**Pseudonymize** scans locally and proposes consistent aliases for identifying
information. Review and correct the proposals, then explicitly apply replacements.
Applied aliases remain inspectable and undoable while the document is open;
originals and mappings are not saved or copied. The GLiNER2 FP16/FP32 detector
has known English/Russian and hidden-source misses, so review the result before
sharing. It does not guarantee anonymization; see the [qualification results](tests/fixtures/pseudonymization/README.md).

OCR and automatic pseudonymization are available on Apple Silicon macOS and
Windows x64, the two platforms with releases. Intel macOS, Windows ARM64 and
Linux build from source, unsupported, with native-text import only.
Model downloads require an explicit choice; subsequent processing runs offline.
Windows runtime and native acceptance checks remain outstanding. Image-only DOCX
has no app OCR path. See [OCR limitations](tests/fixtures/ocr-qualification/README.md)
and [model verification](tests/fixtures/model-settings/README.md).

## Install

Download the latest build from [Releases](https://github.com/lawkitt/mdoc/releases):
`mdoc_<version>_arm64.dmg` for Apple Silicon Macs, or
`mdoc_<version>_x64-setup.exe` for Windows x64 (stable versions also have an
`.msi` for managed deployment). Builds are not signed yet: on macOS, right-click
mdoc → **Open** on first launch; on Windows, choose **More info → Run anyway**.
Check a download against `SHA256SUMS`, or verify where it was built with
`gh attestation verify <file> -R lawkitt/mdoc`.

The Windows installer also installs the Microsoft Visual C++ Redistributable
(x64) when it is missing; OCR and pseudonymization need it. The `.msi` requires
it to be installed first.

## Run from source

All dependencies, including the pinned AnyDoc converter, are public.

```sh
git clone https://github.com/lawkitt/mdoc.git
cd mdoc
cargo run
# Or open a document:
cargo run -- path/to/document.pdf
```

Markdown and DOCX paths work the same way. **Open** also accepts multiple files;
each converts when activated. PDF pages needing OCR wait for your explicit choice.
Save writes Markdown and preserves the original source.

## Development

Development commands are [just](https://just.systems) recipes; CI runs the
same ones. `just setup` installs the pinned tools, `just doctor` checks them.

```sh
just check      # fmt, cargo-deny, clippy and tests, as CI runs them
just check-all  # plus the OCR and installed-model smoke tests (downloads)
```

See [development notes](docs/development.md) for the full recipe list,
ownership, dependency updates, fixtures, performance probes, build-cache
cleanup, packaging and releases.
Headless checks do not establish native appearance, IME, accessibility or
cross-platform runtime acceptance.

## Project documentation

- [Roadmap](ROADMAP.md): current status, next steps and deferred work.
- [Glossary](CONTEXT.md): product and code terminology.
- [Architecture decisions](docs/adr/): accepted behavior and its rationale.
- [Evidence](docs/evidence/): historical measurements and verification records.
- [Production readiness](docs/design/pseudonymization-production-readiness.md): open qualification gates.

Completed design interviews are retained in git history; ADRs are the durable
records. The [mdoc website](https://lawkitt.com/tools/mdoc/) provides the product overview.

## License and attribution

mdoc is derived from [Zorite](https://github.com/packetThrower/zorite).
The app remains [GPL-3.0-or-later](LICENSE); internal workspace crates retain
their MIT licenses and original copyright notices. See [NOTICE.md](NOTICE.md)
for provenance and [THIRD-PARTY-LICENSES.html](THIRD-PARTY-LICENSES.html) for
dependency licenses.
