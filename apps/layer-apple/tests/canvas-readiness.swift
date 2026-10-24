import AppKit
import QuartzCore
import SwiftUI

@main final class CanvasReadinessChecks: NativeWorkspaceInputFixture {
    @MainActor static func run() async throws {
        _ = NSApplication.shared
        NSApp.setActivationPolicy(.prohibited)
        for platform: UInt32 in [0, 1] {
            let store = EditorStore(platform: platform, persistence: EditorPersistence(root: nil), managedWorkspaces: false)
            try await wait("Platform \(platform) bootstrap", failure: { store.failure }) { !store.bootstrap.isNull }
            try require(!store.snapshot["canvas_ready"].bool, "An unattached canvas must not have document shaders ready")
            store.canvasSubmitted = true
            try require(!store.canvasReady,
                "Submitting the backdrop must not announce document readiness")
            let surface = attachSurface(store, CGSize(width: 64, height: 64))
            try await wait("Platform \(platform) document readiness", seconds: 120, failure: { store.failure },
                step: { await frame(store.native!) }) { store.snapshot["canvas_ready"].bool }
            try require(store.canvasSubmitted && store.canvasReady,
                "Startup publication must preserve submission and make the canvas ready")
            store.native!.detach()
            await frame(store.native!)
            withExtendedLifetime(surface) {}
            print("PASS: platform \(platform) backdrop stays preparing; document startup updates native readiness")
        }
        try require(NSApp.windows.isEmpty, "Readiness checks must not create windows")
        NSApp.setActivationPolicy(.accessory)
        for platform: UInt32 in [0, 1] { try await deferredMountedReadiness(platform) }
    }
    @MainActor static func canvas(_ view: NSView) -> MacCanvasView? {
        if let canvas = view as? MacCanvasView { return canvas }
        return view.subviews.lazy.compactMap(canvas).first
    }
    @MainActor static func deferredMountedReadiness(_ platform: UInt32) async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("deferred-readiness-\(UUID())")
        defer { try? FileManager.default.removeItem(at: root) }
        let store = EditorStore(platform: platform, persistence: EditorPersistence(root: root), deferLaunch: true)
        let window = NSWindow(contentRect: CGRect(x: 80, y: 60, width: 1200, height: 870),
            styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        let host = NSHostingView(rootView: EditorView(store: store) { MacMetalCanvas(store: store) }.frame(width: 1200, height: 870))
        window.contentView = host; window.makeKeyAndOrderFront(nil); NSApp.activate(ignoringOtherApps: true)
        defer { window.contentView = nil; window.close() }
        try await wait("Mounted canvas", failure: { store.failure }) { canvas(host) != nil }
        let mounted = canvas(host)!
        let callback = store.bootstrapChanged
        var notifications = 0
        store.bootstrapChanged = { notifications += 1; callback?() }
        try await wait("Deferred mounted canvas readiness", seconds: 120, failure: { store.failure }) { store.canvasReady }
        try await drain(0.05)
        let view = canvas(host)!
        let automatic = view.accessibilityValue() as? String
        let before = notifications
        let current = store.bootstrapChanged
        var explicitCalls = 0
        store.bootstrapChanged = { explicitCalls += 1; current?() }
        let activeWake = store.wake
        var wakeCalls = 0
        store.wake = { wakeCalls += 1; activeWake?() }
        let activeInterrupt = store.interruptInput
        var interruptions = 0
        store.interruptInput = { interruptions += 1; activeInterrupt?() }
        let probe = MacCanvasView(store: store)
        store.bootstrapChanged?()
        store.wake?()
        let callbacksOwned = explicitCalls == 1 && wakeCalls == 1
        probe.stop()
        try await drain(0.05)
        let explicit = view.accessibilityValue() as? String
        let expected = store.bootstrap["canvas_ready"].string
        withExtendedLifetime(probe) {}
        note("Deferred platform \(platform): ready=\(store.canvasReady) replaced=\(mounted !== view) attached=\(view.window === window) notifications=\(before) explicitCalls=\(explicitCalls) wakeCalls=\(wakeCalls) automatic=\(automatic ?? "nil") explicit=\(explicit ?? "nil") expected=\(expected)")
        try require(callbacksOwned, "An unmounted canvas probe must not replace the mounted canvas callbacks")
        try require(store.canvasSubmitted && store.canvasReady && interruptions == 0,
            "Stopping an unmounted canvas must preserve the live canvas submission and input ownership")
        try require(automatic == expected,
            "Deferred mounted readiness must update native accessibility automatically; explicit callback result: \(explicit ?? "nil")")
    }
    @MainActor static func main() {
        _ = NSApplication.shared
        Task { @MainActor in
            do { try await run(); exit(0) }
            catch { note("FAIL: \(error)"); exit(1) }
        }
        NSApp.run()
    }
}
