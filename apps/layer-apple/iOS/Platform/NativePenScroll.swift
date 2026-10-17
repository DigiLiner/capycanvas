import SwiftUI
import UIKit
import ObjectiveC

struct NativePenScroll: View {
    var showsIndicators = true
    var axes: Axis.Set = .vertical
    var body: some View { PenScrollHost(showsIndicators: showsIndicators, axes: axes).allowsHitTesting(false) }
}

private struct PenScrollHost: UIViewRepresentable {
    let showsIndicators: Bool
    let axes: Axis.Set
    func makeUIView(context: Context) -> PenScrollView { let view = PenScrollView(); view.showsIndicators = showsIndicators; view.axes = axes; return view }
    func updateUIView(_ view: PenScrollView, context: Context) { view.showsIndicators = showsIndicators; view.axes = axes; view.attach() }
}

final class PenScrollView: UIView {
    private static var controllerKey: UInt8 = 0
    var showsIndicators = true
    var axes: Axis.Set = .vertical
    override init(frame: CGRect) { super.init(frame: frame); isUserInteractionEnabled = false }
    required init?(coder: NSCoder) { fatalError("Use init(frame:)") }
    override func didMoveToWindow() { super.didMoveToWindow(); attach() }
    override func layoutSubviews() { super.layoutSubviews(); attach() }
    func attach() {
        var node = superview
        while let view = node {
            if let scroll = view as? UIScrollView {
                let controller = objc_getAssociatedObject(scroll, &Self.controllerKey) as? PenScrollController ?? PenScrollController(scroll: scroll)
                objc_setAssociatedObject(scroll, &Self.controllerKey, controller, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
                controller.showsIndicators = showsIndicators; controller.axes = axes; controller.indicators()
                if let window { ScrollPointerInput.attach(to: window) }
                return
            }
            node = view.superview
        }
    }
}

private final class PenScrollController: NSObject {
    private weak var scroll: UIScrollView?
    var showsIndicators = true
    var axes: Axis.Set = .vertical
    init(scroll: UIScrollView) {
        self.scroll = scroll
        super.init()
        let pencil = NSNumber(value: UITouch.TouchType.pencil.rawValue)
        if !scroll.panGestureRecognizer.allowedTouchTypes.contains(pencil) { scroll.panGestureRecognizer.allowedTouchTypes.append(pencil) }
        ScrollPointerInput.controllers.add(self)
    }
    func indicators() {
        guard let scroll else { return }
        let horizontal = axes.contains(.horizontal) && showsIndicators && ScrollPointerInput.mouse
        let vertical = axes.contains(.vertical) && showsIndicators && ScrollPointerInput.mouse
        if scroll.showsHorizontalScrollIndicator != horizontal { scroll.showsHorizontalScrollIndicator = horizontal }
        if scroll.showsVerticalScrollIndicator != vertical { scroll.showsVerticalScrollIndicator = vertical }
    }
}

private final class ScrollPointerInput: UIGestureRecognizer, UIGestureRecognizerDelegate {
    private static var observerKey: UInt8 = 0
    static let controllers = NSHashTable<PenScrollController>.weakObjects()
    private(set) static var mouse = false
    static func input(mouse: Bool) {
        if self.mouse == mouse { return }
        self.mouse = mouse
        for controller in controllers.allObjects { controller.indicators() }
    }
    static func attach(to window: UIWindow) {
        if objc_getAssociatedObject(window, &observerKey) != nil { return }
        let observer = ScrollPointerInput(target: nil, action: nil)
        observer.allowedTouchTypes = [UITouch.TouchType.direct, .pencil, .indirectPointer].map { NSNumber(value: $0.rawValue) }
        observer.delegate = observer; observer.cancelsTouchesInView = false
        window.addGestureRecognizer(observer)
        for type in [UITouch.TouchType.indirectPointer, .pencil] {
            let hover = UIHoverGestureRecognizer(target: observer, action: #selector(observer.hovered))
            hover.allowedTouchTypes = [NSNumber(value: type.rawValue)]
            hover.cancelsTouchesInView = false; hover.delegate = observer
            window.addGestureRecognizer(hover)
        }
        objc_setAssociatedObject(window, &observerKey, observer, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
    }
    func gestureRecognizer(_ recognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        Self.input(mouse: touch.type == .indirectPointer)
        return recognizer !== self
    }
    func gestureRecognizer(_ recognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool { true }
    @objc private func hovered(_ recognizer: UIHoverGestureRecognizer) {
        if recognizer.state == .began || recognizer.state == .changed {
            Self.input(mouse: recognizer.allowedTouchTypes.contains(NSNumber(value: UITouch.TouchType.indirectPointer.rawValue)))
        }
    }
}
