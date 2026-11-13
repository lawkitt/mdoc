- **Hosted iOS failures name their failing tests.** A failed `ci/ios`
  part reads its xcresult and names each failed test, as
  `ios:<target>/<class>/<method>`, in its error annotation ("Failed in
  ci/ios (ui 1): <test> and 1 more", as the PC's status words it) and job
  summary, and `ci/ios` repeats them, so ADR 0112's "same test elsewhere"
  rule can read hosted iOS failures
  ([ADR 0113](docs/adr/0113-ci-targets-by-commit-and-detection-off-macos.md)).
