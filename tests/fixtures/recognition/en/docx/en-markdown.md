---
status: known-flaw
flaw: hidden-underscore-name
issue: unfiled
kind: docx
source: import
---
| Counsel | Contact |
| --- | --- |
| ⟦PERSON|Alice Morgan⟧ | ⟦EMAIL|alice@example.invalid⟧ |

[Write](mailto:⟦EMAIL|alice@example.invalid⟧) [Contact](https://example.invalid/clients/⟦PERSON|Alice_Morgan⟧)
![Signature](images/⟦PERSON|Alice_Morgan⟧.png)
`client = "⟦PERSON|Alice Morgan⟧"`
```json
{"email": "⟦EMAIL|alice@example.invalid⟧"}
```
<span data-client="⟦PERSON|Alice Morgan⟧">Private</span>
