import AppKit
import QuartzCore

@main struct CanvasReadinessChecks {
    @MainActor static func main() async throws {
        _ = NSApplication.shared
        NSApp.setActivationPolicy(.prohibited)
        for platform: UInt32 in [0, 1] {
            let store = EditorStore(platform: platform, persistence: EditorPersistence(root: nil), managedWorkspaces: false)
            try await wait("Platform \(platform) bootstrap", failure: { store.failure }) { !store.bootstrap.isNull }
            try require(!store.snapshot["canvas_ready"].bool, "An unattached canvas must not have document shaders ready")
            store.canvasSubmitted = true
            let view = MacCanvasView(store: store)
            try require(!store.canvasReady && view.accessibilityValue() as? String == store.bootstrap["starting_canvas"].string,
                "Submitting the backdrop must not announce document readiness")
            let surface = attachSurface(store, CGSize(width: 64, height: 64))
            try await wait("Platform \(platform) document readiness", seconds: 120, failure: { store.failure },
                step: { await frame(store.native!) }) { store.snapshot["canvas_ready"].bool }
            try require(store.canvasSubmitted && store.canvasReady,
                "Startup publication must preserve submission and make the canvas ready")
            try require(view.accessibilityValue() as? String == store.bootstrap["canvas_ready"].string,
                "Startup publication must update native accessibility without another submission or input")
            store.native!.detach()
            await frame(store.native!)
            view.stop()
            withExtendedLifetime(surface) {}
            print("PASS: platform \(platform) backdrop stays preparing; document startup updates native readiness")
        }
        try require(NSApp.windows.isEmpty, "Readiness checks must not create windows")
    }
}
