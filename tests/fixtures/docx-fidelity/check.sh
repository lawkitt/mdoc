#!/usr/bin/env bash
# Usage: tests/fixtures/docx-fidelity/check.sh [--update-baseline]
# The DOCX fidelity check (README.md beside this script): renders each fixture
# as the Source pane does and scores it against Word's reference PDF by page
# count and SSIM. Lists the reference PDFs still to export; fails when a score
# drops more than two points below this platform's baseline. Run by hand at
# each docxide bump; --update-baseline records the new scores.
# shellcheck source=scripts/lib.sh
source "$(dirname "$0")/../../../scripts/lib.sh"

cargo run --quiet --example docx_fidelity -- tests/fixtures/docx-fidelity "$@"
