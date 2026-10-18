import Foundation
import QuartzCore

/// Production document coordinator, owner queues and durable palette/settings
/// storage in disposable sessions. UI delivery is a separate XCTest workflow.
@main struct ColorWorkflowChecks {
    @MainActor static func main() async throws {
        for platform: UInt32 in [0, 1] {
            let root = FileManager.default.temporaryDirectory.appendingPathComponent("capy-color-workflows-\(UUID())")
            defer { try? FileManager.default.removeItem(at: root) }
            let scene = UUID().uuidString
            let store = EditorStore(platform: platform, scene: scene, persistence: EditorPersistence(root: root))
            try await wait("Workspace startup") { store.workspaces?.ready == true || store.failure != nil }
            try require(store.failure == nil, store.failure ?? "")
            let surface = attachSurface(store, CGSize(width: 128, height: 128))
            try await wait("Metal startup", failure: { store.failure }, step: {
                try await prepare(store.native!, "Metal startup")
                await frame(store.native!)
            }) { store.snapshot["shaders_ready"].bool }
            let files = store.projectFiles
            let epoch = store.state["document_file"]["epoch"].uint
            try await store.apply(["type": "invoke", "command": "new_document"])
            try await wait("New drawing dialog") { files.creating && !store.state["new_drawing"].isNull }
            for step in newSheet([63, 47], space: "DisplayP3", depth: "U16", transparent: true) { try await files.newDrawingStep(step) }
            for step: [String: Any] in [["op": "presets"], ["op": "save_as"], ["op": "preset_name", "text": " "]] { try await files.newDrawingStep(step) }
            try require(!store.state["new_drawing"]["drawer"]["save"]["can_save"].bool, "A blank preset name cannot be saved")
            var refused = false
            do { try await files.newDrawingStep(["op": "save_preset"]) } catch { refused = true }
            try require(refused && files.creating && store.state["document_file"]["epoch"].uint == epoch
                && store.state["settings"]["new_document"]["setups"].array.isEmpty, "A refused preset must retain the dialog, drawing and presets")
            for step: [String: Any] in [["op": "preset_name", "text": " Studio P3 "], ["op": "save_preset"]] { try await files.newDrawingStep(step) }
            try require(store.state["settings"]["new_document"]["setups"][0]["name"].string == "Studio P3", "Save the trimmed preset name")
            for step: [String: Any] in [["op": "use_preset"], ["op": "create"]] { try await files.newDrawingStep(step) }
            try await wait("New drawing") { store.state["document_file"]["epoch"].uint == epoch + 1 && !files.busy }
            try require(store.state["settings"]["new_document"]["recent"][0]["id"].string == "Studio P3", "Create remembers the preset it used")
            try require(store.state["colors"]["rgb_space"].string == "DisplayP3", "New must adopt the selected working space")
            try await store.apply(["type": "invoke", "command": "new_document"])
            try await wait("Reopened dialog") { files.creating && !store.state["new_drawing"].isNull }
            try require(store.state["new_drawing"]["page"]["preset"].string == "Studio P3", "Reopen with the last preset")
            try await files.newDrawingStep(["op": "cancel"])
            try await wait("New cancellation") { !files.busy }
            try require(store.state["document_file"]["epoch"].uint == epoch + 1, "Cancel must preserve the drawing")

            func color(_ action: [String: Any]) async throws { try await store.apply(["type": "color", "action": action]) }
            func library(_ action: [String: Any]) async throws { try await color(["op": "library", "action": action]) }
            let starting = store.state["color_library"]["palettes"].array.count
            try await library(["op": "create_palette", "name": "Studio colors"])
            let palette = store.state["color_library"]["palettes"].array.first { $0["name"].string == "Studio colors" }!["id"].uint
            func saved(_ owner: EditorStore) -> JSON {
                owner.state["color_library"]["palettes"].array.first { $0["id"].uint == palette } ?? JSON()
            }
            for space in ["Srgb", "DisplayP3", "AdobeRgb", "ProPhoto"] {
                let definition = JSON(["space": space, "rgba": [1.125, -0.125, 0.34567891, 213.0 / 65535]])
                try await color(["op": "set_slot", "slot": "foreground", "color": definition.raw])
                let tagged = store.state["colors"]["foreground"]
                try await library(["op": "store", "palette": palette, "name": space, "color": tagged.raw])
                let swatch = saved(store)["swatches"].array.last!
                try require(swatch["color"].stableKey == tagged.stableKey, "Store must retain extended values and precise alpha in \(space)")
                try await library(["op": "rename", "id": swatch["id"].uint, "name": "Saved " + space])
                try await color(["op": "set_slot", "slot": "foreground", "color": ["space": "Srgb", "rgba": [0, 0, 0, 1]]])
                try await library(["op": "use", "id": swatch["id"].uint])
                try require(store.state["colors"]["foreground"].stableKey == tagged.stableKey, "Use must restore exact tagged values in \(space)")
            }
            try await library(["op": "rename_palette", "id": palette, "name": "Retained colors"])
            let savedLibrary = store.state["color_library"].stableKey
            // A new document changes wheel space but retains workspace palettes.
            try await store.apply(["type": "invoke", "command": "new_document"])
            try await wait("Next document dialog") { files.creating && !store.state["new_drawing"].isNull }
            for step in newSheet([67, 43], space: "ProPhoto", depth: "U16") + [["op": "create"]] { try await files.newDrawingStep(step) }
            try await wait("Next document") { store.state["document_file"]["epoch"].uint == epoch + 2 && !files.busy }
            try require(store.state["color_library"].stableKey == savedLibrary && store.state["colors"]["rgb_space"].string == "ProPhoto",
                "Document adoption must preserve palettes while updating the wheel gamut")
            let settings = store.state["settings"]["new_document"].stableKey
            let flushed = await withCheckedContinuation { done in store.flushPersistence { done.resume(returning: $0) } }
            try require(flushed, "Settings and palettes must flush durably")
            try await store.workspaces!.closed()
            let restored = EditorStore(platform: platform, scene: scene, persistence: EditorPersistence(root: root))
            try await wait("Fresh owner restoration") { restored.workspaces?.ready == true || restored.failure != nil }
            try require(restored.state["color_library"].stableKey == savedLibrary, "Fresh owner must restore palette IDs, names, tags and samples")
            try require(restored.state["settings"]["new_document"].stableKey == settings, "Fresh owner must restore New drawing presets")
            let removal = saved(restored)["swatches"][0]["id"].uint
            for action: [String: Any] in [["op": "remove", "id": removal], ["op": "remove_palette", "id": palette]] {
                try await restored.apply(["type": "color", "action": ["op": "library", "action": action]])
            }
            try require(restored.state["color_library"]["palettes"].array.count == starting, "Swatch and palette removal must preserve the remaining palettes")
            try await restored.workspaces!.closed()
            print("PASS: platform \(platform), New drawing presets/validation/Cancel, all tagged palette spaces, document adoption and fresh-owner durable restoration")
            withExtendedLifetime(surface) {}
        }
    }
}
