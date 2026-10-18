import SwiftUI
import UniformTypeIdentifiers

/// Owns one shared document request. Heavy conversion and immutable previews
/// stay on the file worker; only atomic adoption enters the render owner.
@MainActor final class DocumentColorController: ObservableObject {
    @Published private(set) var busy = false
    @Published private(set) var publishing = false
    @Published private(set) var ready = false
    @Published private(set) var loaded = false
    @Published private(set) var color = JSON()
    @Published private(set) var sourceInfo = JSON()
    @Published private(set) var rows: [JSON] = []
    @Published private(set) var previews: [CGImage] = []
    @Published private(set) var clipped: UInt64 = 0
    @Published private(set) var copy = false
    @Published var error: String?
    let preferences: ColorPreferencesStore
    let operation: String
    let history: Bool
    let properties: Bool
    let source: Bool
    private let redo: Bool
    @Published private(set) var catalog = JSON()
    @Published private(set) var bootstrap = JSON()
    @Published private(set) var propertiesView = JSON()
    private weak var store: EditorStore?
    private let expected: (UInt64, UInt64)?
    private let completion: (Bool) -> Void
    private var task: NativeProjectTask?
    private var closing = false
    private var finished = false
    init(store: EditorStore, request: JSON, expected: (UInt64, UInt64)?, completion: @escaping (Bool) -> Void) {
        self.preferences = store.colorPreferences
        self.store = store; self.expected = expected; self.completion = completion
        source = ["repair_source_profile", "rasterize_source"].contains(request["type"].string)
        operation = source ? request["type"].string : request["operation"].string
        history = request["type"].string == "color_history"
        properties = request["type"].string == "properties"
        redo = request["redo"].bool
        refreshCopy()
    }
    var text: JSON { catalog["document_color_copy"] }
    var title: String {
        if properties { return propertiesView["title"].isNull ? bootstrap["preparing_document"].string : propertiesView["title"].string }
        if history { return text[redo ? "redo_title" : "undo_title"].string }
        let titles = ["repair_source_profile": "repair_title", "rasterize_source": "rasterize_title", "assign": "assign_title", "depth": "depth_title"]
        return text[titles[operation] ?? "convert_title"].string
    }
    var propertiesDone: String { propertiesView["done"].isNull ? bootstrap["common"]["done"].string : propertiesView["done"].string }
    func refreshCopy() {
        guard let store else { return }
        catalog = store.catalog; bootstrap = store.bootstrap
    }
    func convertedName(_ completion: @escaping @MainActor (String) -> Void) {
        let name = (store?.state["tabs"][0]["title"].string ?? "") as NSString
        store?.query(["type": "document_delivery_message", "message": ["type": "converted_name", "name": name.deletingPathExtension]]) { completion($0.string) }
    }
    func load() {
        if properties || source {
            capture(source ? .source : .properties) { [weak self] task in
                NativeProjectTask.io.async {
                    do {
                        let details = try task.details()
                        DispatchQueue.main.async {
                            guard let self else { return }
                            if self.source { self.sourceInfo = details; self.color = details["color"] }
                            else { self.propertiesView = details; self.rows = details["rows"].array }
                            self.loaded = true; self.busy = false; self.task = nil
                            if self.closing { self.finish(false) }
                        }
                    } catch { let message = error.localizedDescription
                        DispatchQueue.main.async { self?.failed(message) }
                    }
                }
            }
        } else {
            store?.query(["type": "document_color"]) { [weak self] value in
                guard let self, !finished else { return }
                guard !value["space"].string.isEmpty else { failed(bootstrap["action_failed"].string); return }
                color = value; loaded = true
                if history { prepare(nil) }
            }
        }
    }
    private func capture(_ kind: NativeProjectTask.Kind, work: @escaping (NativeProjectTask) -> Void) {
        guard !busy, !closing, !finished else { return }
        guard let native = store?.native else { failed(catalog["native_copy"]["color"]["canvas_unavailable"].string); return }
        busy = true; ready = false; error = nil
        native.projectTask(kind: kind, expected: expected) { [weak self] task, error in
            DispatchQueue.main.async {
                guard let self else { task?.cancel(); return }
                if self.closing { task?.cancel(); self.busy = false; self.finish(false); return }
                guard let task else { self.failed(error ?? self.bootstrap["action_failed"].string); return }
                self.task = task; work(task)
            }
        }
    }
    func invalidate() {
        guard !busy else { return }
        ready = false; previews = []; clipped = 0; task = nil; error = nil
    }
    func prepare(_ choice: JSON?, copy: Bool = false) {
        guard !busy, !closing, !finished else { return }
        invalidate(); self.copy = copy
        capture(source ? .source : .color) { [weak self] task in
            guard let self, let native = store?.native else { task.cancel(); return }
            let history = self.history
            native.prepareEdit(task, choice: choice, copy: copy) { [weak self] error in
              NativeProjectTask.io.async {
                do {
                    if let error { throw HostFailure(message: error) }
                    let details = try task.details()
                    let images = history ? [] : try [task.comparison(after: false), task.comparison(after: true)]
                    DispatchQueue.main.async {
                        guard let self else { return }
                        self.busy = false
                        if self.closing { self.finish(false); return }
                        if self.source { self.sourceInfo = details }
                        self.clipped = details["clipped_channels"].uint; self.previews = images; self.ready = true
                        if self.history { self.apply() }
                    }
                } catch { let message = error.localizedDescription
                    DispatchQueue.main.async { self?.failed(message) }
                }
              }
            }
        }
    }
    func apply() {
        guard !busy, ready, !copy, !closing, !finished, let task, let native = store?.native else { return }
        busy = true; publishing = true
        native.finishProject(task, opening: true, title: title, url: nil) { [weak self] error in
            DispatchQueue.main.async {
                guard let self else { return }
                self.publishing = false; self.busy = false
                if let error { self.failed(error); return }
                // Color/history swaps the renderer without changing document
                // identity. Retire pending readbacks from its previous caches.
                if !self.source { self.store?.layerThumbnails.reset(); self.store?.filterPreviews.reset() }
                self.finish(true)
            }
        }
    }
    func saveCopy(to url: URL, access: URL? = nil) {
        guard !busy, ready, copy, !closing, !finished, let task else { return }
        let source = URL(string: store?.state["document_file"]["location"]["uri"].string ?? "")
        let delivery = catalog["document_delivery_copy"]
        guard url.pathExtension.lowercased() == "capy" else { error = delivery["converted_filename_invalid"].string; return }
        guard source?.standardizedFileURL.resolvingSymlinksInPath() != url.standardizedFileURL.resolvingSymlinksInPath() else {
            error = delivery["separate_copy"].string; return
        }
        busy = true; error = nil
        let exists = text["file_exists"].string
        NativeProjectTask.io.async { [weak self] in
            let scope = access?.startAccessingSecurityScopedResource() == true
            defer { if scope { access?.stopAccessingSecurityScopedResource() } }
            do {
                // A folder choice grants a destination, not permission to
                // replace an existing file. AppKit's Save panel asks separately.
                if access != nil && FileManager.default.fileExists(atPath: url.path) {
                    throw HostFailure(message: exists)
                }
                try task.write(to: url)
                DispatchQueue.main.async { self?.busy = false; self?.finish(true) }
            } catch { let message = error.localizedDescription
                DispatchQueue.main.async { self?.failed(message) }
            }
        }
    }
    func cancel() {
        guard !finished, !publishing else { return }
        closing = true; task?.cancel()
        if !busy { finish(false) }
    }
    private func failed(_ message: String) {
        busy = false; ready = false; task = nil
        if closing { finish(false) } else { error = message }
    }
    private func finish(_ success: Bool) {
        guard !finished else { return }
        finished = true; task = nil; completion(success)
    }
}
