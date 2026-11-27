//! Ask in the window (#1164 Decisions 2, 3 and 22; Milestone 2 item 10),
//! only while "AI connector (preview)" runs: what the document header's
//! choice does (`TabEvent::Ask`), and the Project page's Ask button. The
//! words and links are `crate::ai::ask`'s.
//!
//! Ask Claude or Ask ChatGPT shares the Project as part of the click, in
//! place of any other, then opens the app with a prompt naming the
//! document by the label the connector gives it. Copy for AI is today's
//! Copy for AI. Each choice is remembered as `ai_connector.ask`.
//!
//! Its actions and its view (the Project page's Ask button and menu) each
//! have their file.
mod actions;
mod view;
