- **Labelled identifiers are proposed even when invalid.** After its label
  a СНИЛС, ИНН, passport, account, БИК, payment card or personnel number
  («Табельный номер: VK-731628») is proposed with its category whatever its
  checksum, a digit short or long or with an OCR letter for a digit too, as
  is an English passport or card number; an unlabelled number that fails
  its checksum is still left to the model (fixes #381).
