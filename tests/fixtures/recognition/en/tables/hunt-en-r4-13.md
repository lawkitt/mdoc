---
status: known-flaw
flaw: bank-slash-filter
issue: unfiled
kind: tables
source: hunt-r4
expect: boundary: gold BANK "38-47-21/00631859"; proposed PHONE "00631859"
expect: miss: BANK "62-18-43/00492716"
---
# Reimbursement schedule with shared appointment details

The first cell in the source schedule spans two rows. It identifies one appointment, while the two payment rows identify separate recipients. The export preserves that arrangement as an HTML table rather than repeating the appointment description in each row.

<table>
<tr><th>Appointment</th><th>Recipient and address</th><th>Payment instruction</th></tr>
<tr><td rowspan="2">Inspection attendance, 17 December 2026</td><td>⟦PERSON|Sevra Mornell⟧<br>⟦ADDRESS|61 Thistle Row, Winchester⟧</td><td>Bank account: ⟦BANK|62-18-43/00492716⟧<br>GBP 125</td></tr>
<tr><td>⟦PERSON|Dorin Pellane⟧<br>⟦ADDRESS|8 Vellor Lane, Salisbury⟧</td><td>Bank account: ⟦BANK|38-47-21/00631859⟧<br>GBP 140</td></tr>
<tr><td colspan="2">Total reimbursement</td><td>GBP 265</td></tr>
</table>

Each bank value combines a branch code and account number using the slash printed on the supplied instruction. The values identify payment accounts, not proceedings. The totals row contains an amount only and must not be used as a substitute payment instruction.

The administrator must compare the schedule with the signed expense forms before release. A change to either destination requires confirmation through the existing contact channel. Approval of the total does not by itself approve a different recipient or a different bank account.

The two addresses are correspondence addresses supplied by the recipients. The schedule does not authorise publication of them with a public attendance list. Paragraph 9.2 of the reimbursement procedure governs retention of the supporting receipts.

Prepared on 18 December 2026 for the next payment run.
