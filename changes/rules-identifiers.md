- **Identifiers take their label's category, and references are left
  alone.** A value after ИНН is a tax identifier and after СНИЛС an
  identity number, valid or not and in table columns too; numbers after
  "Article", "Договор №" or "поручение №", digit runs too long for a phone,
  emails without a dotted domain and tax rates are no longer proposed
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), fixes
  #375, #380, #381, #389).
