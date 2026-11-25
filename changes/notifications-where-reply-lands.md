- **Notifications for AI deliveries and review requests.** Behind "AI
  connector (preview)", off by default, a reply or document delivered by an
  AI app raises a system notification, and so does an AI app asking for a
  document that needs review, once until it is opened. A notification names
  only the AI app and the label the app knows the document by, never a
  title or a name. Clicking it brings mdoc forward on the reply's Reply
  page, or on the document, ready for review. mdoc never takes focus by
  itself: a "new" badge on the shared Project's card in the sidebar counts
  what arrived and opens the newest, and the setting "Open deliveries when
  they arrive" (`ai_connector.open_deliveries`) opens each delivery as it
  lands (Milestone 2 item 9,
  [#1164](https://github.com/lawkitt/mdoc/issues/1164)).
