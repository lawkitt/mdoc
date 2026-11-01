---
status: known-flaw
flaw: case-number ru-identifier-category invalid-identifier-missed
issue: 364 unfiled
kind: contract
source: import
expect: miss: IDENTITY "123-456-789 00"
expect: miss: CASE "А40-123456/2026"
expect: boundary: gold TAX "7701234567"; proposed IDENTITY "7701234567"
---
Заказчик: ⟦PERSON|Анна Сергеевна Ёлкина⟧. Паспорт: ⟦IDENTITY|4510 123456⟧. ИНН: ⟦TAX|7701234567⟧. СНИЛС: ⟦IDENTITY|123-456-789 00⟧. Расчётный счёт: ⟦BANK|40702810900000001234⟧. БИК: ⟦BANK|044525225⟧. Цена 407 028 рублей; дело № ⟦CASE|А40-123456/2026⟧.
