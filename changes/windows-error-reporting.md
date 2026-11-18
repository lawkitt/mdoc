- **Crashes on Windows no longer reach Windows Error Reporting.** When mdoc or
  one of its conversion workers crashed, Windows filed a crash report under
  ProgramData and could send it to Microsoft, although the process held
  document text. Every mdoc process now opts out of Windows Error Reporting
  when it starts, without changing any system setting; mdoc's own crash
  report is unchanged ([#925](https://github.com/lawkitt/mdoc/issues/925)).
