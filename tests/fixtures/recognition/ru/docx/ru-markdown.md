---
status: known-flaw
flaw: hidden-underscore-name
issue: 376
kind: docx
source: import
expect: miss: PERSON "Анна_Ёлкина" ×2
---
| Заказчик | ИНН |
| --- | --- |
| ⟦ORG|ООО «Северный Вектор»⟧ | ⟦TAX|7701234567⟧ |

[Письмо](mailto:⟦EMAIL|anna@example.invalid⟧)
[Клиент](clients/⟦PERSON|Анна_Ёлкина⟧.md)
![Подпись](images/⟦PERSON|Анна_Ёлкина⟧.png)
`директор = "⟦PERSON|Анна Ёлкина⟧"`
<div data-client="⟦PERSON|Анна Ёлкина⟧">Договор</div>
