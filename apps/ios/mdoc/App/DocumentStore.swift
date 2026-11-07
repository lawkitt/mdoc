import Foundation

/// The open documents' models (ADR 0104). A model is made when its document
/// opens, never while a view draws, and goes when the document closes,
/// unless a job still works on it. Views look models up; they never make
/// one. Outside observation: what is open is the navigator's to say.
@MainActor
final class DocumentStore {
    private var models: [DocumentID: DocumentViewModel] = [:]
    /// The window's undo steps of a document whose model went, put back
    /// when it opens again (#672): the core keeps its undo history.
    private var undoSteps: [DocumentID: Int] = [:]
    private let make: (DocumentID) throws -> DocumentViewModel

    init(make: @escaping (DocumentID) throws -> DocumentViewModel) {
        self.make = make
    }

    /// The open document's model, if it has one.
    func model(for id: DocumentID?) -> DocumentViewModel? {
        id.flatMap { models[$0] }
    }

    /// The document's model, made now if it has none.
    func open(_ id: DocumentID) throws -> DocumentViewModel {
        if let model = models[id] {
            return model
        }
        let model = try make(id)
        model.systemUndoSteps = undoSteps.removeValue(forKey: id) ?? 0
        models[id] = model
        return model
    }

    /// Lets go of every model but those in `kept`.
    func evict(keeping kept: Set<DocumentID>) {
        for (id, model) in models where !kept.contains(id) {
            if model.systemUndoSteps > 0 { undoSteps[id] = model.systemUndoSteps }
            models[id] = nil
        }
    }

    /// A deleted document: its model and steps go.
    func remove(_ id: DocumentID) {
        models[id] = nil
        undoSteps[id] = nil
    }

    /// Every model held now.
    var all: [DocumentViewModel] { Array(models.values) }

    /// The ids of the models held now.
    var ids: Set<DocumentID> { Set(models.keys) }
}
