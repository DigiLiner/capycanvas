import UIKit

// Run as a standalone UIKit application with the production Shared/iOS sources
// and native bridge. Supplied recognizer states test callback delivery to Rust;
// physical Pencil recognition and rendered cursor appearance remain separate.
private final class Hover: UIHoverGestureRecognizer {
    var phase: UIGestureRecognizer.State = .began
    var point = CGPoint(x: 200, y: 150)
    override var state: UIGestureRecognizer.State { get { phase } set { phase = newValue } }
    override func location(in view: UIView?) -> CGPoint { point }
    override var altitudeAngle: CGFloat { .pi / 3 }
    override func azimuthAngle(in view: UIView?) -> CGFloat { .pi / 4 }
    override var rollAngle: CGFloat { 0.2 }
    override var zOffset: CGFloat { 0.5 }
}

@MainActor private final class HoverChecks: NSObject, UIApplicationDelegate, UIWindowSceneDelegate {
    func application(_ application: UIApplication, configurationForConnecting session: UISceneSession,
        options: UIScene.ConnectionOptions) -> UISceneConfiguration {
        let configuration = UISceneConfiguration(name: "Canvas hover", sessionRole: session.role)
        configuration.delegateClass = HoverChecks.self
        return configuration
    }
    func application(_ application: UIApplication,
        didFinishLaunchingWithOptions options: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
        Task { @MainActor in
            do { try await run(); print("PASS: UIKit hover exit, cancellation and next-hover recovery"); exit(0) }
            catch { print("FAIL: \(error.localizedDescription)"); exit(1) }
        }
        return true
    }
    private func run() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("capy-hover-\(UUID())")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        // Reuse the existing optional input recorder without a test-only hook.
        setenv("CAPY_TRACE_DIRECTORY", root.path, 1)
        setenv("CAPY_TRACE_SECONDS", "120", 1)
        setenv("CAPY_TRACE_GPU", "0", 1)
        let store = EditorStore(platform: 0, persistence: EditorPersistence(root: nil))
        let canvas = CanvasView(store: store)
        canvas.frame = CGRect(x: 0, y: 0, width: 600, height: 450)
        canvas.contentScaleFactor = 2
        defer { canvas.stop() }
        guard let native = store.native else { throw HostFailure(message: "No owner") }
        func flush() async {
            await withCheckedContinuation { continuation in
                native.submit(2, JSON(["type": "catalog"])) { _ in
                    DispatchQueue.main.async { continuation.resume() }
                }
            }
        }
        let surface = CAMetalLayer(); surface.bounds = CGRect(x: 0, y: 0, width: 1200, height: 900)
        native.attach(surface, width: 1200, height: 900, scale: 1)
        defer { native.detach(); withExtendedLifetime(surface) {} }
        func frame() async {
            let now = FrameTrace.now()
            await withCheckedContinuation { done in
                native.frame(now: now, target: now + 16_666_667) { _, _, _ in done.resume() }
            }
            await flush()
        }
        let ready = Date().addingTimeInterval(30)
        while !store.snapshot["shaders_ready"].bool {
            guard Date() < ready && store.failure == nil else {
                throw HostFailure(message: store.failure ?? "Hover picker canvas startup")
            }
            await frame(); try await Task.sleep(for: .milliseconds(10))
        }
        store.invoke("fit_canvas"); await flush()
        let colors = store.state["colors"].stableKey
        store.invoke("eyedropper"); await flush()
        func expectPreview(_ visible: Bool) async throws {
            let deadline = Date().addingTimeInterval(8)
            repeat {
                await frame()
                if store.colorPreviewing == visible { break }
                guard Date() < deadline && store.failure == nil else {
                    throw HostFailure(message: store.failure ?? "Hover picker preview expected \(visible), received \(store.colorPreviewing)")
                }
                try await Task.sleep(for: .milliseconds(10))
            } while true
            guard ["pick_visible", "pick_layer"].contains(store.state["layer_tools"]["tool"].string),
                store.state["colors"].stableKey == colors,
                !store.command("undo")["enabled"].bool && !store.command("redo")["enabled"].bool else {
                throw HostFailure(message: "Hover/leave must retain the picker, colors and empty artwork history")
            }
        }
        try await expectPreview(false)
        let hover = Hover()
        let viewport = store.state["camera"]["viewport"]
        hover.point = CGPoint(x: CGFloat(viewport[0].number) / (2 * canvas.contentScaleFactor),
            y: CGFloat(viewport[1].number) / (2 * canvas.contentScaleFactor))
        for phase: UIGestureRecognizer.State in [.began, .changed, .ended, .began, .cancelled] {
            hover.phase = phase; canvas.hovered(hover); await flush()
            try await expectPreview(phase == .began || phase == .changed)
        }
        // A hover recognizer must not overwrite an active Pencil contact.
        let contact = UITouch()
        canvas.contacts[ObjectIdentifier(contact)] = PencilContact(id: 1, tool: 0, button: 0)
        hover.phase = .changed; canvas.hovered(hover); await flush()
        try await expectPreview(false)
        canvas.contacts.removeAll()
        for phase: UIGestureRecognizer.State in [.began, .ended] {
            hover.phase = phase; canvas.hovered(hover); await flush()
            try await expectPreview(phase == .began)
        }
        await flush()
        native.finishTrace()
        if let failure = store.failure { throw HostFailure(message: failure) }
        let deadline = Date().addingTimeInterval(8)
        var trace: URL?
        while trace == nil && Date() < deadline {
            trace = try FileManager.default.contentsOfDirectory(at: root, includingPropertiesForKeys: nil)
                .first { $0.pathExtension == "jsonl" }
            if trace == nil { try await Task.sleep(for: .milliseconds(20)) }
        }
        guard let trace else { throw HostFailure(message: "Input recorder did not finish") }
        let lines = try String(contentsOf: trace, encoding: .utf8).split(separator: "\n")
        let inputs = try lines.dropFirst().compactMap { line -> [UInt64]? in
            let values = try JSONSerialization.jsonObject(with: Data(line.utf8)) as! [NSNumber]
            return values[0].uint64Value == 2 ? values.map(\.uint64Value) : nil
        }
        guard inputs.map({ $0[8] }) == [0, 0, 0, 0] else {
            throw HostFailure(message: "Only began/changed callbacks reach the pointer recorder; exits use cursor_leave and active Pencil overlap stays suppressed; received phases \(inputs.map { $0[8] })")
        }
        guard inputs.allSatisfy({ $0[6] == 1 && $0[7] == 0 && $0[9] == 0 && $0[10] == 1 }) else {
            throw HostFailure(message: "Each hover must reach Rust as one accepted real pen sample")
        }
        guard !store.command("undo")["enabled"].bool && !store.command("redo")["enabled"].bool else {
            throw HostFailure(message: "Hover must preserve empty artwork history")
        }
    }
}

@main struct CanvasHoverChecks {
    static func main() {
        UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(HoverChecks.self))
    }
}
