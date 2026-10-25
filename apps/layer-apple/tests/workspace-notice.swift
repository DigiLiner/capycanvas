import AppKit
import SwiftUI

@main struct WorkspaceNoticeChecks {
    @MainActor static func main() async throws {
        _ = NSApplication.shared
        let presence = CanvasNoticePresence()
        var replies: [(UInt64, Bool, String?)] = []
        presence.answer = { replies.append(($0, $1, $2)) }
        func notice(_ text: String, enabled: Bool) -> JSON {
            JSON(["id": 7, "text": text, "name": text,
                "actions": [["id": "undo", "label": text, "enabled": enabled]]])
        }
        presence.publish(notice("Ready", enabled: true))
        try await Task.sleep(for: .milliseconds(2300))
        presence.publish(notice("Unavailable", enabled: false))
        precondition(presence.notice["text"].string == "Unavailable")
        precondition(!presence.notice["actions"][0]["enabled"].bool)
        try await Task.sleep(for: .milliseconds(2100))
        precondition(presence.notice.isNull)
        precondition(replies.count == 1 && replies[0].0 == 7 && !replies[0].1)
        presence.publish(notice("Retired", enabled: true))
        precondition(presence.notice.isNull && replies.count == 1)
        presence.publish(JSON(["id": 8, "text": "Next", "actions": []]))
        precondition(presence.notice["text"].string == "Next")
        presence.hide()
        try await scrollRestoration()
    }
    @MainActor private static func scrollRestoration() async throws {
        NSApp.setActivationPolicy(.prohibited)
        let window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 80, height: 160),
            styleMask: [.borderless], backing: .buffered, defer: true)
        window.isReleasedWhenClosed = false
        defer { window.contentView = nil; window.close() }
        let position = EditorScrollPosition()
        position.offset = 240
        var observed: CGFloat?
        func mount() {
            window.contentView = NSHostingView(rootView:
                EditorPositionedScrollView(position: position, offsetChanged: { observed = $0 }) {
                    Rectangle().frame(width: 60, height: 1000)
                }.frame(width: 80, height: 160))
        }
        func waitForOffset(_ target: CGFloat) async throws {
            let deadline = Date().addingTimeInterval(5)
            while observed.map({ abs($0 - target) < 1 }) != true {
                window.contentView?.layoutSubtreeIfNeeded()
                guard Date() < deadline else { throw NSError(domain: "Native scroll offset restoration", code: 1) }
                try await Task.sleep(for: .milliseconds(10))
            }
        }
        mount()
        try await waitForOffset(240)
        position.scrollTo(y: 400)
        try await waitForOffset(400)
        window.contentView = nil
        observed = nil
        mount()
        try await waitForOffset(400)
    }
}
