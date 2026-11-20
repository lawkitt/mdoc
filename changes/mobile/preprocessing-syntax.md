- **Values that look like Markdown are found.** An email in angle brackets
  (`<name@domain>`), a value with a literal `*` or `#` ("Fennel*Orbit Ltd",
  "ООО «Лист#8»") and a number at the start of a line after a label
  ("Паспорт:" then "46. 18 731904") are proposed instead of being hidden
  as source syntax; only whole HTML tags and real list markers stay
  protected, and a value after a list bullet is proposed without it
  (fixes #411; #423 and #428 in part;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
