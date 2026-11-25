import Foundation
import UIKit

/// Where Ask AI's wait for the AI app's answer stands (#1164 decision 11).
enum ReplyOffer: Equatable {
    /// The share sheet is up.
    case sharing
    /// The text went to an app; mdoc has not been left since.
    case awaitingReturn
    /// mdoc is back from the AI app: the "Paste reply?" banner shows.
    case offered
}

/// Ask AI on a phone (#1164 decision 11, Milestone 2 item 18): the share
/// sheet carries the document's text, names replaced, into an AI app; back
/// in mdoc, a "Paste reply?" banner restores the answer the user pastes.
/// Only behind Settings › AI connector (preview); the views check it.
extension DocumentViewModel {
    /// The share sheet's own entries that are no AI app: Ask AI leaves
    /// the apps and extensions the device has, and Copy.
    static let askAIExcludedActivities: [UIActivity.ActivityType] = [
        .mail, .message, .airDrop, .print, .assignToContact, .saveToCameraRoll, .addToReadingList,
        .markupAsPDF, .openInIBooks, TitledText.saveToFiles,
    ]

    /// Ask AI: what Share sends, the Vault's text checked against the
    /// Project's names now, titled by the file's name with names replaced,
    /// never by the document's own title. Refused, with why, until the
    /// document is Ready.
    func askAI() {
        do {
            let handOff = try session.copyForAI()
            let title = (handOff.fileName as NSString).deletingPathExtension
            leftSinceAsk = false
            replyOffer = .sharing
            askAIShare = ShareItems(items: [TitledText(text: handOff.text, title: title)])
            onChange()
        } catch {
            replyOffer = nil
            errorMessage = error.localizedDescription
        }
    }

    /// Ask AI's sheet closed: `activity` took the text, or the user
    /// cancelled (nil). An app that opened in front of mdoc may close the
    /// sheet only once mdoc is back, and then the banner shows at once. Copy
    /// opens no app: a trip away before it was not to the AI app, so the
    /// banner waits for the next one.
    func askAIFinished(activity: UIActivity.ActivityType?) {
        askAIShare = nil
        guard replyOffer == .sharing else { return }
        guard let activity else {
            replyOffer = nil
            return
        }
        if activity == .copyToPasteboard { leftSinceAsk = false }
        if leftSinceAsk {
            offerReply()
        } else {
            replyOffer = .awaitingReturn
        }
    }

    /// mdoc went to the background.
    func appLeft() {
        guard replyOffer == .sharing || replyOffer == .awaitingReturn else { return }
        leftSinceAsk = true
    }

    /// mdoc is in front again: back from the AI app, the banner shows.
    func appReturned() {
        if replyOffer == .awaitingReturn && leftSinceAsk {
            offerReply()
        }
    }

    /// The banner shows over the bar, so the review bar and, in a compact
    /// window, the Original give way to it: the user is back for the reply.
    private func offerReply() {
        replyOffer = .offered
        if wordsFix != nil { cancelFixWords() }
        if isReviewing { endReview() }
        if originalCoversText { show(.text) }
    }

    /// The banner shows.
    var showsReplyBanner: Bool { replyOffer == .offered }

    /// × on the banner: Paste and Type a reply stay in the bar.
    func dismissReplyBanner() {
        replyOffer = nil
    }
}
