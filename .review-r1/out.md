Found **seven issues** at `9410e064`. The first three introduce company-name false negatives; the others leave English requirements incomplete.

1. **P1 — Public-body keywords erase real companies.** [en.toml:123](E:/t3/worktrees/mdoc/windows-lane-4/crates/mdoc-detect/rules/en.toml:123), [en.toml:150](E:/t3/worktrees/mdoc/windows-lane-4/crates/mdoc-detect/rules/en.toml:150)  
   ORG spans containing `cma`, `sec`, or `commission` are rejected anywhere unless they include a recognized legal suffix. This drops **[CMA CGM](https://www.cma-cgm.com/About/TheGroup)**, **[SEC Newgate](https://secnewgate.com/)**, and **[Commission Junction](https://www.cj.com/about-us)**. These names previously survived. Restrict ambiguous abbreviations and generic words using public-body context; add company counterexamples through `correct()`.

2. **P2 — The geographic-prefix rule accepts arbitrary company names as places.** [en.toml:283](E:/t3/worktrees/mdoc/windows-lane-4/crates/mdoc-detect/rules/en.toml:283)  
   `Republic of` followed by any one to three words qualifies for rejection. Consequently, **[The Republic of Tea](https://www.republicoftea.com/blogs/about-us/about-us)** is dropped as ORG. There is no geographic-name validation. Narrow this rule to recognized places or supporting context, with private-company counterexamples.

3. **P2 — A law abbreviation globally suppresses an unrelated company abbreviation.** [rules.rs:827](E:/t3/worktrees/mdoc/windows-lane-4/crates/mdoc-detect/src/rules.rs:827)  
   In `Data Protection Act (DPA). DPA Microphones (“DPA”) supplied the equipment. DPA invoiced us.`, the document-wide set removes every standalone `DPA`, including references to **[the microphone company](https://www.dpamicrophones.com/)**. Company definitions cannot override the blacklist. Track conflicting definitions and preserve ambiguous company mentions; test both meanings in one document.

4. **P2 — Several public-body and law filters still apply only to ORG.** [en.toml:164](E:/t3/worktrees/mdoc/windows-lane-4/crates/mdoc-detect/rules/en.toml:164)  
   PERSON detections such as `Ninth Circuit`, `Chancery Division`, `Attorney General`, and `Bankruptcy Code` survive: their rejection patterns belong exclusively to the ORG shape, and the widened word lists do not cover them. This leaves the [English PERSON requirement in #378](https://github.com/lawkitt/mdoc/issues/378#issuecomment-6571061364) incomplete. Share suitable patterns across both categories and test each label.

5. **P2 — Public boards remain completely uncovered.** [en.toml:165](E:/t3/worktrees/mdoc/windows-lane-4/crates/mdoc-detect/rules/en.toml:165)  
   `National Labor Relations Board` and `Board of Education` still survive as ORG. Neither the new words nor the body patterns recognize boards, although #378 explicitly requests them. Add qualified public-board patterns while preserving private advisory boards.

6. **P2 — The place list leaves ordinary countries and cities proposed as ORG.** [en.toml:239](E:/t3/worktrees/mdoc/windows-lane-4/crates/mdoc-detect/rules/en.toml:239), [en.toml:268](E:/t3/worktrees/mdoc/windows-lane-4/crates/mdoc-detect/rules/en.toml:268)  
   Examples include `North Macedonia`, `Phoenix`, and standalone `Plymouth`. None matches the list or the administrative-prefix rules. These remain gaps against [#478’s country and city requirements](https://github.com/lawkitt/mdoc/issues/478#issuecomment-6571064909). Add coverage beyond the selected examples without suppressing company names containing those places.

7. **P2 — A line break defeats defined-law abbreviation recognition.** [rules.rs:697](E:/t3/worktrees/mdoc/windows-lane-4/crates/mdoc-detect/src/rules.rs:697)  
   `Biometric Information\nPrivacy Act (“BIPA”)` does not register `BIPA`: title separators exclude newlines, and the remaining suffix `Privacy Act` cannot spell its initials. Subsequent BIPA detections survive, despite #378 requiring defined abbreviations to disappear. Support wrapped titles and test LF and CRLF.

Static review only; no edits, builds, or tests. `git diff --check` passed.