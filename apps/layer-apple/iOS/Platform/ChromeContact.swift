import SwiftUI
import UIKit

extension View {
    func editorChromeContact(_ action: @escaping (CGPoint?, Bool) -> Bool) -> some View {
        background(EditorContact(action: action))
    }
}

private struct EditorContact: UIViewRepresentable {
    let action: (CGPoint?, Bool) -> Bool
    func makeUIView(context: Context) -> EditorContactView { EditorContactView() }
    func updateUIView(_ view: EditorContactView, context: Context) { view.observer.action = action }
    static func dismantleUIView(_ view: EditorContactView, coordinator: ()) { view.detach() }
}

private final class EditorContactView: UIView {
    let observer = ChromeContacts(target: nil, action: nil)
    private weak var attached: UIView?
    private var focus: NSObjectProtocol?
    override init(frame: CGRect) {
        super.init(frame: frame)
        isUserInteractionEnabled = false
        observer.cancelsTouchesInView = false; observer.delaysTouchesBegan = false; observer.delaysTouchesEnded = false
        observer.requiresExclusiveTouchType = false
        observer.allowedTouchTypes = [UITouch.TouchType.direct, .pencil, .indirectPointer].map { NSNumber(value: $0.rawValue) }
    }
    required init?(coder: NSCoder) { fatalError("Use init(frame:)") }
    override func didMoveToSuperview() { super.didMoveToSuperview(); attach() }
    override func didMoveToWindow() { super.didMoveToWindow(); detach(); attach() }
    private func attach() {
        let next = window == nil ? nil : superview
        guard next !== attached else { return }
        detach(); attached = next; next?.addGestureRecognizer(observer)
        if let scene = window?.windowScene { focus = NotificationCenter.default.addObserver(forName: UIScene.willDeactivateNotification, object: scene, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.observer.cancel() }
        } }
    }
    func detach() {
        if let focus { NotificationCenter.default.removeObserver(focus); self.focus = nil }
        observer.cancel(); attached?.removeGestureRecognizer(observer); attached = nil
    }
}

private final class ChromeContacts: UIGestureRecognizer {
    var action: (CGPoint?, Bool) -> Bool = { _, _ in false }
    private var held = Set<ObjectIdentifier>()
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        for touch in touches where !(touch.view is CanvasView) {
            if action(touch.location(in: view), true) { held.insert(ObjectIdentifier(touch)) }
        }
    }
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        held.subtract(touches.map(ObjectIdentifier.init)); _ = action(nil, !held.isEmpty)
        if event.allTouches?.allSatisfy({ $0.phase == .ended || $0.phase == .cancelled }) == true { state = .failed }
    }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) { touchesEnded(touches, with: event) }
    override func canPrevent(_ preventedGestureRecognizer: UIGestureRecognizer) -> Bool { false }
    override func canBePrevented(by preventingGestureRecognizer: UIGestureRecognizer) -> Bool { false }
    func cancel() { held.removeAll(); _ = action(nil, false) }
    override func reset() { super.reset(); cancel() }
}
