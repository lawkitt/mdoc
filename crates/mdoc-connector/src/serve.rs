//! The connector over the pipe (Milestone 2 item 7): one wire request
//! from the `mdoc-mcp` shim, answered through [`Connector`] alone, so
//! every read and delivery is the service's, checked and recorded as it
//! is. The answer holds what the service handed out (checked text,
//! labels, headings, counts, ids) and refusals by code: no error's words,
//! so a Vault message never reaches an AI app.
use crate::{Connector, ConnectorError, Delivery, DeliveryKind, Read, Readiness, State};
use mdoc_mcp::wire::{
    self, Answer, Call, Done, Heading, Kind, Listed, PartRow, Refusal, Request, Why,
};

/// Answer one request line: the answer line.
pub fn answer_line(connector: &Connector, line: &[u8]) -> Vec<u8> {
    let answer = match serde_json::from_slice::<Request>(line) {
        Ok(request) if request.v == wire::VERSION => answer(connector, request),
        Ok(_) => Answer::Refused(Refusal::Version),
        Err(_) => Answer::Refused(Refusal::BadRequest),
    };
    serde_json::to_vec(&answer).expect("answers serialize")
}

/// Answer one request.
pub fn answer(connector: &Connector, request: Request) -> Answer {
    let app = request.app;
    let done = match request.call {
        Call::ListDocuments => connector.list_documents().map(|rows| Done::Documents {
            documents: rows
                .into_iter()
                .map(|row| Listed {
                    doc_id: row.doc_id,
                    label: row.label,
                    chars: row.size.chars,
                    words: row.size.words,
                    needs_review: match row.state {
                        State::Ready => None,
                        State::NeedsReview(readiness) => Some(why(readiness)),
                    },
                })
                .collect(),
        }),
        Call::GetDocument { doc_id, part } => connector.get_document(&app, &doc_id, part).map(read),
        Call::Deliver {
            kind,
            title,
            read,
            markdown,
        } => {
            let kind = match kind {
                Kind::Reply => DeliveryKind::Reply,
                Kind::Document => DeliveryKind::Document,
            };
            let delivery = Delivery {
                kind,
                title,
                read,
                markdown,
            };
            connector
                .deliver(&app, delivery)
                .map(|delivered| Done::Delivered {
                    reply: delivered.reply,
                    document: delivered.document,
                    restored: delivered.restored,
                    unresolved: delivered.unresolved,
                })
        }
    };
    match done {
        Ok(done) => Answer::Ok(done),
        Err(error) => Answer::Refused(refusal(&error)),
    }
}

fn read(read: Read) -> Done {
    let headings = |headings: Vec<crate::Heading>| {
        (headings.into_iter())
            .map(|heading| Heading {
                level: heading.level,
                text: heading.text,
            })
            .collect()
    };
    match read {
        Read::Text {
            doc_id,
            label,
            text,
        } => Done::Text {
            doc_id,
            label,
            text,
        },
        Read::Outline {
            doc_id,
            label,
            note,
            size,
            parts,
        } => Done::Outline {
            doc_id,
            label,
            note,
            chars: size.chars,
            words: size.words,
            parts: (parts.into_iter())
                .map(|part| PartRow {
                    part: part.part,
                    chars: part.chars,
                    headings: headings(part.headings),
                })
                .collect(),
        },
        Read::Part {
            doc_id,
            label,
            part,
            of,
            headings: these,
            text,
        } => Done::Part {
            doc_id,
            label,
            part,
            of,
            headings: headings(these),
            text,
        },
        Read::NeedsReview {
            doc_id,
            label,
            readiness,
        } => Done::NeedsReview {
            doc_id,
            label,
            why: why(readiness),
        },
    }
}

fn why(readiness: Readiness) -> Why {
    match readiness {
        Readiness::ChangedSinceReview => Why::ChangedSinceReview,
        Readiness::NamesNotReplaced(count) => Why::NamesNotReplaced { count },
        Readiness::NotReviewed | Readiness::Ready => Why::NotReviewed,
    }
}

fn refusal(error: &ConnectorError) -> Refusal {
    match error {
        ConnectorError::NothingShared => Refusal::NothingShared,
        ConnectorError::NotShared => Refusal::NotShared,
        ConnectorError::NoSuchPart { of } => Refusal::NoSuchPart { of: *of },
        ConnectorError::ShareChanged => Refusal::ShareChanged,
        ConnectorError::NotRead => Refusal::NotRead,
        ConnectorError::TitleNeeded => Refusal::TitleNeeded,
        ConnectorError::Empty => Refusal::Empty,
        ConnectorError::Vault(error) => {
            // Kept in mdoc: a Vault message is not for an AI app.
            eprintln!("mdoc: the AI connector's call failed: {error}");
            Refusal::Unavailable
        }
    }
}
