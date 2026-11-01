---
status: known-flaw
flaw: hidden-underscore-name ru-identifier-category
issue: unfiled
kind: docx
source: import
expect: miss: PERSON "Анна_Ёлкина" ×2
expect: boundary: gold TAX "7701234567"; proposed PHONE "7701234567"
---
| Заказчик | ИНН |
| --- | --- |
| ⟦ORG|ООО «Северный Вектор»⟧ | ⟦TAX|7701234567⟧ |

[Письмо](mailto:⟦EMAIL|anna@example.invalid⟧)
[Клиент](clients/⟦PERSON|Анна_Ёлкина⟧.md)
![Подпись](images/⟦PERSON|Анна_Ёлкина⟧.png)
`директор = "⟦PERSON|Анна Ёлкина⟧"`
<div data-client="⟦PERSON|Анна Ёлкина⟧">Договор</div>
