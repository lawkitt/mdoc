---
status: known-flaw
flaw: initials-only-missed
issue: 387
kind: tables
source: hunt-r6
expect: miss: PERSON "A. M."
expect: miss: PERSON "N. O."
---
# Appointment attendance register

Each row concerns one individual.

| Given name | Surname | Initials | Function |
| --- | --- | --- | --- |
| ⟦PERSON|Avelina⟧ | ⟦PERSON|McVellin⟧ | ⟦PERSON|A. M.⟧ | Witness |
| ⟦PERSON|Nerovan⟧ | ⟦PERSON|O’Kell⟧ | ⟦PERSON|N. O.⟧ | Recipient |

The administrator must compare the signing record with the row concerned. A shared first initial is not sufficient to establish that two records concern the same person. The surname and the remaining initials must also be checked before a correction is made.

| Personal contact | Telephone | Document check |
| --- | --- | --- |
| ⟦EMAIL|avelina.attendance@reed-vell.invalid⟧ | ⟦PHONE|+44 7700 900731⟧ | Passport: ⟦IDENTITY|MT 73-84.2961⟧ |
| ⟦EMAIL|nerovan.receipt@purl-kell.invalid⟧ | ⟦PHONE|+1.202.555.0187⟧ | Passport: ⟦IDENTITY|QV 58.24-7319⟧ |

Attendance is proposed for 27 January 2027. The total verification charge is GBP 48. Paragraph 8.2 governs retention of the source images and the correction record.
