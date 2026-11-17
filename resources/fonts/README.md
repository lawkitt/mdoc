# DOCX preview fallback fonts

Metric-compatible fallbacks the DOCX preview renders with when a document's
font is not installed (`crates/mdoc-import/src/docx_preview/fonts.rs`).
Installed fonts always win: docxide reaches Carlito and Caladea only when
Calibri or Cambria is missing. The packages carry this folder: `fonts\` beside
`mdoc.exe` on Windows, `Contents/Resources/fonts` on macOS.

| Faces | Stands in for | Version | Licence | Source |
| --- | --- | --- | --- | --- |
| Carlito | Calibri | 1.104 | OFL-1.1, `Carlito-OFL.txt` | [google/fonts `ofl/carlito`](https://github.com/google/fonts/tree/7af0d90b4d532ffa4b041937b560c6df4e7b1e95/ofl/carlito) |
| Caladea | Cambria | 1.001 | OFL-1.1, `Caladea-OFL.txt` | [google/fonts `ofl/caladea`](https://github.com/google/fonts/tree/7af0d90b4d532ffa4b041937b560c6df4e7b1e95/ofl/caladea) |
| Liberation Sans, Serif, Mono | Arial, Times New Roman, Courier New | 2.1.5 | OFL-1.1, `Liberation-LICENSE.txt` | [liberation-fonts 2.1.5](https://github.com/liberationfonts/liberation-fonts/releases/tag/2.1.5) (`liberation-fonts-ttf-2.1.5.tar.gz`) |

Each family has Regular, Bold, Italic and BoldItalic, as published. None of
them draws ₽, so the currency-sign fallback (`glyph_fallback.rs`) stays.

SHA-256:

```text
ae3cb2dcbc925809dd29d2a44e9802211cab66be541bacbfc9c08c74b27c3742  Caladea-Bold.ttf
ccabaa7b7e2fdf253d2b1a5fa699dd8a3df8d835a9eb285ad82631a677eb76c0  Caladea-BoldItalic.ttf
4359a8e24f748b6447b1ff6d7a174febe70961d29f8bb8634b56dacd740a3deb  Caladea-Italic.ttf
f1e899278b7b4491aba5b6a8253c4b04c050cc59b21865be5c37559a775153cd  Caladea-Regular.ttf
bb5d20f79b82599ec72983597437373a80f2d2085fa91fc144fd74e876a594db  Carlito-Bold.ttf
b32928186c119599e03ca6a1ffc680fdcb7fac95772f4b95d989cf6cd3861517  Carlito-BoldItalic.ttf
0b019225e58d702bfedcbd35c21696769f8ee115cb6343f84c2f240312450d1c  Carlito-Italic.ttf
f6418f708baede9789daef5d458c0f53d2a888af9820e8062934e504fedc6595  Carlito-Regular.ttf
bd62a0672d0b9b6710b01df434c80ad54fa5f0835207eb7b17b7a761463067bb  LiberationMono-Bold.ttf
79451f3c09fe25116098853b7a2ca6e2436220ccc11af022979adbcf195be130  LiberationMono-BoldItalic.ttf
605c01c711b44480a7508d349dfbf3264e81fa43d69e61cfa7d10b86e764c4d1  LiberationMono-Italic.ttf
f2b83c763e8afd21709333370bed4774337fae82267937e2b5aea7e2fbd922c1  LiberationMono-Regular.ttf
788abee4c806d660e8aee46689dd8540cd4bb98da03dcc9d171ce3efd99a9173  LiberationSans-Bold.ttf
698da70fc191cc5f33ad4d6d3fe830fe4624b898ea2e3169955928b7c491f1ee  LiberationSans-BoldItalic.ttf
e5bae5c4cde31f22142753855f4f8fb86da6ff39955ed3c0a11248b0d16948b0  LiberationSans-Italic.ttf
76d04c18ea243f426b7de1f3ad208e927008f961dc5945e5aad352d0dfde8ee8  LiberationSans-Regular.ttf
d754ba427cfe0bca54ae052384baa8f842da5bd6550ad4da024ac441e7a7d5ce  LiberationSerif-Bold.ttf
f17db8af71e24d2066b587546021d4f0b296be389512b658dec3c09affeb11a7  LiberationSerif-BoldItalic.ttf
0e3dea9f8d613e006ccfa62201f33e265d19167bd0907725c3e145368b04fc2e  LiberationSerif-Italic.ttf
058ea80864aef09a23f45cbec2bb5400bc3dfbdea01c3f10538a21fcb497fb74  LiberationSerif-Regular.ttf
```
