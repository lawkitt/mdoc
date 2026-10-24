# Model attribution pins

Download consent and Settings read source and license from the app's model
manifests. OCR's app manifest wraps pdf-inspector's artifact manifest so URLs,
sizes and digests still have one owner. Recheck attribution when updating a pin.

- Both GLiNER precisions: the [export model card at
  e5948986](https://huggingface.co/jugaadsrl/gliner2-privacy-filter-PII-multi-onnx/blob/e594898629d452e8311796f5f329c7edbeda907c/README.md)
  declares Apache-2.0 and identifies Fastino's base model. The [base card at
  1cb41660](https://huggingface.co/fastino/gliner2-privacy-filter-PII-multi/blob/1cb4166094dc58fa8d836429f060d6c95f62b495/README.md)
  also declares Apache-2.0. The existing encoder MIT notice is retained;
  Microsoft's [mDeBERTa card at
  a0484667](https://huggingface.co/microsoft/mdeberta-v3-base/blob/a0484667b22365f84929a935b5e50a51f71f159d/README.md)
  declares MIT.
- PP-OCRv6 Small: GreatV/oar-ocr's [v0.7.0 LICENSE at its release
  commit](https://github.com/GreatV/oar-ocr/blob/4e350472b058ef78a640634bc9095fc3836c3e34/LICENSE)
  is Apache-2.0.
- English & Russian OCR: the same v0.7.0 detector plus the recognizer and
  dictionary from v0.3.0, whose [LICENSE at its release
  commit](https://github.com/GreatV/oar-ocr/blob/7a0ea9cbe5de7d9e4c8415b032f4854fb688245e/LICENSE)
  is also Apache-2.0.

Verified from those pinned cards and LICENSE files on 2026-10-24. No model
artifacts, revisions, sizes or hashes changed with the attribution addition.
