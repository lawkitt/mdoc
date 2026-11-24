- **One ordering rule for Vault operations.** Saves, moves, deletes,
  exports, imports, replies and the quit follow one order: each is a job
  that reads what the window shows only when its turn comes, names before
  text, and changes the window only once the Vault has it. Typing during a
  move, delete, export or quit is never lost, and a large import no longer
  holds up saves ([ADR 0130](docs/adr/0130-one-ordering-rule-for-vault-operations.md),
  #1153).
