import SwiftUI
import AppKit

extension View {
    func editorChromeContact(_ action: @escaping (CGPoint?, Bool) -> Bool) -> some View {
        background(EditorContact(action: action))
    }
}

private struct EditorContact: NSViewRepresentable {
    let action: (CGPoint?, Bool) -> Bool
    func makeNSView(context: Context) -> EditorContactView { EditorContactView() }
    func updateNSView(_ view: EditorContactView, context: Context) { view.action = action }
    static func dismantleNSView(_ view: EditorContactView, coordinator: ()) { view.detach() }
}

private final class EditorContactView: NSView {
    var action: (CGPoint?, Bool) -> Bool = { _, _ in false }
    private var monitor: Any?
    private var focus: NSObjectProtocol?
    private var held = Set<Int>()
    override var isFlipped: Bool { true }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow(); detach()
        guard let window else { return }
        focus = NotificationCenter.default.addObserver(forName: NSWindow.didResignKeyNotification, object: window, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.held.removeAll(); _ = self?.action(nil, false) }
        }
        monitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .leftMouseUp, .rightMouseDown, .rightMouseUp, .otherMouseDown, .otherMouseUp, .mouseMoved, .leftMouseDragged, .rightMouseDragged, .otherMouseDragged]) { [weak self, weak window] event in
            guard let self, let window else { return event }
            let previous = self.held
            self.held = self.held.filter { NSEvent.pressedMouseButtons & (1 << $0) != 0 }
            if previous != self.held { _ = self.action(nil, !self.held.isEmpty) }
            if [.leftMouseUp, .rightMouseUp, .otherMouseUp].contains(event.type) {
                self.held.remove(event.buttonNumber); _ = self.action(nil, !self.held.isEmpty)
            } else if [.leftMouseDown, .rightMouseDown, .otherMouseDown].contains(event.type), event.window === window {
                let point = self.convert(event.locationInWindow, from: nil)
                let target = window.contentView.flatMap { $0.hitTest($0.convert(event.locationInWindow, from: nil)) }
                if self.bounds.contains(point), !(target is MacCanvasView),
                   self.action(point, true) { self.held.insert(event.buttonNumber) }
            }
            return event
        }
    }
    func detach() {
        if let monitor { NSEvent.removeMonitor(monitor); self.monitor = nil }
        if let focus { NotificationCenter.default.removeObserver(focus); self.focus = nil }
        held.removeAll(); _ = action(nil, false)
    }
}
