"""Pinned candidate files for the vision evaluation (lawkitt/mdoc#911, decision 8).

Every file comes from an official source at a fixed revision and is checked by
size and SHA-256 after download. Seal text detection has no official ONNX
build, so `fetch` converts the pinned Paddle files with paddle2onnx (see
`CONVERSIONS`); the conversion is deterministic on one machine and the
result's digest is listed so another machine can compare.
"""
from __future__ import annotations

from dataclasses import dataclass

HF = "https://huggingface.co"
MIRAGE_REV = "3d03cee88c0acbe4e71b94b567ab14e6e55f40c9"
CACTUS_REV = "d2c5cd70432ed78345f3ba6b620bff2feeec6c86"  # "Publish Cactus engine 4.1.1"
LAYOUT_ONNX_REV = "feb74619326f634e0e883218598096a3733ad9f7"
LAYOUT_PADDLE_REV = "aa52b8528c84f9b1a34ac3a88fe0e576edb9d11d"
SEAL_SERVER_REV = "a66e4245e02aeccedf63d6bc6ef2dab4726e3956"
SEAL_MOBILE_REV = "22ecaa81550a4c159bf3779293aa15d4b4797346"
OAR_REC = "https://github.com/GreatV/oar-ocr/releases/download/v0.3.0"

ONNX_RUNTIME = "1.27.0"  # the app's runtime, crates/mdoc-import/src/ocr.rs
RXING = ("rxing", "0.9.4", "6fbf2a0e464c316b30b4cab99e898f20a9d8e42ca3d7a767e670c9a330f2202f")


@dataclass(frozen=True)
class Pin:
    path: str  # relative to the models folder
    url: str
    sha256: str
    size: int
    platform: str = "any"  # "macos-arm64", "windows-x86_64", "ios-arm64" or "any"


def _cactus(platform: str, name: str, sha: str, size: int) -> Pin:
    return Pin(
        f"cactus-4.1.1/{platform}/{name}",
        f"{HF}/Cactus-Compute/cactus/resolve/{CACTUS_REV}/releases/4.1.1/{platform}/{name}",
        sha,
        size,
        platform,
    )


PINS: list[Pin] = [
    # Mirage, the reference candidate (never shipped, decision 25).
    Pin("mirage/mirage.cact", f"{HF}/Cactus-Compute/mirage/resolve/{MIRAGE_REV}/mirage.cact",
        "4249a0f7515272ff76b9414bec17d7e7daf84b15b66c7fb4f17fce1e66f9f237", 21_611_043),
    Pin("mirage/config.json", f"{HF}/Cactus-Compute/mirage/resolve/{MIRAGE_REV}/config.json",
        "f7a3c37642086f954b1000fe80edb544a153c76fe68fb1b8173ddb97d465e915", 1_430),
    # Cactus engine 4.1.1 (closed source, Apache-2.0 binaries).
    Pin("cactus-4.1.1/manifest.json",
        f"{HF}/Cactus-Compute/cactus/resolve/{CACTUS_REV}/releases/4.1.1/manifest.json",
        "27168631232dbff52a205b54bbfad54fd9b81f06fa368657d07449e2221f5e25", 8_897),
    Pin("cactus-4.1.1/LICENSE",
        f"{HF}/Cactus-Compute/cactus/resolve/{CACTUS_REV}/releases/4.1.1/LICENSE",
        "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30", 11_358),
    _cactus("macos-arm64", "libcactus.a", "f611ff8be89764be29bba251a97cba1b15382658e1739d35f873b587fda5554d", 2_021_472),
    _cactus("macos-arm64", "cactus.h", "9be7eb25b1d89180495d04a0d2c27053f282dac85add50cacc48d73302819d9d", 1_721),
    _cactus("windows-x86_64", "libcactus.a", "1c667e54b46944e971fd74445fc735b3e8b0710c265deed7713e003e781a7862", 3_022_488),
    _cactus("windows-x86_64", "cactus.h", "9be7eb25b1d89180495d04a0d2c27053f282dac85add50cacc48d73302819d9d", 1_721),
    _cactus("ios-arm64", "libcactus.a", "134739c1e867a5927374baa68d2ac0621ac293be9b7fab8379d94a70904a7966", 1_963_224),
    _cactus("ios-arm64", "cactus.h", "9be7eb25b1d89180495d04a0d2c27053f282dac85add50cacc48d73302819d9d", 1_721),
    # PP-DocLayout_plus-L: PaddlePaddle's own ONNX build, plus the Paddle
    # inference.yml the class list is checked against.
    Pin("paddle/PP-DocLayout_plus-L_onnx/inference.onnx",
        f"{HF}/PaddlePaddle/PP-DocLayout_plus-L_onnx/resolve/{LAYOUT_ONNX_REV}/inference.onnx",
        "77afb2caa74dd13240d087d2eced91d7fcd2caebd16006a0a66162fc8707ff0e", 129_736_329),
    Pin("paddle/PP-DocLayout_plus-L_onnx/inference.yml",
        f"{HF}/PaddlePaddle/PP-DocLayout_plus-L_onnx/resolve/{LAYOUT_ONNX_REV}/inference.yml",
        "d60f782a16f96afb27e8280399899a94c3e9ffc694ffb2f913ea00af1c522f1e", 1_838),
    Pin("paddle/PP-DocLayout_plus-L/inference.yml",
        f"{HF}/PaddlePaddle/PP-DocLayout_plus-L/resolve/{LAYOUT_PADDLE_REV}/inference.yml",
        "d60f782a16f96afb27e8280399899a94c3e9ffc694ffb2f913ea00af1c522f1e", 1_838),
    # PP-OCRv4 seal text detection, server (the seal pipeline's default) and
    # mobile: Paddle files, converted by `fetch` (CONVERSIONS).
    *[
        Pin(f"paddle/{name}/{f}", f"{HF}/PaddlePaddle/{name}/resolve/{rev}/{f}", sha, size)
        for name, rev, files in [
            ("PP-OCRv4_server_seal_det", SEAL_SERVER_REV, [
                ("inference.json", "bd9a5f7db2ffcbdafd32c7c329d4cb0a5135fa21dab06cd4fe1ed3f630a5a9fc", 307_109),
                ("inference.pdiparams", "62dc877ad9357bc19e2bd5260523a1736b0d7f311ec11ec60f7e4059af996a64", 113_954_077),
                ("inference.yml", "2e792dee0dcee6116414975391c34df699d0bffe52f8beccea07e4291455f6c2", 925),
            ]),
            ("PP-OCRv4_mobile_seal_det", SEAL_MOBILE_REV, [
                ("inference.json", "05feef1acb00aa4cd7362b15f7f501fc4f99d7b1fa73c1c871e0c7b1504b0f5c", 229_777),
                ("inference.pdiparams", "6c8eaed9baf7af3a55c9b36d5aeabc6061b99b2cb9dfb7d0157397a1aad0fe44", 4_692_937),
                ("inference.yml", "e8a924f30ed9a2072b7745bc5a9612308137b562d644e62831b69f99bb8049ab", 927),
            ]),
        ]
        for f, sha, size in files
    ],
    # PP-OCRv5 Cyrillic mobile recognition: the app's own pinned file
    # (pdf-inspector PP_OCR_CYRILLIC), output-identical to PaddlePaddle's
    # cyrillic_PP-OCRv5_mobile_rec_onnx.
    Pin("ocr/cyrillic_pp-ocrv5_mobile_rec.onnx", f"{OAR_REC}/cyrillic_pp-ocrv5_mobile_rec.onnx",
        "a18d96d7c8d73d90f2ed056549caa1de3a8e6cb744cccba16cd593ea8cd2d569", 8_076_390),
    Pin("ocr/ppocrv5_cyrillic_dict.txt", f"{OAR_REC}/ppocrv5_cyrillic_dict.txt",
        "db40aa52ceb112055be80c694afdf655d5d2c4f7873704524cc16a447ca913ba", 2_781),
]

# paddle2onnx 2.1.0 with paddlepaddle 3.3.1, opset 16 (the opset of
# PaddlePaddle's own PP-DocLayout_plus-L ONNX build), checker on. Expected
# digests are the macOS arm64 results; another platform reports its own.
CONVERSIONS = {
    "paddle/PP-OCRv4_server_seal_det_onnx/inference.onnx": (
        "paddle/PP-OCRv4_server_seal_det",
        "1fb57939fe8801224858fb05fba73ff14d6d3194861ac00e2dcd3ff0410ad457",
    ),
    "paddle/PP-OCRv4_mobile_seal_det_onnx/inference.onnx": (
        "paddle/PP-OCRv4_mobile_seal_det",
        "bd6a01ad272a17f0d8dafeb0adc8cdd458527ba3372b6124405a4e74214c6ca6",
    ),
}

# PP-DocLayout_plus-L's label_list, in output index order, as the official
# inference.yml lists it (checked by `fetch`).
LAYOUT_LABELS = [
    "paragraph_title", "image", "text", "number", "abstract", "content", "figure_title",
    "formula", "table", "reference", "doc_title", "footnote", "header", "algorithm",
    "footer", "seal", "chart", "formula_number", "aside_text", "reference_content",
]
