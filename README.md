<h1 align="center">
  <img src="build/appicon.svg" alt="mdoc" width="120px">
  <br>
  <span>mdoc</span>
</h1>
<p align="center"><b>Use AI on client documents without sending client names.</b></p>
<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/hero-dark.webp">
    <img alt="mdoc finds the people, companies, addresses and bank details in a fictional letter before claim, replaces them with aliases such as PERSON_1 and ORG_1, and restores the real names in the AI's reply" width="100%" src="docs/images/hero-light.webp">
  </picture>
</p>

mdoc reads scanned PDFs, Word and other office documents, swaps names and other
identifying details for consistent aliases, and puts the real names back into
the AI's reply. Everything runs on your computer. Nothing is uploaded.
[Learn more](https://lawkitt.com/mdoc/).

**Reads scans.** Recognition runs locally, with the original page beside the text.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/ocr-dark.png">
    <img alt="The first page of Brown v. Board of Education, a scanned PDF, recognized into editable Markdown beside the original page" width="100%" src="docs/images/ocr-light.png">
  </picture>
</p>

**You review every replacement.** People, companies, emails, phone numbers,
addresses and bank details are proposed with consistent aliases; untick what
should stay and add anything detection missed.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/review-dark.png">
    <img alt="The Review panel listing 14 identities in a fictional letter before claim, each with its alias, and the matching mentions underlined in the text" width="100%" src="docs/images/review-light.png">
  </picture>
</p>

**The reply comes back with real names.** Copy for AI, paste the answer back,
and mdoc restores the originals in a new tab.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/restore-dark.png">
    <img alt="An AI reply pasted back into mdoc, with PERSON_1, ORG_2 and the other aliases restored to the real names" width="100%" src="docs/images/restore-light.png">
  </picture>
  <br>
  <sub>Fictional letter and reply. Captured in mdoc 0.1.0-beta.11.</sub>
</p>

[Watch the films](https://lawkitt.com/mdoc/#demo-title) ·
[More screenshots and how they are made](docs/product-media/README.md)

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
