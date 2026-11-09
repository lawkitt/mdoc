- **Company names in nested quotes stay whole.** A Russian company whose
  quoted name holds a second quoted name (ООО «Правовая группа «Кедровый
  щит»», straight quotes too) is proposed as one span, from its legal form
  to the last closing quote, where the detector cut it at the inner quote,
  kept only the inner name or left the outer closing quote out (fixes #557).
