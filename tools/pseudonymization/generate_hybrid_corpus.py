"""Generate original synthetic structured-PII fixtures; no network or documents."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "tests/fixtures/pseudonymization/hybrid/corpus.json"


def inn(seed):
    digits = [int(c) for c in seed]
    weights = ([2, 4, 10, 3, 5, 9, 4, 6, 8] if len(seed) == 9
               else [7, 2, 4, 10, 3, 5, 9, 4, 6, 8])
    digits.append(sum(d * w for d, w in zip(digits, weights)) % 11 % 10)
    if len(seed) == 10:
        weights = [3, 7, 2, 4, 10, 3, 5, 9, 4, 6, 8]
        digits.append(sum(d * w for d, w in zip(digits, weights)) % 11 % 10)
    return "".join(map(str, digits))


def snils(seed):
    check = sum(int(d) * w for d, w in zip(seed, range(9, 0, -1))) % 101
    if check == 100:
        check = 0
    return f"{seed[:3]}-{seed[3:6]}-{seed[6:]} {check:02}"


def gold(category, value):
    return f"⟦{category}|{value}⟧"


def main():
    # Retain the established short corpus verbatim, including invalid/OCR gold.
    base = json.loads((ROOT / "tests/fixtures/pseudonymization/corpus.json").read_text())
    corpus = [f for f in base if f["kind"] not in {"long", "stress"}]

    def add(name, language, kind, text):
        corpus.append(dict(id=f"hybrid-{name}", language=language,
                           kind=kind, annotated=text))

    # Calibration positives: exact formats and source surfaces defined up front.
    add("email", "en", "calibration", f"Email: {gold('EMAIL', 'qa+legal@example.invalid')}. Mail {gold('EMAIL', 'qa+legal@example.invalid')}.")
    add("phone", "mixed", "calibration", f"Phone: {gold('PHONE', '+1 (202) 555-0189')}; телефон: {gold('PHONE', '+7 (495) 555-02-19')}.")
    add("inn", "ru", "calibration", f"ИНН: {gold('TAX', inn('770100001'))}; ИНН физлица: {gold('TAX', inn('5001000001'))}.")
    add("snils", "ru", "calibration", f"СНИЛС: {gold('IDENTITY', snils('123456789'))}. Номер дела А40-987654/2026.")
    add("iban", "en", "calibration", f"IBAN: {gold('BANK', 'GB29 NWBK 6016 1331 9268 19')}; IBAN: {gold('BANK', 'DE89 3704 0044 0532 0130 00')}.")
    add("ru-bank", "ru", "calibration", f"Расчётный счёт: {gold('BANK', '40702810900000001234')}; БИК: {gold('BANK', '044525225')}. Цена 407 028 руб.")
    add("hidden", "mixed", "calibration", f"[write](mailto:{gold('EMAIL', 'hidden@example.invalid')})\n<div title='ИНН: {gold('TAX', inn('770100002'))}'>Ё</div>\n`СНИЛС: {gold('IDENTITY', snils('234567891'))}`")
    add("calibration-negative", "mixed", "calibration", "Article 309; дело А40-987654/2026; цена 202 555 0147 рублей; ИНН, СНИЛС и счёт — названия полей. not-a-mail@localhost; EMAIL; PHONE; BANK.")

    # Holdout definition is frozen before the first run; do not retune on it.
    add("holdout-email", "mixed", "holdout", f"😀 é Ё контакты: {gold('EMAIL', 'юрист@example.invalid')}; {gold('EMAIL', 'mixed.а@example.invalid')}.\n```json\n{{\"email\": \"{gold('EMAIL', 'case-42@example.invalid')}\"}}\n```\n[contact](https://example.invalid/?email={gold('EMAIL', 'url@example.invalid')})")
    add("holdout-phone", "mixed", "holdout", f"[call](tel:{gold('PHONE', '+7-495-555-03-29')})\n<div data-phone='{gold('PHONE', '+44 20 7946 0123')}'>x</div>\nTelephone: {gold('PHONE', '202-555-0169')}. Телефон: {gold('PHONE', '8 (495) 555-01-47')}.")
    add("holdout-inn", "ru", "holdout", f"ИНН: {gold('TAX', inn('540100003'))}; inn={gold('TAX', inn('6601000002'))}; ИНН: {gold('TAX', inn('540100003'))}. Цена 540100003 руб.")
    add("holdout-snils", "ru", "holdout", f"СНИЛС: {gold('IDENTITY', snils('345678912'))}\n`snils={gold('IDENTITY', snils('456789123').replace('-', '').replace(' ', ''))}`")
    add("holdout-bank", "mixed", "holdout", f"IBAN: {gold('BANK', 'NL91 ABNA 0417 1643 00')}; account: {gold('BANK', '40702810500000009876')}; БИК: {gold('BANK', '044525593')}.\n| счёт | БИК |\n| --- | --- |\n| {gold('BANK', '40702810500000009876')} | {gold('BANK', '044525593')} |")
    add("holdout-unlabeled", "ru", "holdout", f"| ИНН | СНИЛС |\n| --- | --- |\n| {gold('TAX', inn('540100003'))} | {gold('IDENTITY', snils('345678912'))} |")
    add("holdout-invalid-sensitive", "ru", "holdout", f"ИНН: {gold('TAX', '7701234567')}; СНИЛС: {gold('IDENTITY', '123-456-789 00')}; телефон: {gold('PHONE', '+7 (495) 555-О1-47')}. Ошибки распознавания не делают сведения несекретными.")
    add("holdout-short-international", "en", "holdout", f"Telephone: {gold('PHONE', '+65 6123 4567')}. Term is 65 days.")
    add("holdout-identifiers-as-contract-numbers", "ru", "negative", f"Договор № {inn('540100003')}, цена {inn('6601000002')} рублей. Срок 12.10.2026; ст. 309 ГК РФ; приложение 112-233-445 95.\nПлатёжное поручение № 044525593.")
    add("holdout-negative-labels", "mixed", "negative", "ИНН: 0000000000; СНИЛС: 000-000-000 00; phone: 42; БИК: 12345; account: 12345.67. Formula x + 2026 - 314; case 202-555-0169.")
    add("holdout-negative-checksums", "mixed", "negative", "IBAN category; GB28NWBK60161331926819 appears as an unlabeled invalid example. Article 12345678900; `invoice_id=40702810500000009876`; case number 7707083894.")
    add("holdout-negative-long-digits", "en", "negative", "Reference 123456789012345678901234567890. Invoice +1234567890123456789012345. Date 2026-10-07, section 12.3.4.")
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(corpus, ensure_ascii=False, indent=2) + "\n")
    print(f"Wrote {len(corpus)} fixtures to {OUT}")


if __name__ == "__main__":
    main()
