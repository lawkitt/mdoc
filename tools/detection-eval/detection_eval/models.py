"""The shortlist (decision 9 on lawkitt/mdoc#903): one entry per model.

Each entry names its Hugging Face repository and pinned revision, its
adapter family, a size class, its threshold, and how its labels become the
app's GLiNER2 label names (`fixtures.LABELS`). Labels a model has that map to
nothing are dropped, as the app drops them (decision 12). A zero-shot model
(GLiNER, GLiNER2) gets `prompts`: the label words it is asked for, each
renamed to an app label; a token classifier gets `labels`: its own tag names
(without the B-/I-/E-/S- prefix) renamed the same way.

To add a model: add an entry here (see README.md).
"""

from __future__ import annotations

from dataclasses import dataclass, field

# The app's own schema, for GLiNER2 models: the labels are the app's.
APP_PROMPTS = {label: label for label in (
    "person", "full_name", "organization", "email", "phone_number", "address",
    "passport_number", "national_id_number", "tax_id", "bank_account", "iban",
    "routing_number",
)}

# Plain-language prompts for GLiNER v1 models.
GLINER_PROMPTS = {
    "person": "person",
    "organization": "organization",
    "email": "email",
    "phone number": "phone_number",
    "address": "address",
    "passport number": "passport_number",
    "national id number": "national_id_number",
    "social security number": "national_id_number",
    "tax id": "tax_id",
    "bank account number": "bank_account",
    "iban": "iban",
    "routing number": "routing_number",
}


@dataclass(frozen=True)
class Model:
    id: str
    repo: str
    revision: str
    family: str  # gliner2 | gliner | token | pii_tracer
    size: str  # large | mid | small
    licence: str
    threshold: float = 0.5
    prompts: dict[str, str] = field(default_factory=dict)
    labels: dict[str, str] = field(default_factory=dict)
    note: str = ""


MODELS: list[Model] = [
    # Large.
    Model(
        "gliner2-pii", "fastino/gliner2-privacy-filter-PII-multi", "1cb4166094dc58fa8d836429f060d6c95f62b495",
        "gliner2", "large", "Apache-2.0", prompts=APP_PROMPTS,
        note="the current default; the app's schema, one pass per family",
    ),
    Model(
        "gliner2-large", "fastino/gliner2-large-v1", "d7aa8a2850d175a9227ff80358ea380723d03ad2",
        "gliner2", "large", "Apache-2.0", prompts=APP_PROMPTS,
    ),
    Model(
        "gliner-guard-omni", "hivetrace/gliner-guard-omni", "763f8aa9a7714dd2c3264f3751acdd5d854c5ac4",
        "gliner2", "large", "Apache-2.0", prompts=APP_PROMPTS,
    ),
    Model(
        "nvidia-gliner-pii", "nvidia/gliner-PII", "bd23e8ef4425fd04e34c5204ab49ffaa706eae79",
        "gliner", "large", "NVIDIA Open Model License",
        prompts={
            **GLINER_PROMPTS,
            "company_name": "organization",
            "street_address": "address",
            "ssn": "national_id_number",
            "account_number": "bank_account",
            "bank_routing_number": "routing_number",
        },
    ),
    # Mid.
    Model(
        "bardsai-eu-pii-v2", "bardsai/eu-pii-anonimization-multilang-v2", "07a9a69b0d75c551f5f9450b290f864bfc7dfb51",
        "token", "mid", "Apache-2.0",
        labels={
            "PERSON_NAME": "person",
            "PERSON_ALIAS": "person",
            "ORGANIZATION_NAME": "organization",
            "EMAIL_ADDRESS": "email",
            "PHONE_NUMBER": "phone_number",
            "POSTAL_ADDRESS": "address",
            "DOCUMENT_IDENTIFIER": "passport_number",
            "PERSON_IDENTIFIER": "national_id_number",
            "ORGANIZATION_IDENTIFIER": "tax_id",
            "BANK_ACCOUNT_IDENTIFIER": "bank_account",
            "ACCOUNT_IDENTIFIER": "bank_account",
            "PAYMENT_CARD": "bank_account",
        },
    ),
    Model(
        "nym-pii", "Wismut/nym-pii-multilingual", "0ed0242d67a30ca6a83e5f118506ff1e1daf83ca",
        "token", "mid", "MIT", labels=(NYM := {
            "GIVEN_NAME": "person",
            "SURNAME": "person",
            "COMPANY_NAME": "organization",
            "EMAIL": "email",
            "PHONE": "phone_number",
            "FAX_NUMBER": "phone_number",
            "STREET_ADDRESS": "address",
            "SECONDARY_ADDRESS": "address",
            "BUILDING_NUMBER": "address",
            "STREET_NAME": "address",
            "ZIP_CODE": "address",
            "PASSPORT": "passport_number",
            "DRIVERS_LICENSE": "national_id_number",
            "GOVERNMENT_ID": "national_id_number",
            "SSN": "national_id_number",
            "EMPLOYEE_ID": "national_id_number",
            "CUSTOMER_ID": "national_id_number",
            "MEDICAL_RECORD_NUMBER": "national_id_number",
            "LICENSE_PLATE": "national_id_number",
            "TAX_ID": "tax_id",
            "ACCOUNT_NUMBER": "bank_account",
            "CREDIT_DEBIT_CARD": "bank_account",
            "IBAN": "iban",
            "SWIFT_BIC": "routing_number",
            "ROUTING_NUMBER": "routing_number",
        }),
        note="GIVEN_NAME and SURNAME tags of one name join into one person span",
    ),
    Model(
        "pii-tracer", "perplexity-ai/PII-Tracer", "d25c16f2e57e321f6d2527715c01df9112f956f5",
        "pii_tracer", "mid", "MIT",
        labels={
            "private_person": "person",
            "private_address": "address",
            "private_email": "email",
            "private_phone": "phone_number",
            "account_number": "bank_account",
        },
        note="pplx-pii-masking; no organization or ID tags; remote code",
    ),
    Model(
        "openai-privacy-filter", "openai/privacy-filter", "7ffa9a043d54d1be65afb281eddf0ffbe629385b",
        "token", "mid", "Apache-2.0",
        labels={
            "private_person": "person",
            "private_address": "address",
            "private_email": "email",
            "private_phone": "phone_number",
            "account_number": "bank_account",
        },
        note="no organization or ID tags",
    ),
    Model(
        "nuner-zero", "numind/NuNER_Zero", "c90187673f464518dca09f41689184ed6976242c",
        "gliner", "mid", "MIT",
        prompts={k.lower(): v for k, v in GLINER_PROMPTS.items()},
        note="lower-case labels; adjacent same-label tokens joined",
    ),
    Model(
        "gliner-stream-pii", "knowledgator/gliner-stream-pii-v1.0", "e871777dc4b3b688747a0433fff8d94a36fcc7b0",
        "gliner", "mid", "Apache-2.0",
        prompts=(KNOWLEDGATOR := {
            "name": "person",
            "person": "person",
            "organization": "organization",
            "email address": "email",
            "phone number": "phone_number",
            "address": "address",
            "passport number": "passport_number",
            "social security number": "national_id_number",
            "national id": "national_id_number",
            "tax identification number": "tax_id",
            "bank account": "bank_account",
            "iban": "iban",
            "routing number": "routing_number",
        }),
    ),
    # Small.
    Model(
        "gravitee-bert-small", "gravitee-io/bert-small-pii-detection", "f8c27a85c51c0168f07b9dcf00265bf0a4097939",
        "token", "small", "Apache-2.0",
        labels={
            "PERSON": "person",
            "ORGANIZATION": "organization",
            "EMAIL_ADDRESS": "email",
            "PHONE_NUMBER": "phone_number",
            "LOCATION": "address",
            "US_PASSPORT": "passport_number",
            "US_SSN": "national_id_number",
            "US_DRIVER_LICENSE": "national_id_number",
            "US_ITIN": "tax_id",
            "US_BANK_NUMBER": "bank_account",
            "CREDIT_CARD": "bank_account",
            "IBAN_CODE": "iban",
        },
        note="LOCATION includes bare places; English only",
    ),
    Model(
        "gliner-pii-edge", "knowledgator/gliner-pii-edge-v1.0", "9b7f39b0a2da971a5beea78d35f1539d4009c891",
        "gliner", "small", "Apache-2.0", prompts=KNOWLEDGATOR,
    ),
    Model(
        "nym-pii-small", "Wismut/nym-pii-multilingual-small", "4348999cd3c2e20c49615e9af7c6bbb45b64cd85",
        "token", "small", "MIT", labels=NYM,
        note="GIVEN_NAME and SURNAME tags of one name join into one person span",
    ),
]

BY_ID = {m.id: m for m in MODELS}

# Decision 9's combos, members in rank order (the first wins a category).
# GLiNER2-PII takes part as `default`: the app's own snapshots (the shipped
# FP16 build through the app's pipeline), as a shipped combo would run it.
COMBOS: dict[str, list[str]] = {
    "default+pii-tracer": ["default", "pii-tracer"],
    "default+bardsai-eu-pii-v2": ["default", "bardsai-eu-pii-v2"],
    "default+nuner-zero": ["default", "nuner-zero"],
}
