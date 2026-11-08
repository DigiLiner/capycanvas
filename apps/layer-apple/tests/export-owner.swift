import Foundation
import Combine
import AppKit
import QuartzCore
import ImageIO
import UniformTypeIdentifiers

/// Real Metal owner and file coordinator, with disposable native-panel/provider
/// destinations. Library/preset operations run on the production file worker.
@main struct ExportOwnerChecks {
    static func io<T>(_ work: @escaping () throws -> T) async throws -> T {
        try await withCheckedThrowingContinuation { done in
            NativeProjectTask.io.async { done.resume(with: Result(catching: work)) }
        }
    }
    static func rejected(_ label: String, _ work: () throws -> Void) throws {
        var failed = false
        do { try work() } catch { failed = true }
        try require(failed, label)
    }
    @MainActor static func wait(_ label: String, store: EditorStore, _ ready: () -> Bool) async throws {
        try await CapyTest.wait(label, failure: { store.failure }, step: { try await prepare(store.native!, label) }, ready)
    }
    static func image(_ url: URL, type: UTType, depth: Int, extent: [Int]) throws {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
            let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
            let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [String: Any] else {
            throw HostFailure(message: "ImageIO cannot reopen \(url.lastPathComponent)")
        }
        try require(CGImageSourceGetType(source) as String? == type.identifier, "\(url.lastPathComponent): expected \(type.identifier), got \(CGImageSourceGetType(source) as String? ?? "none")")
        try require(image.bitsPerComponent == depth && [image.width, image.height] == extent, "Output precision and dimensions")
        try require(properties[kCGImagePropertyProfileName as String] != nil && image.colorSpace?.copyICCData() != nil, "Embedded output profile")
    }
    static func library(_ root: URL) throws -> JSON {
        let locations = StorageLocations.within(root.appendingPathComponent("preferences"))!
        let preferences = ColorPreferencesStore(locations: locations)
        let color = JSON(["space": "ProPhoto", "depth": "U16"])
        try require(try preferences.profiles().isEmpty, "Missing library is empty")
        let initial = try preferences.presets(color: color, request: JSON(["type": "get", "index": 2]))
        try require(initial["recipe"]["depth"].string == "U16", "Missing preferences use shared defaults")
        let original = root.appendingPathComponent("Original.icc")
        let bytes = CGColorSpace(name: CGColorSpace.displayP3)!.copyICCData()! as Data
        try bytes.write(to: original)
        let profile = try preferences.importProfile(original)
        _ = try preferences.importProfile(original)
        let entries = try preferences.profiles()
        try require(entries.count == 1 && entries[0]["profile"].isNull, "Deduplicate profiles and keep list payloads small")
        let id = entries[0]["id"].string
        let selected = try preferences.profile(id)
        try require(Data(selected["profile"]["Icc"].array.map { UInt8($0.uint) }) == bytes, "Reuse exact ICC bytes")
        let recipe = initial["recipe"].replacing("profile", with: profile)
        let saved = try preferences.presets(color: color, request: JSON(["type": "save", "name": "Studio", "recipe": recipe.raw]))
        try require(saved["index"].uint == 4, "Save named recipe")
        let file = locations.exportPresets
        let savedBytes = try Data(contentsOf: file)
        try rejected("Reject duplicate preset") { _ = try preferences.presets(color: color,
            request: JSON(["type": "save", "name": "Studio", "recipe": recipe.raw])) }
        try rejected("Reject unsupported ICC before publication") {
            _ = try preferences.presets(color: color, request: JSON(["type": "update", "index": 4,
                "recipe": recipe.replacing("profile", with: JSON(["profile": ["Icc": [1,2,3]], "channels": "Rgb", "name": "Broken"])).raw]))
        }
        try require(try Data(contentsOf: file) == savedBytes, "Failed presets preserve the existing file")
        let stored = locations.colorProfiles.appendingPathComponent("\(id).icc")
        try Data("Damaged copy".utf8).write(to: stored)
        try require(try !preferences.profiles()[0]["issue"].isNull, "Report damaged saved profiles")
        try rejected("Reject changed saved profile") { _ = try preferences.profile(id) }
        _ = try preferences.importProfile(original)
        try require(try Data(contentsOf: stored) == bytes, "Reimport repairs library copy")
        try preferences.removeProfile(id)
        try require(try preferences.profiles().isEmpty && Data(contentsOf: original) == bytes, "Remove only the library copy")
        let reopened = ColorPreferencesStore(locations: locations)
        let restored = try reopened.presets(color: color, request: JSON(["type": "get", "index": 4]))
        try require(restored["recipe"].stableKey == recipe.stableKey, "Named preset retains ICC after library removal and restart")
        let update = recipe.replacing("format", with: JSON("Png"))
        _ = try reopened.presets(color: color, request: JSON(["type": "update", "index": 4, "recipe": update.raw]))
        _ = try reopened.presets(color: color, request: JSON(["type": "remember", "index": 1, "recipe": update.raw]))
        _ = try reopened.presets(color: color, request: JSON(["type": "reset", "index": 1]))
        let reset = try reopened.presets(color: color, request: JSON(["type": "get", "index": 1]))
        try require(reset["recipe"]["depth"].string == "U8", "Reset destination uses shared default")
        let removed = try reopened.presets(color: color, request: JSON(["type": "remove", "index": 4]))
        try require(removed["names"].array.count == 4, "Remove named preset")
        let overflowLocations = StorageLocations.within(root.appendingPathComponent("overflow"))!
        let overflowDirectory = overflowLocations.colorProfiles
        try FileManager.default.createDirectory(at: overflowDirectory, withIntermediateDirectories: true)
        let overflow = ColorPreferencesStore(locations: overflowLocations)
        for index in 0..<129 {
            try Data([0]).write(to: overflowDirectory.appendingPathComponent(String(format: "%064x.icc", index)))
        }
        let overflowEntries = try overflow.profiles()
        try require(overflowEntries.count == 129 && overflowEntries.allSatisfy { !$0["issue"].isNull },
            "Every corrupt or over-quota entry must stay visible and removable")
        try rejected("Shared quota prevents publication") { _ = try overflow.importProfile(original) }
        for entry in overflowEntries.prefix(2) { try overflow.removeProfile(entry["id"].string) }
        _ = try overflow.importProfile(original)
        try require(try overflow.profiles().count == 128, "Removing excess entries permits import again")
        try rejected("Reject read outside the library") { _ = try overflow.profile("../Original") }
        try rejected("Reject removal outside the library") { try overflow.removeProfile("../Original") }
        try require(try Data(contentsOf: original) == bytes, "Rejected keys must preserve the original")
        print("PASS ICC library: exact import/reuse, deduplication, damaged-copy retry, safe removal, atomic named/destination presets")
        print("PASS shared ICC library: over-quota entries remain visible/removable, import resumes after cleanup and invalid keys preserve sources")
        return profile
    }
    @MainActor static func main() async throws {
        _ = NSApplication.shared
        NSApp.setActivationPolicy(.prohibited)
        precondition(StorageLocations.installation != nil, "Private installation storage must initialize")
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("capy-export-owner-\(UUID())")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let firstStore = EditorStore(platform: 0,
            persistence: EditorPersistence(root: root.appendingPathComponent("state-0")), managedWorkspaces: false)
        try await CapyTest.wait("Shared copy startup", failure: { firstStore.failure }) { !firstStore.bootstrap.isNull }
        let profile = try await io { try library(root) }
        let cmyk = try await io { try ColorPreferencesStore(locations: nil).importProfile(
            URL(fileURLWithPath: "/System/Library/ColorSync/Profiles/Generic CMYK Profile.icc")) }
        for platform: UInt32 in [0,1] {
            let directory = root.appendingPathComponent("state-\(platform)")
            let store = platform == 0 ? firstStore
                : EditorStore(platform: platform, persistence: EditorPersistence(root: directory), managedWorkspaces: false)
            let native = store.native!
            let surface = attachSurface(store, CGSize(width: 128, height: 128))
            defer { native.detach(); withExtendedLifetime(surface) {} }
            try await CapyTest.wait("Metal startup", failure: { store.failure }, step: { await frame(native) }) { store.snapshot["shaders_ready"].bool }
            let master = root.appendingPathComponent("Editable-\(platform).capy")
            var destination: URL? = master
            var chosenType: UTType?
            var staging: URL?
            var deliveries = 0
            var deliveredWhileAttached = false
            var deliveryWindow: NSWindow?
            store.projectFiles = ProjectFiles(store: store, dialogs: .init(open: { _, done in done([]) }, save: { _,type,done in
                deliveries += 1
                deliveredWhileAttached = deliveredWhileAttached || deliveryWindow?.attachedSheet != nil
                chosenType = type; done(destination)
            }, create: { _,done in done(newSheet([64, 48], space: "DisplayP3", depth: "U16", transparent: true)) },
                export: platform == 0 ? { url,done in
                    deliveries += 1
                    deliveredWhileAttached = deliveredWhileAttached || deliveryWindow?.attachedSheet != nil
                    staging = url
                    if let destination {
                        do { try FileManager.default.copyItem(at: url, to: destination); done(destination) }
                        catch { preconditionFailure("Fixture provider: \(error)") }
                    } else { done(nil) }
                } : nil))
            let dismissal = store.projectFiles.$exportEditor.sink { [weak files = store.projectFiles] editor in
                if editor == nil { files?.exportDismissed() }
            }
            defer { withExtendedLifetime(dismissal) {} }
            func invoke(_ command: String) async throws { try await store.apply(["type": "invoke", "command": command]) }
            func idle(_ label: String = "Document completion", allowError: Bool = false) async throws {
                try await wait(label, store: store) { !store.projectFiles.busy && store.state["requests"].array.isEmpty }
                if !allowError { try require(store.projectFiles.error == nil, store.projectFiles.error ?? "") }
            }
            func dialog() async throws -> ExportController {
                try await invoke("export_document")
                try await wait("Export options", store: store) { store.projectFiles.exportEditor?.loaded == true || store.projectFiles.exportEditor?.error != nil }
                let editor = store.projectFiles.exportEditor!
                try require(editor.error == nil, editor.error ?? "")
                return editor
            }
            func pauseIO() async -> DispatchSemaphore {
                let release = DispatchSemaphore(value: 0)
                await withCheckedContinuation { entered in
                    NativeProjectTask.io.async {
                        entered.resume()
                        _ = release.wait(timeout: .now() + .seconds(30))
                    }
                }
                return release
            }
            func delayedDialog() async throws -> ExportController {
                let release = await pauseIO()
                defer { release.signal() }
                try await invoke("export_document")
                try await wait("Assigned unloaded Export", store: store) { store.projectFiles.exportEditor != nil }
                let editor = store.projectFiles.exportEditor!
                try require(!editor.loaded && editor.view["dialog"].isNull, "Export is assigned before its file worker loads the dialog")
                var ownerReady = false
                let changes = store.projectFiles.objectWillChange.sink {
                    DispatchQueue.main.async { if editor.loaded && !editor.view["dialog"].isNull { ownerReady = true } }
                }
                defer { changes.cancel() }
                release.signal()
                try await wait("Export owner dialog publication", store: store) { ownerReady }
                try require(editor.error == nil, editor.error ?? "")
                return editor
            }
            func acceptedRecipe() async -> JSON {
                let id = store.state["requests"].array.first { $0["kind"]["request"]["type"].string == "export" }!["id"].uint
                return await withCheckedContinuation { done in
                    store.query(["type": "export_dialog_recipe", "id": id]) { done.resume(returning: $0) }
                }
            }
            func finished(_ editor: ExportController) async throws {
                try await wait("Export worker", store: store) { !editor.busy && (!editor.previews.isEmpty || editor.error != nil) }
                try require(editor.error == nil, editor.error ?? "")
            }
            try await invoke("new_document"); try await idle("New document completion")
            try await store.apply(["type":"color", "action":["op":"definition", "color":["space":"DisplayP3", "rgba":[0.7,0.3,0.15,0.8]]]])
            try await invoke("select_all"); try await store.apply(["type":"layer", "action":["op":"fill_selection"]]); try await invoke("deselect")
            _ = await withCheckedContinuation { done in native.flushPersistence { done.resume(returning: $0) } }
            try await invoke("save_document"); try await idle("Master save completion")
            try await invoke("add_layer") // Export must preserve unsaved state too.
            let state = store.state["document_file"].stableKey
            let original = try Data(contentsOf: master)
            func localized(_ editor: ExportController) async throws {
                let fields = editor.fields.stableKey
                let literalNames = Array(editor.view["preset_names"].array.dropFirst(4)).map(\.string)
                let profiles = editor.profiles.map(\.stableKey)
                let destination = editor.view["destination"].uint
                for (language, index, title) in [("ja", 2, "画像を書き出す"), ("en", 1, "Export image")] {
                    try await store.apply(["type": "preferences", "action": ["type": "edit", "id": "language", "value": index]])
                    try await wait("Export language", store: store) { store.interfaceLanguage == language }
                    editor.refreshCopy()
                    try await wait("Export copy projections", store: store) { !editor.busy && editor.copy["title"].string == title }
                    try require(editor.profiles.map(\.stableKey) == profiles && editor.fields.stableKey == fields
                        && editor.view["destination"].uint == destination,
                        "Retained Export relocalization preserves exact profiles, fields and selection")
                    try require(Array(editor.view["preset_names"].array.dropFirst(4)).map(\.string) == literalNames,
                        "Retained Export preserves authored preset names")
                    let size = NativeTextContext.caption(["type": "export_size", "width": 64, "height": 48], language: language)
                    try require(size.contains("64") && size.contains("48") && size.contains(language == "ja" ? "出力" : "Output"),
                        "Shared output-size copy follows the active language")
                    let number = NativeTextContext.caption(["type": "numeric_error", "reason": ["reason": "whole_number",
                        "label": ["message": "color-features-export-quality"]]], language: language)
                    try require(number.contains(editor.copy["quality"].string), "Numeric errors use the localized shared field label")
                }
            }
            var editor = try await delayedDialog()
            try await finished(editor)
            try require(editor.previews.count == 2, "Export automatically prepares its initial comparison")
            try await localized(editor)
            let accepted = editor.fields.stableKey
            editor.act(["type": "navigate", "page": "size"]); try await finished(editor)
            try require(editor.view["page"].string == "size", "Export navigation uses the retained shared page")
            editor.act(["type": "back"]); try await finished(editor)
            try require(editor.view["page"].string == "root" && editor.fields.stableKey == accepted,
                "Export Back preserves all accepted draft fields")
            editor.cancel(); try await idle("Initial Export cancellation")
            try require(store.state["document_file"].stableKey == state, "Cancel leaves unsaved master unchanged")
            let presetPath = StorageLocations.within(directory)!.exportPresets
            let unreadablePresets = Data("Preserved export preset bytes".utf8)
            try await io {
                try FileManager.default.createDirectory(at: presetPath.deletingLastPathComponent(), withIntermediateDirectories: true)
                try unreadablePresets.write(to: presetPath)
                try FileManager.default.setAttributes([.posixPermissions: 0], ofItemAtPath: presetPath.path)
            }
            try await invoke("export_document")
            try await idle("Initial preset I/O failure completion", allowError: true)
            try require(store.projectFiles.error != nil && store.projectFiles.exportEditor == nil,
                "Initial preset I/O failure reports through Files and retires Export")
            try await io {
                try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: presetPath.path)
                try require(Data(contentsOf: presetPath) == unreadablePresets, "Initial preset I/O failure preserves unreadable preset bytes")
                try FileManager.default.removeItem(at: presetPath)
            }
            store.projectFiles.dismissAlert()

            editor = try await dialog()
            editor.act(["type": "preset", "index": 2]); try await finished(editor)
            editor.change("format", JSON("Jpeg")); try await finished(editor)
            let jpegRecipe = await acceptedRecipe()
            try require(jpegRecipe["depth"].string == "U8" && jpegRecipe["background"].string == "White"
                && editor.view["depths"].array.map { $0["value"].string } == ["U8"]
                && !editor.view["backgrounds"].array.map { $0["value"].string }.contains("Preserve"), "JPEG draft supplies valid dependent values and choices")
            editor.change("encoding", (await acceptedRecipe())["encoding"].replacing("dither", with: JSON("Stochastic8")))
            try await finished(editor)
            editor.change("format", JSON("Png")); try await finished(editor)
            editor.change("depth", JSON("U16")); try await finished(editor)
            try require((await acceptedRecipe())["encoding"]["dither"].string == "None"
                && !editor.view["show_dither"].bool, "16-bit draft removes inapplicable dither")
            editor.imported(cmyk); try await finished(editor)
            let cmykRecipe = await acceptedRecipe()
            try require(cmykRecipe["format"].string == "Tiff" && cmykRecipe["background"].string == "White"
                && editor.view["formats"].array.map { $0["value"].string } == ["Tiff", "Jpeg"]
                && !editor.view["backgrounds"].array.map { $0["value"].string }.contains("Preserve"), "CMYK import supplies supported output choices")
            let cmykOutput = root.appendingPathComponent("CMYK-\(platform).tiff")
            let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 240, height: 180),
                styleMask: [.titled], backing: .buffered, defer: false)
            let sheet = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 180, height: 100),
                styleMask: [.titled], backing: .buffered, defer: false)
            window.isReleasedWhenClosed = false
            sheet.isReleasedWhenClosed = false
            defer { window.endSheet(sheet); sheet.close(); window.close() }
            store.projectFiles.presentationWindow = window
            deliveryWindow = window
            window.orderFront(nil)
            window.beginSheet(sheet, completionHandler: { _ in })
            try require(window.attachedSheet === sheet, "Owned export sheet is attached")
            let beforeDelivery = deliveries
            destination = cmykOutput
            editor.choose()
            try await wait("Export dismissal while attached", store: store) { store.projectFiles.exportEditor == nil }
            try require(deliveries == beforeDelivery && store.projectFiles.busy,
                "Export delivery waits for native sheet detachment")
            window.endSheet(sheet)
            try await idle("CMYK sheet-detached delivery")
            try require(window.attachedSheet == nil && !deliveredWhileAttached && deliveries == beforeDelivery + 1,
                "Native sheet detachment delivers exactly once")
            store.projectFiles.exportDismissed()
            try require(deliveries == beforeDelivery + 1, "Repeated dismissal cannot deliver again")
            deliveryWindow = nil
            store.projectFiles.presentationWindow = nil
            window.orderOut(nil)
            try image(cmykOutput, type: .tiff, depth: 16, extent: [64,48])
            let cmykSource = CGImageSourceCreateWithURL(cmykOutput as CFURL, nil)!
            try require(CGImageSourceCreateImageAtIndex(cmykSource, 0, nil)?.colorSpace?.model == .cmyk,
                "The shared draft must produce an actual CMYK delivery copy")
            editor = try await dialog()
            editor.change("quality", JSON(0)); editor.preview()
            try await wait("Invalid output", store: store) { !editor.view["error"].isNull }
            try require(editor.error == nil && store.failure == nil, "Invalid export drafts stay in the shared form without a failed recipe query")
            editor.act(["type": "preset", "index": 2]); try await finished(editor)
            editor.imported(profile); try await finished(editor)
            let recipe = (await acceptedRecipe()).replacing("size", with: JSON(["Fit": ["bounds": [32,32], "enlarge": false]]))
                .replacing("resolution", with: JSON(["Ppi": 240]))
            var publishedExtents: [[UInt64]] = []
            let publication = editor.$previews.dropFirst().sink { images in
                if !images.isEmpty { publishedExtents.append(editor.details["output_extent"].array.map(\.uint)) }
            }
            let release = await pauseIO()
            defer { release.signal() }
            editor.preview()
            editor.act(["type": "fit", "enabled": true]) {
                editor.act(["type": "bound", "axis": 0, "value": 32])
                editor.act(["type": "bound", "axis": 1, "value": 32]); editor.change("resolution", recipe["resolution"])
            }
            try await wait("Queued latest preview", store: store) {
                !editor.busy && editor.view["bounds"][0].uint == 32 && editor.view["bounds"][1].uint == 32
                    && editor.fields["resolution"].stableKey == recipe["resolution"].stableKey
            }
            var draftError: String?
            do {
                _ = try NativeTextContext.number(editor.view["numeric"]["dimension"], value: 0,
                    operation: ["type": "expression", "text": "1 +"], language: store.interfaceLanguage)
            } catch { draftError = error.localizedDescription }
            guard let draftError else { throw HostFailure(message: "The incomplete native expression must fail validation") }
            editor.error = draftError
            release.signal()
            try await wait("Latest automatic preview", store: store) { !editor.busy && !publishedExtents.isEmpty }
            try require(editor.error == draftError, "Automatic successor preview preserves native draft validation errors")
            publication.cancel()
            try require(publishedExtents.allSatisfy { $0 == [32,24] }, "Edits queued during preview publish only the latest output extent")
            try require(editor.previews.count == 2 && editor.details["output_extent"][0].uint == 32
                && editor.details["output_extent"][1].uint == 24, "Complete before/output comparison and fit")
            editor.preset("save"); try await finished(editor)
            editor.act(["type": "name", "value": "Delivery"]); try await finished(editor)
            editor.storePreset(); try await finished(editor)
            let preferences = store.colorPreferences
            let color = JSON(["space": "DisplayP3", "depth": "U16"])
            let previousCustom = try await io { try preferences.presets(color: color, request: JSON(["type":"get", "index":3])) }
            destination = nil; editor.choose(); try await idle("Named output destination cancellation")
            let remembered = try await io { try preferences.presets(color: color, request: JSON(["type":"get", "index":3])) }
            try require(remembered["recipe"].stableKey == previousCustom["recipe"].stableKey, "Cancelled destination preserves the previous Custom output choices")
            editor = try await dialog()
            editor.act(["type": "preset", "index": 4]); try await finished(editor)
            try require((await acceptedRecipe()).stableKey == recipe.stableKey, "Reuse named output after cancel")
            try await localized(editor)
            let tiff = root.appendingPathComponent("Output-\(platform).tiff")
            destination = tiff; editor.choose(); try await idle("Named TIFF delivery")
            try image(tiff, type: .tiff, depth: 16, extent: [32,24])
            if platform == 1 { try require(chosenType == .tiff, "NSSavePanel receives TIFF type") }
            else {
                try await io { }
                try require(staging?.pathExtension == "tiff" && !FileManager.default.fileExists(atPath: staging!.path), "Provider staging type and cleanup")
            }
            let persisted = try await io { try ColorPreferencesStore(locations: StorageLocations.within(directory)).presets(color: color, request: JSON(["type":"get", "index":3])) }
            try require(persisted["recipe"].stableKey == recipe.stableKey, "Successful named output remembers Custom, preserving named preset")
            for (format,type) in [("Png",UTType.png),("Jpeg",UTType.jpeg)] {
                editor = try await dialog()
                let output = root.appendingPathComponent("Output-\(platform)." + type.preferredFilenameExtension!)
                destination = output
                let value = (await acceptedRecipe()).replacing("format", with: JSON(format)).replacing("background", with: JSON("White"))
                editor.change("format", value["format"]); editor.change("background", value["background"])
                editor.choose(); try await idle("\(format) delivery")
                try image(output, type:type, depth:8, extent:[64,48])
            }
            editor = try await dialog(); editor.preview(); editor.cancel(); try await idle("Active preview cancellation")
            if platform == 1 {
                editor = try await dialog(); destination = root.appendingPathComponent("Missing/output.png")
                editor.choose(); try await idle("Missing-directory delivery failure", allowError:true)
                try require(store.projectFiles.error != nil, "Write failure is visible")
                store.projectFiles.error = nil
                editor = try await dialog(); destination = root.appendingPathComponent("Retry.png")
                editor.change("format", JSON("Png")); editor.choose(); try await idle("PNG retry delivery")
                try image(destination!, type:.png, depth:8, extent:[64,48])
            }
            let expectedState = try JSON.decode(state).replacing("export_uri", with: JSON(destination!.absoluteString))
            let currentState = store.state["document_file"]
            let masterBytesUnchanged = try Data(contentsOf: master) == original
            if currentState.stableKey != expectedState.stableKey || !masterBytesUnchanged {
                let failure = JSON(["expected_document_file": expectedState.raw, "actual_document_file": currentState.raw,
                    "master_bytes_unchanged": masterBytesUnchanged, "commands": store.state["commands"].raw])
                FileHandle.standardError.write(Data(("Export master state: " + failure.stableKey + "\n").utf8))
            }
            try require(currentState.stableKey == expectedState.stableKey && masterBytesUnchanged,
                "All output operations preserve master bytes, location and dirty state, and remember the last successful export")
            print("PASS platform \(platform): PNG/TIFF/JPEG, comparison, profile/resize/16-bit output, named presets, picker cancellation, retry and master preservation")
        }
    }
}
