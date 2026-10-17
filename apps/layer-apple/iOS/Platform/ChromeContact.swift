import SwiftUI
import UIKit

extension View {
    func editorChromeContact(_ action: @escaping (CGPoint?, Bool) -> Bool) -> some View {
        background(EditorContact(action: action).allowsHitTesting(false))
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
    private weak var attached: UIWindow?
    private var focus: NSObjectProtocol?
    override init(frame: CGRect) {
        super.init(frame: frame)
        isUserInteractionEnabled = false; observer.coordinates = self
        observer.cancelsTouchesInView = false; observer.delaysTouchesBegan = false; observer.delaysTouchesEnded = false
        observer.requiresExclusiveTouchType = false
        observer.allowedTouchTypes = [UITouch.TouchType.direct, .pencil, .indirectPointer].map { NSNumber(value: $0.rawValue) }
    }
    required init?(coder: NSCoder) { fatalError("Use init(frame:)") }
    override func didMoveToSuperview() { super.didMoveToSuperview(); attach() }
    override func didMoveToWindow() { super.didMoveToWindow(); detach(); attach() }
    private func attach() {
        let next = window
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
    weak var coordinates: UIView?
    private var held = [ObjectIdentifier:UUID]()
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        guard let coordinates else { return }
        for touch in touches where !(touch.view is CanvasView) {
            let point = touch.location(in: coordinates)
            guard touch.window === coordinates.window, coordinates.bounds.contains(point) else { continue }
            if action(point, true) { held[ObjectIdentifier(touch)] = UUID() }
        }
    }
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        let retired = held.filter { key, _ in touches.contains { ObjectIdentifier($0) == key } }
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            for (key, owner) in retired where self.held[key] == owner { self.held.removeValue(forKey: key) }
            _ = self.action(nil, !self.held.isEmpty)
            if self.held.isEmpty && self.numberOfTouches == 0 { self.state = .failed }
        }
    }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
        for touch in touches { held.removeValue(forKey: ObjectIdentifier(touch)) }; _ = action(nil, !held.isEmpty)
        if event.allTouches?.allSatisfy({ $0.phase == .ended || $0.phase == .cancelled }) == true { state = .failed }
    }
    override func canPrevent(_ preventedGestureRecognizer: UIGestureRecognizer) -> Bool { false }
    override func canBePrevented(by preventingGestureRecognizer: UIGestureRecognizer) -> Bool { false }
    func cancel() { held.removeAll(); _ = action(nil, false) }
    override func reset() { super.reset(); cancel() }
}
