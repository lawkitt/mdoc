---
status: pass
flaw: bank-slash-filter
issue: 425
kind: tables
source: hunt-r4
---
# Reimbursement schedule with shared appointment details

One inspection appointment covers both payments below.

<table>
<tr><th>Appointment</th><th>Recipient and address</th><th>Payment instruction</th></tr>
<tr><td rowspan="2">Inspection attendance, 17 December 2026</td><td>⟦PERSON|Sevra Mornell⟧<br>⟦ADDRESS|61 Thistle Row, Winchester⟧</td><td>Bank account: ⟦BANK|62-18-43/00492716⟧<br>GBP 125</td></tr>
<tr><td>⟦PERSON|Dorin Pellane⟧<br>⟦ADDRESS|8 Vellor Lane, Salisbury⟧</td><td>Bank account: ⟦BANK|38-47-21/00631859⟧<br>GBP 140</td></tr>
<tr><td colspan="2">Total reimbursement</td><td>GBP 265</td></tr>
</table>

The totals row must not be used as a payment instruction.

The administrator must compare the schedule with the signed expense forms before release. A change to either destination requires confirmation through the existing contact channel. Approval of the total does not by itself approve a different recipient or a different bank account.

The two addresses are correspondence addresses supplied by the recipients. The schedule does not authorise publication of them with a public attendance list. Paragraph 9.2 of the reimbursement procedure governs retention of the supporting receipts.

Prepared on 18 December 2026 for the next payment run.
