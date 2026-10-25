import Foundation
import Darwin
import CryptoKit
import AppKit
import QuartzCore
import ImageIO

@main struct ProjectFileChecks {
    @MainActor static var prepareFrame: (() async throws -> Void)?
    @MainActor static func wait(_ description: String, _ condition: () -> Bool) async throws {
        try await CapyTest.wait(description, step: { if let prepareFrame { try await prepareFrame() } }, condition)
    }
    @MainActor static func startupFrame(_ store: EditorStore) async throws {
        // No display link drives this fixture. Prepare document work without a
        // drawable, then run the host's bundled-catalog/startup completion path.
        let prepared = await withCheckedContinuation { continuation in
            store.native!.flushPersistence { continuation.resume(returning: $0) }
        }
        precondition(prepared, "Offscreen startup preparation failed")
        let now = FrameTrace.now()
        await withCheckedContinuation { continuation in
            store.native!.frame(now: now, target: now + 16_666_667) { _, _, _ in continuation.resume() }
        }
        try await Task.sleep(for: .milliseconds(10))
    }
    @MainActor static func main() async throws {
        if CommandLine.arguments.dropFirst().first == "--grant-lifecycle" {
            try await persistedGrantLifecycle()
            return
        }
        _ = NSApplication.shared
        NSApp.setActivationPolicy(.prohibited)
        precondition(StorageLocations.installation != nil, "Private installation storage must initialize")
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("capy-files-\(UUID())")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
        defer { try? FileManager.default.removeItem(at: root) }
        let atomic = root.appendingPathComponent("atomic")
        try Data([1,2,3]).write(to: atomic)
        do {
            try ProjectFileIO.atomicWrite(to: atomic) { fd in
                _ = [UInt8](repeating: 7, count: 16).withUnsafeBytes { Darwin.write(fd, $0.baseAddress, $0.count) }
                throw HostFailure(message: "Injected write failure")
            }
            preconditionFailure("Failed write must throw")
        } catch {}
        precondition((try? Data(contentsOf: atomic)) == Data([1,2,3]))
        do {
            try ProjectFileIO.atomicWrite(to: atomic, beforeCommit: { throw HostFailure(message: "Cancelled") }) { fd in
                _ = [UInt8](repeating: 9, count: 16).withUnsafeBytes { Darwin.write(fd, $0.baseAddress, $0.count) }
            }
            preconditionFailure("Cancelled publication must throw")
        } catch {}
        precondition((try? Data(contentsOf: atomic)) == Data([1,2,3]))
        precondition((try? FileManager.default.contentsOfDirectory(atPath: root.path)) == ["atomic"])
        for platform: UInt32 in [0, 1] {
            let store = EditorStore(platform: platform,
                persistence: EditorPersistence(root: root.appendingPathComponent("files-\(platform)")), managedWorkspaces: false)
            prepareFrame = { try await startupFrame(store) }
            defer { prepareFrame = nil }
            let target = root.appendingPathComponent("drawing-\(platform).capy")
            let invalid = root.appendingPathComponent("invalid-\(platform).capy")
            try Data("incomplete".utf8).write(to: invalid)
            var saveLocation: URL? = target
            var openLocation: URL? = target
            var choices = 0
            var creationOptions: [[String: Any]]? = newSheet([63, 47], space: "Srgb", depth: "U8")
            var beforeOpen: (() -> Void)?
            store.projectFiles = ProjectFiles(store: store, dialogs: .init(
                open: { _, done in choices += 1; beforeOpen?(); done(openLocation.map { [$0] } ?? []) },
                save: { [weak store] name, _, callback in
                    if let request = store?.state["requests"].array.first(where: { $0["kind"]["request"]["type"].string == "save" }) {
                        precondition(name == request["kind"]["request"]["name"].string, "Save picker must preserve the shared suggested filename")
                    }
                    choices += 1; callback(saveLocation)
                },
                create: { dialog, callback in
                    precondition(!dialog["untitled"].string.isEmpty && dialog["drawer"].isNull && dialog["can_create"].bool)
                    callback(creationOptions)
                },
                export: platform == 0 ? { staging, callback in
                    choices += 1
                    if let url = saveLocation {
                        do { try FileManager.default.copyItem(at: staging, to: url); callback(url) }
                        catch { preconditionFailure("Provider copy failed: \(error)") }
                    } else { callback(nil) }
                } : nil, exportOptions: { $0.choose($0.recipe) }))
            let layer = attachSurface(store, CGSize(width: 128, height: 128))
            let startupDeadline = Date().addingTimeInterval(45)
            while !store.snapshot["shaders_ready"].bool && store.failure == nil {
                precondition(Date() < startupDeadline, "Native canvas startup did not finish")
                try await startupFrame(store)
            }
            precondition(store.failure == nil, store.failure ?? "Attach failed")
            func invoke(_ command: String) async throws {
                store.invoke(command)
                try await wait("Document operation did not finish: \(command)") {
                    !store.projectFiles.busy && store.state["requests"].array.isEmpty
                }
                // Ensure the owner has actually received the action before
                // checking its completion, rather than observing old idle state.
                _ = await withCheckedContinuation { continuation in
                    store.native!.submit(2, JSON(["type":"catalog"])) { continuation.resume(returning: $0 != nil) }
                }
                try await wait("Document operation did not settle: \(command)") { !store.projectFiles.busy && store.state["requests"].array.isEmpty }
            }
            func language(_ tag: String) async throws {
                let index = store.bootstrap["shipped_tags"].array.firstIndex(where: { $0.string == tag })!
                try await store.apply(["type": "preferences", "action": ["type": "edit", "id": "language", "value": index + 1]])
                try await wait("Document language did not refresh") { store.interfaceLanguage == tag }
            }
            try await language("ja")
            try await wait("Untitled drawing title did not refresh") {
                store.projectFiles.title == "無題" && store.projectFiles.title == store.state["tabs"][0]["title"].string
            }
            try await invoke("save_document_as")
            try await language("en")
            precondition(store.projectFiles.error == nil, store.projectFiles.error ?? "")
            precondition(FileManager.default.fileExists(atPath: target.path))
            precondition(store.state["document_file"]["location"]["name"].string == target.lastPathComponent)
            let mode = try FileManager.default.attributesOfItem(atPath: target.path)[.posixPermissions] as! NSNumber
            precondition(mode.intValue & 0o777 == 0o600)
            store.invoke("add_layer")
            try await wait("Layer edit not applied") { store.state["layers"].array.count == 3 }
            let id = store.state["layer_tools"]["editing_layer"]["id"].uint
            store.layer(["op":"rename", "id":id, "name":"Saved layer"])
            try await wait("Rename not applied") { store.state["layers"].array.contains { $0["label"].string == "Saved layer" } }
            try await invoke("save_document")
            precondition(!store.state["document_file"]["modified"].bool)
            var savedData = try Data(contentsOf: target)
            // A file can launch the app before its first state publication or
            // before the canvas attaches. Retain it through normal startup.
            for (published, stored) in [(false, true), (true, true), (false, false)] {
                let cold = EditorStore(platform: platform, persistence: EditorPersistence(root:
                    stored ? root.appendingPathComponent("startup-\(platform)-\(published)") : nil))
                cold.projectFiles = ProjectFiles(store: cold, dialogs: .init(
                    open: { _, _ in preconditionFailure("An external URL must not open a picker") },
                    save: { _, _, _ in preconditionFailure("Startup must not save a blank drawing") }))
                if published {
                    try await wait("Initial editor state missing") { !cold.state.isNull }
                    try await wait("Workspace must initialize before Metal in this ordering") { cold.workspaces?.ready == true }
                } else { precondition(cold.state.isNull) }
                precondition(!cold.snapshot["gpu_ready"].bool)
                cold.projectFiles.openURL(target)
                if let error = cold.projectFiles.error {
                    throw HostFailure(message: "Startup Open rejected the supplied file: \(error)")
                }
                let surface = CAMetalLayer(); surface.bounds = layer.bounds
                cold.native!.attach(surface, width: 128, height: 128, scale: 1)
                if published {
                    // Metal attachment publishes an enabled Open command before
                    // first-frame bundled-filter validation has even started.
                    // Drain the owner/UI handoff while retaining that ordering.
                    for _ in 0..<2 {
                        _ = await withCheckedContinuation { continuation in
                            cold.native!.submit(2, JSON(["type": "catalog"])) { continuation.resume(returning: $0 != nil) }
                        }
                    }
                    precondition(cold.snapshot["gpu_ready"].bool && !cold.snapshot["shaders_ready"].bool)
                    precondition(!cold.projectFiles.busy && cold.state["requests"].array.isEmpty
                        && cold.state["document_file"]["epoch"].uint == 0,
                        "External Open must wait for first-frame startup, not just Metal attachment")
                }
                // Drive the actual first-frame/catalog sequence. A recovery
                // flush alone omits the startup publication that can race Open.
                let deadline = Date().addingTimeInterval(45)
                while cold.state["document_file"]["epoch"].uint != 1 || cold.projectFiles.busy {
                    if cold.projectFiles.error != nil || cold.failure != nil { break }
                    precondition(Date() < deadline, "Startup Open did not finish")
                    try await startupFrame(cold)
                }
                if let error = cold.projectFiles.error ?? cold.failure {
                    throw HostFailure(message: "Startup Open failed after preparation: \(error)")
                }
                precondition(cold.state["document_file"]["location"]["name"].string == target.lastPathComponent)
                precondition(cold.state["layers"].array.contains { $0["label"].string == "Saved layer" })
                precondition(!cold.state["document_file"]["modified"].bool)
                precondition(stored ? cold.workspaces?.ready == true : cold.workspaces == nil)
                await cold.workspaces?.detached()
                withExtendedLifetime(surface) {}
            }
            print("Startup Open passes for Apple platform \(platform)")
            // Sequential external delivery opens independent drawings without
            // a location picker. Concurrent delivery is covered by drawing-tabs.swift.
            for _ in 0..<2 {
                let epoch = store.state["document_file"]["epoch"].uint
                let originalChoices = choices
                store.projectFiles.openURL(target)
                try await wait("External Open did not settle") {
                    store.state["document_file"]["epoch"].uint == epoch + 1 && !store.projectFiles.busy
                }
                precondition(store.state["document_file"]["location"]["name"].string == target.lastPathComponent)
                precondition(choices == originalChoices && store.failure == nil && store.projectFiles.error == nil)
            }
            // A URL arriving alongside a cancelled New request waits for the
            // first file operation, then opens its own drawing.
            let extent = creationOptions
            creationOptions = nil
            let queuedEpoch = store.state["document_file"]["epoch"].uint
            store.invoke("new_document")
            store.projectFiles.openURL(target)
            let queuedDeadline = Date().addingTimeInterval(45)
            while store.state["document_file"]["epoch"].uint != queuedEpoch + 1 || store.projectFiles.busy {
                guard Date() < queuedDeadline else { throw HostFailure(message: "Queued Open: epoch=\(store.state["document_file"]["epoch"].uint), expected=\(queuedEpoch + 1), busy=\(store.projectFiles.busy), ready=\(store.snapshot["shaders_ready"].bool), error=\(store.projectFiles.error ?? store.failure ?? "none")") }
                try await startupFrame(store)
            }
            precondition(store.projectFiles.error == nil && store.failure == nil)
            creationOptions = extent
            print("External Open admission passes for Apple platform \(platform)")
            saveLocation = nil
            try await invoke("save_document_as")
            precondition((try? Data(contentsOf: target)) == savedData)
            let choicesAfterCancellation = choices
            let savedLayerName = "Saved after cancelling Save As"
            store.layer(["op":"rename", "id":id, "name":savedLayerName])
            try await wait("Rename after cancelled Save As") {
                store.state["layers"].array.contains { $0["label"].string == savedLayerName }
            }
            try await invoke("save_document")
            precondition(choices == choicesAfterCancellation && store.projectFiles.error == nil,
                "Save after cancelling Save As must retain the original destination without another picker")
            precondition(!store.state["document_file"]["modified"].bool)
            savedData = try Data(contentsOf: target)
            store.invoke("add_layer")
            try await wait("Unsaved edit not applied") { store.state["layers"].array.count == 4 }
            let epoch = store.state["document_file"]["epoch"].uint
            let newOptions = creationOptions
            creationOptions = nil
            try await invoke("new_document")
            precondition(store.state["document_file"]["epoch"].uint == epoch && store.state["layers"].array.count == 4)
            creationOptions = newOptions
            try await invoke("new_document")
            precondition(store.projectFiles.error == nil, store.projectFiles.error ?? "")
            precondition(store.state["layers"].array.count == 2 && store.state["document_file"]["epoch"].uint == epoch + 1)
            let createdEpoch = store.state["document_file"]["epoch"].uint
            creationOptions = nil
            try await invoke("new_document")
            precondition(store.state["document_file"]["epoch"].uint == createdEpoch, "Cancelled size choice must keep the drawing")
            let png = root.appendingPathComponent("export-\(platform).png")
            saveLocation = png
            let projectLocation = store.state["document_file"]["location"].raw
            try await invoke("export_document")
            precondition(store.projectFiles.error == nil, store.projectFiles.error ?? "")
            let source = CGImageSourceCreateWithURL(png as CFURL, nil)!
            let image = CGImageSourceCreateImageAtIndex(source, 0, nil)!
            precondition(image.width == 63 && image.height == 47)
            precondition(!store.state["document_file"]["modified"].bool)
            precondition(store.state["document_file"]["location"].isNull && projectLocation is NSNull)
            let exported = try Data(contentsOf: png)
            saveLocation = nil
            try await invoke("export_document")
            precondition((try? Data(contentsOf: png)) == exported, "Cancelled PNG destination must preserve the existing file")
            openLocation = invalid
            try await invoke("open_document")
            precondition(store.projectFiles.error != nil)
            precondition(store.state["layers"].array.count == 2)
            store.projectFiles.error = nil
            store.invoke("add_layer")
            try await wait("Unsaved failed-open fixture") { store.state["layers"].array.count == 3 }
            func layerStateWithoutPreviewRevisions(_ rows: JSON) -> String {
                JSON(rows.array.map { row in
                    var fields = row.object
                    fields.removeValue(forKey: "paint_revision")
                    fields.removeValue(forKey: "mask_revision")
                    return fields
                }).stableKey
            }
            func reportLayerMismatch(_ expected: JSON) {
                let state = JSON(["expected_layers": expected.raw, "actual_layers": store.state["layers"].raw,
                    "commands": store.state["commands"].raw, "document_file": store.state["document_file"].raw])
                FileHandle.standardError.write(Data(("Failed Open history state: " + state.stableKey + "\n").utf8))
            }
            let unsavedRows = store.state["layers"]
            let unsavedLayers = unsavedRows.stableKey
            let unsavedContent = layerStateWithoutPreviewRevisions(unsavedRows)
            let unsavedFile = store.state["document_file"].stableKey
            store.invoke("open_document")
            try await wait("Invalid replacement must finish with an error") {
                !store.projectFiles.busy && store.projectFiles.error != nil && store.state["requests"].array.isEmpty
            }
            precondition(store.state["layers"].stableKey == unsavedLayers
                && store.state["document_file"].stableKey == unsavedFile,
                "Failed Open must retain the original unsaved tab")
            store.projectFiles.error = nil
            store.invoke("undo")
            try await wait("Failed Open must retain Undo") { store.state["layers"].array.count == 2 }
            store.invoke("redo")
            do {
                try await wait("Failed Open must retain Redo") { layerStateWithoutPreviewRevisions(store.state["layers"]) == unsavedContent }
            } catch {
                reportLayerMismatch(unsavedRows)
                throw error
            }
            openLocation = nil
            try await invoke("open_document")
            let cancelledLayersMatch = layerStateWithoutPreviewRevisions(store.state["layers"]) == unsavedContent
            if !cancelledLayersMatch { reportLayerMismatch(unsavedRows) }
            precondition(store.state["document_file"]["modified"].bool && cancelledLayersMatch)
            store.invoke("undo")
            try await wait("Restore the clean fixture before the queued-edit check") {
                store.state["layers"].array.count == 2 && !store.state["document_file"]["modified"].bool
            }
            print("PASS platform \(platform): failed Open preserves unsaved drawing, history and retry cancellation")
            store.projectFiles.error = nil; openLocation = target
            beforeOpen = { store.invoke("add_layer") }
            try await invoke("open_document")
            precondition(store.projectFiles.error != nil, "An edit queued before capture must prevent replacement")
            precondition(store.state["layers"].array.count == 3)
            beforeOpen = nil; store.projectFiles.error = nil
            try await invoke("open_document")
            precondition(store.projectFiles.error == nil, store.projectFiles.error ?? "")
            precondition(store.state["layers"].array.contains { $0["label"].string == savedLayerName })
            precondition(!store.state["document_file"]["modified"].bool)
            openLocation = png
            try await invoke("open_document")
            precondition(store.state["document_file"]["location"].isNull)
            precondition(!store.state["document_file"]["unsaved_name"].string.isEmpty)
            precondition(store.projectFiles.title == store.state["document_file"]["unsaved_name"].string)
            saveLocation = nil
            try await invoke("save_document_as")
            precondition(store.state["document_file"]["location"].isNull, "Cancelled Save must retain the imported photo name")
            openLocation = target; saveLocation = target
            try await invoke("open_document")
            precondition(store.state["layers"].array.contains { $0["label"].string == savedLayerName })
            store.invoke("add_layer")
            var closed: Bool?
            store.projectFiles.confirmCurrentClose { closed = $0 }
            try await wait("Close prompt missing") { store.projectFiles.confirming }
            let closeRequest = store.state["requests"].array.first(where: { $0["kind"]["request"]["type"].string == "confirm_close" })!
            precondition(store.projectFiles.alertTitle == closeRequest["kind"]["request"]["title"].string)
            let englishClose = store.projectFiles.alertTitle
            try await language("ja")
            try await wait("Pending close title did not refresh") { store.projectFiles.alertTitle != englishClose }
            let translatedClose = store.state["requests"].array.first(where: { $0["id"].uint == closeRequest["id"].uint })!
            precondition(store.projectFiles.alertTitle == translatedClose["kind"]["request"]["title"].string)
            try await language("en")
            store.projectFiles.choose("cancel")
            try await wait("Close cancellation did not settle") { closed != nil }
            precondition(closed == false)
            closed = nil
            store.projectFiles.confirmCurrentClose { closed = $0 }
            try await wait("Save on close prompt missing") { store.projectFiles.confirming }
            store.projectFiles.choose("save")
            try await wait("Save on close did not finish") { closed != nil }
            precondition(closed == true && !store.state["document_file"]["modified"].bool)
            withExtendedLifetime(layer) {}
            print("Project files pass for Apple platform \(platform)")
        }
    }
    @MainActor static func persistedGrantLifecycle() async throws {
        let arguments = CommandLine.arguments
        try require(arguments.count == 7, "Expected phase, original, picked bookmark, scene UUID and platform")
        let phase = arguments[2], original = URL(fileURLWithPath: arguments[3])
        let bookmark = URL(fileURLWithPath: arguments[4]), scene = arguments[5]
        guard let platform = UInt32(arguments[6]), [0, 1].contains(platform), UUID(uuidString: scene) != nil else {
            throw HostFailure(message: "Invalid private lifecycle identity")
        }
        if phase == "prepare" {
            try Data("owned Save As placeholder".utf8).write(to: original)
            try original.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil).write(to: bookmark)
            return
        }
        try require(["create", "original", "moved", "reject"].contains(phase), "Unknown lifecycle phase")
        let root = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("capy-native-grant-\(scene)-\(platform)", isDirectory: true)
        let temporary = root.appendingPathComponent("tmp", isDirectory: true)
        try FileManager.default.createDirectory(at: temporary, withIntermediateDirectories: true)
        setenv("CAPY_STORAGE_DIR", root.path, 1); setenv("TMPDIR", temporary.path, 1)
        FileHandle.standardError.write(Data(("LIFECYCLE_CONTEXT " + (try JSON(["state": root.path, "phase": phase,
            "scene": scene, "platform": platform]).encoded()) + "\n").utf8))
        _ = NSApplication.shared
        NSApp.setActivationPolicy(.prohibited)
        let persistence = EditorPersistence(root: root)
        let store = EditorStore(platform: platform, scene: scene, persistence: persistence, managedWorkspaces: false)
        let native = store.native!, surface = attachSurface(store, CGSize(width: 256, height: 192))
        defer { native.detach(); withExtendedLifetime(surface) {} }
        var pickers = 0, exported: Data?, exportFailure: String?, exportStage = "idle"
        store.projectFiles = ProjectFiles(store: store, dialogs: .init(open: { _, _ in
            preconditionFailure("Recovery must not reopen an Open picker")
        }, save: { _, _, done in
            pickers += 1
            guard phase == "create" else { done(nil); return }
            do {
                var stale = false
                let url = try URL(resolvingBookmarkData: Data(contentsOf: bookmark), options: [.withoutUI, .withoutImplicitStartAccessing],
                    relativeTo: nil, bookmarkDataIsStale: &stale)
                done(url)
            } catch { preconditionFailure("Picked test grant: \(error)") }
        }, export: { url, done in
            if url.pathExtension == "capy" {
                pickers += 1
                FileHandle.standardError.write(Data("Grant provider package: phase=\(phase), picked=\(pickers)\n".utf8))
                guard phase == "create" else { done(nil); return }
                Task { @MainActor in
                    do {
                        let destination = try await Task.detached(priority: .utility) {
                            var stale = false
                            let destination = try URL(resolvingBookmarkData: Data(contentsOf: bookmark),
                                options: [.withoutUI, .withoutImplicitStartAccessing], relativeTo: nil, bookmarkDataIsStale: &stale)
                            try ProjectFileIO.coordinate(url, writing: false) { source in
                                let input = try FileHandle(forReadingFrom: source)
                                defer { try? input.close() }
                                try ProjectFileIO.coordinate(destination, writing: true) { target in
                                    try ProjectFileIO.atomicWrite(to: target) { descriptor in
                                        guard Darwin.fcopyfile(input.fileDescriptor, descriptor, nil, copyfile_flags_t(COPYFILE_DATA)) == 0 else {
                                            throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno))
                                        }
                                    }
                                }
                            }
                            return destination
                        }.value
                        done(destination)
                    } catch { preconditionFailure("Picked test grant provider delivery: \(error)") }
                }
                return
            }
            exportStage = "delivery"; print("Grant export: \(exportStage)")
            Task { @MainActor in
                do { exported = try await Task.detached { try decodedExportPixels(url) }.value }
                catch { exportFailure = error.localizedDescription }
                exportStage = "decoded"; print("Grant export: \(exportStage)")
                done(nil)
            }
        }, exportOptions: {
            exportStage = "options"; print("Grant export: \(exportStage)")
            $0.choose($0.recipe.replacing("format", with: JSON("Png")))
        }))
        func wait(_ label: String, _ ready: () -> Bool) async throws {
            try await CapyTest.wait(label, seconds: 60, failure: { store.failure ?? store.recovery.error },
                step: { await frame(native) }, ready)
        }
        func invoke(_ command: String) async throws {
            try await store.apply(["type": "invoke", "command": command])
            try await wait(command) { !store.projectFiles.busy && store.state["requests"].array.isEmpty }
        }
        func pixels() async throws -> Data {
            exported = nil; exportFailure = nil
            exportStage = "requested"; print("Grant export: \(exportStage)")
            try await invoke("export_document")
            try await CapyTest.wait("Grant PNG export (\(exportStage))", seconds: 60,
                failure: { store.projectFiles.error ?? store.failure ?? store.recovery.error },
                step: { await frame(native) }) { exported != nil || exportFailure != nil }
            guard let exported else { throw HostFailure(message: exportFailure ?? "Missing production PNG export") }
            return exported
        }
        func layers() -> String {
            JSON(store.state["layers"].array.map {
                $0.replacing("paint_revision", with: JSON(0)).replacing("mask_revision", with: JSON(0)).raw
            }).stableKey
        }
        func fill(_ rgba: [Double]) async throws {
            try await store.apply(["type": "color", "action": ["op": "definition", "color": ["space": "Srgb", "rgba": rgba]]])
            try await invoke("select_all")
            try await store.apply(["type": "layer", "action": ["op": "fill_selection"]])
        }
        func destination(_ uri: String) async throws -> URL {
            let result = await withCheckedContinuation { done in native.savedDestination(uri) { done.resume(returning: $0) } }
            guard let result else { throw HostFailure(message: "NativeOwner must resolve the persisted file grant") }
            return result
        }
        func grants() async throws -> [String: Data] {
            let directory = persistence.locations!.state.appendingPathComponent("FileAccess")
            return try await Task.detached(priority: .utility) { try storedTestFiles(directory) }.value
        }
        func grantHashes(_ files: [String: Data]) -> String {
            JSON(files.mapValues { data in SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined() }).stableKey
        }
        let marker = root.appendingPathComponent("lifecycle.json")
        try await wait("Native lifecycle restoration") { store.snapshot["shaders_ready"].bool && !store.recovery.restoring }
        try require(store.recovery.restoreError == nil && store.recovery.failedDrawings.isEmpty, "Saved artwork must restore without failure")
        var baseline: Data, undoPixels: Data
        var expectedGrantBytes: [String: Data]?
        if phase == "create" {
            try await invoke("add_layer")
            try await fill([0.13, 0.41, 0.79, 1])
            baseline = try await pixels()
            try await invoke("undo"); undoPixels = try await pixels()
            try require(undoPixels != baseline, "The fixture must contain real painted pixels and meaningful Undo")
            try await invoke("redo")
            try require(try await pixels() == baseline, "Redo must restore exact authored pixels")
            try await invoke("save_document_as")
            try await wait("Picked Save As completion") {
                !store.projectFiles.busy && store.state["requests"].array.isEmpty
                    && (store.projectFiles.error != nil || (pickers == 1 && !store.state["document_file"]["modified"].bool
                        && store.state["document_file"]["location"]["uri"].string == original.absoluteString))
            }
            try require(pickers == 1 && store.projectFiles.error == nil, "Save As must use the picked file grant exactly once")
        } else {
            let expected = try JSON.decode(String(contentsOf: marker, encoding: .utf8))
            try require(expected["process"].string != String(ProcessInfo.processInfo.processIdentifier), "Each lifecycle phase must use a fresh process")
            baseline = try Data(contentsOf: root.appendingPathComponent("pixels.bin"))
            undoPixels = try Data(contentsOf: root.appendingPathComponent("undo-pixels.bin"))
            let restoredPixels = try await pixels()
            try require(layers() == expected["layers"].string && restoredPixels == baseline,
                "Fresh NativeOwner/ArtworkRecovery must restore exact layers and painted pixels")
            try await invoke("undo"); try require(try await pixels() == undoPixels, "Restart must preserve exact Undo pixels")
            try await invoke("redo"); try require(try await pixels() == baseline, "Restart must preserve exact Redo pixels")
            try require(store.state["document_file"]["location"]["uri"].string == expected["uri"].string,
                "Recovery must retain the saved destination URI until a new save succeeds")
            let target = try await destination(expected["uri"].string)
            FileHandle.standardError.write(Data(("GRANT_DESTINATION " + (try JSON(["phase": phase,
                "saved_uri": expected["uri"].string, "original_uri": original.absoluteString,
                "resolved_uri": target.absoluteString]).encoded()) + "\n").utf8))
            if phase == "moved" {
                try require(expected["uri"].string == original.absoluteString && target.standardizedFileURL != original.standardizedFileURL,
                    "Moved-file scenarios must validate the original saved URI against its relocated grant")
            }
            let before = try ProjectFileIO.coordinate(target, writing: false) { try Data(contentsOf: $0) }
            if phase == "reject" {
                try require(expected["uri"].string == original.absoluteString && target.standardizedFileURL == original.standardizedFileURL,
                    "The ABA fixture must observe the foreign file at the saved original URI")
                try require(!expected["grants"].object.isEmpty, "The preceding validated Save must retain its exact grant bytes")
                expectedGrantBytes = try expected["grants"].object.mapValues { value in
                    guard let text = value as? String, let bytes = Data(base64Encoded: text) else {
                        throw HostFailure(message: "Invalid private grant baseline")
                    }
                    return bytes
                }
                try require(store.state["document_file"]["modified"].bool && !store.state["document_file"]["recovered"].bool,
                    "A changed original must be modified while the prior clean exit retains its recovery classification")
                let identity = store.drawingIdentity, authored = layers()
                try await invoke("save_document")
                try await wait("Original fingerprint refusal") {
                    !store.projectFiles.busy && store.state["requests"].array.isEmpty && store.projectFiles.error != nil
                }
                try require(pickers == 0 && store.projectFiles.error != nil, "A changed original must refuse direct Save without silently opening a picker")
                let refusal = "The saved drawing changed outside Capy Canvas. Save a copy to keep both versions."
                try require(store.projectFiles.error == refusal,
                    "Normal Save must reach the shared destination fingerprint refusal: \(store.projectFiles.error ?? "nil")")
                try require(try ProjectFileIO.coordinate(target, writing: false) { try Data(contentsOf: $0) } == before,
                    "Fingerprint refusal must leave the foreign original bytes intact")
                store.projectFiles.dismissAlert()
                let refusedPixels = try await pixels()
                try require(store.drawingIdentity == identity && layers() == authored && refusedPixels == baseline,
                    "A refused Save must preserve live artwork and its owner")
            } else {
                try require(!store.state["document_file"]["modified"].bool && !store.state["document_file"]["recovered"].bool,
                    "Persisted grant observation must recognize the unchanged original, including after its move")
                if phase == "moved" { try require(target.standardizedFileURL != original.standardizedFileURL, "The bookmark must follow the moved file") }
                undoPixels = baseline
                try await fill(phase == "original" ? [0.73, 0.21, 0.37, 1] : [0.23, 0.69, 0.31, 1])
                baseline = try await pixels()
                try require(baseline != undoPixels, "Each lazy Save must contain a new authored paint edit")
                try await invoke("save_document")
                try await wait("Lazy Save completion") {
                    !store.projectFiles.busy && store.state["requests"].array.isEmpty
                        && (store.projectFiles.error != nil || (!store.state["document_file"]["modified"].bool
                            && store.state["document_file"]["location"]["uri"].string == target.absoluteString))
                }
                try require(pickers == 0 && store.projectFiles.error == nil && !store.state["document_file"]["modified"].bool,
                    "Fresh ProjectFiles must lazily Save through the persisted grant without a picker")
                try require(store.state["document_file"]["location"]["uri"].string == target.absoluteString,
                    "Successful moved-file Save must publish its resolved destination")
                try require(try ProjectFileIO.coordinate(target, writing: false) { try Data(contentsOf: $0) } != before,
                    "Lazy Save must replace the actual original with the accepted edit")
            }
        }
        let copied = await withCheckedContinuation { done in store.recovery.flush(cleanExit: true) { done.resume(returning: $0) } }
        try require(copied && store.recovery.hasCurrentCopy, "The accepted artwork/history must be durable before process exit")
        let savedGrants = try await grants()
        if let expectedGrantBytes {
            FileHandle.standardError.write(Data("GRANT_PRESERVATION expected=\(grantHashes(expectedGrantBytes)) actual=\(grantHashes(savedGrants))\n".utf8))
            try require(savedGrants == expectedGrantBytes, "Rejected foreign Save must preserve every original grant byte")
        }
        if phase != "reject" {
            let savedURI = store.state["document_file"]["location"]["uri"].string
            try require(try savedGrants.values.contains { try JSON.decode(String(decoding: $0, as: UTF8.self))["uri"].string == savedURI },
                "A validated Save must persist access for its accepted destination URI")
            try baseline.write(to: root.appendingPathComponent("pixels.bin")); try undoPixels.write(to: root.appendingPathComponent("undo-pixels.bin"))
            try AtomicJSONFile.write(try JSON(["layers": layers(), "uri": store.state["document_file"]["location"]["uri"].string,
                "process": String(ProcessInfo.processInfo.processIdentifier),
                "grants": savedGrants.mapValues { $0.base64EncodedString() }]).encoded().data(using: .utf8)!, to: marker)
        }
        print("PASS platform \(platform) phase \(phase): NativeOwner/ArtworkRecovery/ProjectFiles grant, exact pixels/history and destination safety")
    }
}
