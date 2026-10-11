<h1 align="center">
  <img src="resources/icons/icon.png" alt="mdoc" width="120px">
  <br>
  <span>mdoc</span>
</h1>
<p align="center">Prepare legal documents for AI, privately, on your own computer.</p>
<p align="center">
  <img alt="mdoc showing a scanned page of Brown v. Board of Education beside the Markdown recognized from it" width="100%" src="docs/images/pdf-scan-light.png">
</p>

mdoc turns PDFs (including scans), Word and other office documents into
editable Markdown, and replaces client names and other identifying details
with consistent aliases before you paste the text into ChatGPT or Claude.
Your documents never leave your computer. [Learn more](https://lawkitt.com/mdoc/).

<p align="center">
  <img alt="Animation: mdoc recognizes a scanned page, then proposes aliases for names, an email and a phone number in a services agreement and applies them" width="100%" src="docs/images/demo.webp">
</p>
<p align="center">
  <img alt="mdoc in the dark theme showing a Word contract as Markdown beside a preview of the original" width="100%" src="docs/images/docx-dark.png">
  <br>
  <sub>UK Model Services Contract, Crown copyright, licensed under the
  <a href="https://www.nationalarchives.gov.uk/doc/open-government-licence/version/3/">Open Government Licence v3.0</a>.</sub>
</p>

## Installation

<h3>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/apple-dark.svg">
    <img src="docs/images/apple-light.svg" width="24px" align="top" alt="">
  </picture>
  <span>macOS</span>
</h3>

Download `mdoc_<version>_arm64.dmg` from [Releases](https://github.com/lawkitt/mdoc/releases).
Requires a Mac with Apple silicon and macOS 11 or later.

<h3>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/windows-dark.svg">
    <img src="docs/images/windows-light.svg" width="24px" align="top" alt="">
  </picture>
  <span>Windows</span>
</h3>

Download `mdoc_<version>_x64-setup.exe` from [Releases](https://github.com/lawkitt/mdoc/releases).
Requires Windows x64.

The installer is not signed yet. If Microsoft Defender SmartScreen appears,
select **More info**, then **Run anyway**.

> [!WARNING]
> Pseudonymization finds most identifying details, not all of them. Review the
> whole document before you share it. See [privacy and limitations](docs/privacy.md).

#### License

<sup>
Licensed under the <a href="LICENSE">GNU General Public License, Version 3.0
or later</a>. Third-party licenses and attribution:
<a href="THIRD-PARTY-LICENSES.html">THIRD-PARTY-LICENSES.html</a>.
</sup>
