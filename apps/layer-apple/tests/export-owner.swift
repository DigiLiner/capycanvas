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
            func idle(allowError: Bool = false) async throws {
                try await wait("Document completion", store: store) { !store.projectFiles.busy && store.state["requests"].array.isEmpty }
                if !allowError { try require(store.projectFiles.error == nil, store.projectFiles.error ?? "") }
            }
            func dialog() async throws -> ExportController {
                try await invoke("export_document")
                try await wait("Export options", store: store) { store.projectFiles.exportEditor?.loaded == true || store.projectFiles.exportEditor?.error != nil }
                let editor = store.projectFiles.exportEditor!
                try require(editor.error == nil, editor.error ?? "")
                return editor
            }
            func finished(_ editor: ExportController) async throws {
                try await wait("Export worker", store: store) { !editor.busy }
                try require(editor.error == nil, editor.error ?? "")
            }
            try await invoke("new_document"); try await idle()
            try await store.apply(["type":"color", "action":["op":"definition", "color":["space":"DisplayP3", "rgba":[0.7,0.3,0.15,0.8]]]])
            try await invoke("select_all"); try await store.apply(["type":"layer", "action":["op":"fill_selection"]]); try await invoke("deselect")
            _ = await withCheckedContinuation { done in native.flushPersistence { done.resume(returning: $0) } }
            try await invoke("save_document"); try await idle()
            try await invoke("add_layer") // Export must preserve unsaved state too.
            let state = store.state["document_file"].stableKey
            let original = try Data(contentsOf: master)
            func localized(_ editor: ExportController) async throws {
                let localizedRecipe = editor.recipe.stableKey
                let localizedDraft = editor.draft.stableKey
                let literalNames = Array(editor.names.dropFirst(4))
                let localizedProfiles = editor.profiles.map(\.stableKey)
                let localizedSelection = (editor.destination, editor.profileIndex)
                editor.invalidNumber("color-features-export-quality")
                for (language, index, title) in [("ja", 2, "画像を書き出す"), ("en", 1, "Export image")] {
                    try await store.apply(["type": "preferences", "action": ["type": "edit", "id": "language", "value": index]])
                    try await wait("Export language", store: store) { store.interfaceLanguage == language }
                    editor.refreshCopy()
                    try await wait("Export copy projections", store: store) {
                        editor.names.first == store.catalog["export_copy"]["web_share"].string
                            && editor.metadataCopy["label"].string == store.catalog["export_copy"]["metadata"].string
                        && editor.outputSize.contains(language == "ja" ? "出力" : "Output")
                    }
                    try require(editor.copy["title"].string == title
                        && editor.copy.stableKey == store.catalog["export_copy"].stableKey,
                        "Open export controller consumes the current shared catalog")
                    try require(!editor.copy["outside_range"].string.isEmpty
                        && !editor.copy["preview_gainmap"].string.isEmpty
                        && !editor.copy["jpeg_gainmap"].string.isEmpty
                        && !editor.copy["avif_gainmap"].string.isEmpty,
                        "Shared preview, HDR option and warning copy is available")
                    try require(editor.profiles.map(\.stableKey) == localizedProfiles
                        && editor.destination == localizedSelection.0 && editor.profileIndex == localizedSelection.1,
                        "Locale projection preserves exact profile backing and selection")
                    try require(editor.draft.stableKey == localizedDraft && Array(editor.names.dropFirst(4)) == literalNames,
                        "Locale projection preserves the exact draft and literal preset names")
                    let size = NativeTextContext.caption(["type": "export_size", "width": 64, "height": 48], language: language)
                    try require(size.contains("64") && size.contains("48") && size.contains(language == "ja" ? "出力" : "Output"),
                        "Output size caption follows the active language")
                    let number = NativeTextContext.caption(["type": "numeric_error", "reason": ["reason": "whole_number",
                        "label": ["message": "color-features-export-quality"]]], language: language)
                    try await wait("Export number error", store: store) { editor.error == number }
                    try require(number.contains(editor.copy["quality"].string), "Whole-number error resolves its shared field label")
                    try require(editor.recipe.stableKey == localizedRecipe,
                        "Changing export language preserves every recipe value")
                }
                editor.invalidNumber("color-features-export-quality")
                editor.invalidate()
                await withCheckedContinuation { (done: CheckedContinuation<Void, Never>) in
                    native.submit(2, JSON(["type": "catalog"])) { _ in
                        DispatchQueue.main.async { done.resume() }
                    }
                }
                try require(editor.error == nil, "Cleared numeric errors cannot return from an older copy reply")
            }
            var editor = try await dialog()
            try await localized(editor)
            editor.cancel(); try await idle()
            try require(store.state["document_file"].stableKey == state, "Cancel leaves unsaved master unchanged")
            editor = try await dialog()
            editor.preference(JSON(["type": "get", "index": 2])); try await finished(editor)
            editor.change("format", JSON("Jpeg")); try await finished(editor)
            try require(editor.recipe["depth"].string == "U8" && editor.recipe["background"].string == "White"
                && editor.draft["depths"].array.map(\.string) == ["U8"]
                && !editor.draft["backgrounds"].array.map(\.string).contains("Preserve"), "JPEG draft supplies valid dependent values and choices")
            editor.change("encoding", editor.recipe["encoding"].replacing("dither", with: JSON("Stochastic8")))
            try await finished(editor)
            editor.change("format", JSON("Png")); try await finished(editor)
            editor.change("depth", JSON("U16")); try await finished(editor)
            try require(editor.recipe["encoding"]["dither"].string == "None"
                && editor.draft["dithers"].array.map(\.string) == ["None"], "16-bit draft removes inapplicable dither")
            editor.imported(cmyk); try await finished(editor)
            try require(editor.recipe["format"].string == "Tiff" && editor.recipe["background"].string == "White"
                && editor.draft["formats"].array.map(\.string) == ["Tiff", "Jpeg"]
                && !editor.draft["backgrounds"].array.map(\.string).contains("Preserve"), "CMYK import supplies supported output choices")
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
            editor.choose(editor.recipe)
            try await wait("Export dismissal while attached", store: store) { store.projectFiles.exportEditor == nil }
            try require(deliveries == beforeDelivery && store.projectFiles.busy,
                "Export delivery waits for native sheet detachment")
            window.endSheet(sheet)
            try await idle()
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
            editor.preview(editor.recipe.replacing("jpeg_quality", with: JSON(0)))
            try await wait("Invalid output", store: store) { editor.error != nil }
            editor.preference(JSON(["type": "get", "index": 2])); try await finished(editor)
            editor.imported(profile); try await finished(editor)
            let recipe = editor.recipe.replacing("size", with: JSON(["Fit": ["bounds": [32,32], "enlarge": false]]))
                .replacing("resolution", with: JSON(["Ppi": 240]))
            editor.preview(recipe); try await finished(editor)
            try require(editor.previews.count == 2 && editor.details["output_extent"][0].uint == 32
                && editor.details["output_extent"][1].uint == 24, "Complete before/output comparison and fit")
            editor.preference(JSON(["type": "save", "name": "Delivery", "recipe": recipe.raw])); try await finished(editor)
            let preferences = store.colorPreferences
            let color = JSON(["space": "DisplayP3", "depth": "U16"])
            let previousCustom = try await io { try preferences.presets(color: color, request: JSON(["type":"get", "index":3])) }
            destination = nil; editor.choose(editor.recipe); try await idle()
            let remembered = try await io { try preferences.presets(color: color, request: JSON(["type":"get", "index":3])) }
            try require(remembered["recipe"].stableKey == previousCustom["recipe"].stableKey, "Cancelled destination preserves the previous Custom output choices")
            editor = try await dialog()
            editor.preference(JSON(["type":"get", "index":4])); try await finished(editor)
            try require(editor.recipe.stableKey == recipe.stableKey, "Reuse named output after cancel")
            try await localized(editor)
            let tiff = root.appendingPathComponent("Output-\(platform).tiff")
            destination = tiff; editor.choose(editor.recipe); try await idle()
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
                let value = editor.recipe.replacing("format", with: JSON(format)).replacing("background", with: JSON("White"))
                editor.choose(value); try await idle()
                try image(output, type:type, depth:8, extent:[64,48])
            }
            editor = try await dialog(); editor.preview(editor.recipe); editor.cancel(); try await idle()
            if platform == 1 {
                editor = try await dialog(); destination = root.appendingPathComponent("Missing/output.png")
                editor.choose(editor.recipe); try await idle(allowError:true)
                try require(store.projectFiles.error != nil, "Write failure is visible")
                store.projectFiles.error = nil
                editor = try await dialog(); destination = root.appendingPathComponent("Retry.png")
                editor.choose(editor.recipe.replacing("format", with: JSON("Png"))); try await idle()
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
