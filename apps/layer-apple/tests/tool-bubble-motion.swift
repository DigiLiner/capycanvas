import AppKit
import SwiftUI
import QuartzCore

@main final class ToolBubbleMotionChecks: NativeWorkspaceInputFixture {
    @MainActor static func bubbleMarker(_ view: NSView) -> ReorderInputView? {
        if let marker = view as? ReorderInputView, marker.model?.bubbleInput != nil { return marker }
        return view.subviews.lazy.compactMap(bubbleMarker).first
    }
    @MainActor static func run() async throws {
        let environment = ProcessInfo.processInfo.environment
        let started = CACurrentMediaTime()
        guard let photo = environment["CAPY_BUBBLE_MOTION_PHOTO"], let output = environment["CAPY_BUBBLE_MOTION_OUTPUT"] else {
            throw HostFailure(message: "Set the reference photo and owned motion output directory")
        }
        let directory = URL(fileURLWithPath: output, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let root = directory.appendingPathComponent("storage-\(UUID())")
        defer { try? FileManager.default.removeItem(at: root) }
        let store = EditorStore(platform: 0, persistence: EditorPersistence(root: root))
        try await wait("Initial models", failure: { store.failure }) { !store.state.isNull }
        try await store.workspaces!.started()
        let theme = environment["CAPY_BUBBLE_MOTION_THEME"] ?? "light"
        try require(["light", "dark"].contains(theme), "Choose a light or dark motion theme")
        try await store.apply(["type": "set_theme", "theme": theme])
        let window = NSWindow(contentRect: CGRect(x: 40, y: 60, width: 1200, height: 870),
            styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false; window.animationBehavior = .none
        let host = NSHostingView(rootView: EditorView(store: store) { MacMetalCanvas(store: store) }.frame(width: 1200, height: 870))
        window.contentView = host; window.makeKeyAndOrderFront(nil); NSApp.activate(ignoringOtherApps: true)
        defer { window.contentView = nil; window.close() }
        try await wait("Metal canvas", failure: { store.failure }) { store.canvasReady }
        store.projectFiles = ProjectFiles(store: store, dialogs: .init(open: { _, done in done([]) }, save: { _, _, done in done(nil) }))
        let epoch = store.state["document_file"]["epoch"].uint
        store.projectFiles.openURL(URL(fileURLWithPath: photo))
        try await wait("Reference photo", failure: { store.failure }) { store.state["document_file"]["epoch"].uint != epoch && !store.projectFiles.busy }
        try await store.apply(["type": "invoke", "command": "add_layer"])
        try await store.apply(["type": "invoke", "command": "fit_canvas"])
        let tab = store.state["tabs"].array.first { $0["active"].bool }, paint = store.state["layers"].array.first
        try require(tab?["width"].uint == 9504 && tab?["height"].uint == 6336
            && store.state["layers"].array.count == 3 && paint?["selected"].bool == true
            && paint?["editing"].bool == true && paint?["drawing"].bool == true && paint?["object"].bool == false,
            "The reference photo needs an empty selected paint layer above it")
        try await store.workspaces!.perform(["type": "switch", "id": "builtin:workspace:painter"])
        try await drain(1)
        guard let marker = bubbleMarker(host) else { throw HostFailure(message: "No native workspace capture") }
        let bubbles = store.bubbles
        func settle() async throws {
            try await wait("Bubble motion settles", failure: { store.failure }) { !bubbles.needsFrame && bubbles.items.allSatisfy { $0.placement.positioned } }
        }
        let eligible = store.snapshot["header"]["items"].array.filter { $0["tears_bubble"].bool }
        guard let color = eligible.first(where: { $0["resolved_control"]["kind"].string == "color" }),
            let layers = eligible.first(where: { $0["resolved_control"]["kind"].string == "panel" && $0["resolved_control"]["panel"].string == "layers" }) else {
            throw HostFailure(message: "The motion fixture needs Color and Layers sources")
        }
        let tools = eligible.filter { $0["id"].uint != color["id"].uint && $0["id"].uint != layers["id"].uint }
        let sources = Array(tools.prefix(3)) + [layers, color]
        try require(sources.count == 5, "The motion fixture needs five shared header sources")
        for (index, source) in sources.enumerated() {
            for phase in ["down", "up"] {
                try await store.apply(["type": "bubble", "action": ["type": "drag", "grab": ["kind": "header", "id": source["id"].raw],
                    "phase": phase, "position": [380 + index * 100, 420], "viewport": [1200, 870]]])
            }
            try await settle()
        }
        for item in bubbles.items where item.view["kind"].string == "layers" {
            try await store.apply(["type": "bubble", "action": ["type": "close", "id": item.id]])
        }
        try await settle()
        try require(bubbles.items.count == 5, "Five bubble owners must survive setup")
        try await drain(8)
        let baseline = store.state["workspace"]["layout"]["bubbles"].stableKey
        var windows: [[String: Any]] = []
        var witnesses: [[String: Any]] = []
        for gesture in 0..<4 {
            let item = bubbles.items.first!, start = item.placement.center
            let neighbors = bubbles.items.dropFirst().map { $0.placement.center }
            let last = neighbors.last!
            let endPoint = CGPoint(x: min(1100, max(100, last.x + (last.x > 600 ? -100 : 100))),
                y: min(770, max(100, last.y + 90)))
            let path = [start] + neighbors + [endPoint]
            try event(.leftMouseDown, at: start, marker: marker, number: 100 + gesture)
            try await drain(0.02)
            let begin = CACurrentMediaTime()
            var samples = 0, movement = 0.0, previous = start, pointer = start
            var previousCenters = Dictionary(uniqueKeysWithValues: bubbles.items.map { ($0.id, $0.placement.center) })
            var distances = Dictionary(uniqueKeysWithValues: bubbles.items.map { ($0.id, 0.0) })
            var poses: [[String: Any]] = []
            var stationary: [[String: UInt64]] = []
            var firstMovement: TimeInterval?, lastMovement: TimeInterval?
            var stationaryBegin: TimeInterval? = begin
            var previousPoseTime = begin
            repeat {
                let elapsed = CACurrentMediaTime() - begin
                let progress = min(Double(path.count - 1), elapsed / 6 * Double(path.count - 1))
                let index = min(path.count - 2, Int(progress)), fraction = progress - Double(index)
                let point = CGPoint(x: path[index].x + (path[index + 1].x - path[index].x) * fraction,
                    y: path[index].y + (path[index + 1].y - path[index].y) * fraction)
                pointer = point
                try event(.leftMouseDragged, at: point, marker: marker, number: 100 + gesture)
                movement += hypot(item.placement.center.x - previous.x, item.placement.center.y - previous.y)
                previous = item.placement.center; samples += 1
                var moving = false
                var owners: [[String: Any]] = []
                for neighbor in bubbles.items {
                    let center = neighbor.placement.center, before = previousCenters[neighbor.id]!
                    let distance = hypot(center.x - before.x, center.y - before.y)
                    distances[neighbor.id]! += distance; moving = moving || distance > 0
                    previousCenters[neighbor.id] = center
                    owners.append(["id": neighbor.id, "center": [center.x, center.y]])
                }
                let observed = CACurrentMediaTime()
                poses.append(["time_ns": UInt64(observed * 1e9), "moving": moving, "owners": owners])
                if moving {
                    firstMovement = firstMovement ?? observed; lastMovement = observed
                    if let stationaryBegin { stationary.append(["begin_ns": UInt64(stationaryBegin * 1e9), "end_ns": UInt64(observed * 1e9)]) }
                    stationaryBegin = nil
                } else if stationaryBegin == nil { stationaryBegin = previousPoseTime }
                previousPoseTime = observed
                try await drain(0.004)
            } while CACurrentMediaTime() - begin < 6
            let end = CACurrentMediaTime()
            try event(.leftMouseUp, at: pointer, marker: marker, number: 100 + gesture)
            try require(movement > 400 && samples > 200, "Native capture must visibly move the retained bubble throughout the gesture")
            try require(distances.values.allSatisfy { $0 > 8 }, "Every bubble must visibly move during each gesture")
            guard let firstMovement, let lastMovement else { throw HostFailure(message: "The gesture needs timestamped owner movement") }
            if let stationaryBegin { stationary.append(["begin_ns": UInt64(stationaryBegin * 1e9), "end_ns": UInt64(end * 1e9)]) }
            let motionBegin = firstMovement + 0.1, motionEnd = lastMovement - 0.1
            try require((5...10).contains(motionEnd - motionBegin), "Observed owner movement must provide a five-to-ten-second window")
            witnesses.append(["gesture": gesture, "theme": theme, "samples": samples, "observed_distance": movement,
                "waypoints": path.map { [$0.x, $0.y] },
                "poses": poses, "stationary_pose_intervals": stationary,
                "first_movement_ns": UInt64(firstMovement * 1e9), "last_movement_ns": UInt64(lastMovement * 1e9),
                "owners": distances.keys.sorted().map { ["id": $0, "observed_distance": distances[$0]!] as [String: Any] },
                "begin_ns": UInt64(begin * 1e9), "end_ns": UInt64(end * 1e9)])
            if gesture > 0 { windows.append(["begin_ns": UInt64(motionBegin * 1e9), "end_ns": UInt64(motionEnd * 1e9),
                "motion_witness": "witness.json gesture \(gesture), timestamped actual owner poses; first and last movement with 100ms edges"]) }
            try await settle()
            try await capture(window, to: directory.appendingPathComponent("gesture-\(gesture).png"))
            try require(store.state["workspace"]["layout"]["bubbles"].stableKey != baseline, "The drag must author a position before Undo")
            try await store.apply(["type": "invoke", "command": "undo_workspace"])
            try await wait("Bubble drag restored", failure: { store.failure }) { store.state["workspace"]["layout"]["bubbles"].stableKey == baseline }
            try await settle()
            try require(store.state["workspace"]["layout"]["bubbles"].stableKey == baseline, "Undo must restore each gesture exactly")
        }
        for (name, value) in [("windows.json", windows), ("witness.json", witnesses)] {
            try JSONSerialization.data(withJSONObject: value, options: [.prettyPrinted, .sortedKeys]).write(to: directory.appendingPathComponent(name))
        }
        try require(store.failure == nil, store.failure ?? "")
        note("PASS native bubble motion: one prime and three six-second gestures; trace process \(ProcessInfo.processInfo.processIdentifier)")
        try await drain(max(3, (Double(environment["CAPY_TRACE_SECONDS"] ?? "0") ?? 0) - (CACurrentMediaTime() - started) + 3))
    }
    @MainActor static func main() {
        _ = NSApplication.shared; NSApp.setActivationPolicy(.accessory)
        NSEvent.isMouseCoalescingEnabled = false
        Task { @MainActor in
            do { try await run(); exit(0) }
            catch { note("FAIL: \(error.localizedDescription)"); exit(1) }
        }
        NSApp.run()
    }
}
