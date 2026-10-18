import SwiftUI

struct NumberControl: View {
    struct Toolbar: Equatable { var slider: Bool }
    @ObservedObject var store: EditorStore
    let label: String
    let value: Double
    let control: JSON
    var identifier = ""
    var presentedText: String? = nil
    var valueOnly = false
    var inline = false
    var panel = true
    var entryWidth: CGFloat = 48
    var affixes: (prefix: String, suffix: String)? = nil
    var toolbar: Toolbar? = nil
    var registerAdmission: ((String, ((Bool) -> Bool)?) -> Void)? = nil
    var gestureChange: ((String, Double, @escaping @MainActor (String?) -> Void) -> Void)? = nil
    var reset: (() -> Void)? = nil
    let change: (Double, @escaping @MainActor (String?) -> Void) -> Void
    @State private var field = NumericEditState()
    @State private var formatted = JSON()
    @State private var numericLabels = JSON()
    @State private var showsEntry = false
    @State private var horizontalDrag: Bool?
    @State private var dialDrag: Bool?
    @State private var valueScrub: ValueScrub?
    @State private var valueDragCancelled = false
    @FocusState private var scrubFocused: Bool
    @GestureState private var contact = false
    @State private var editing = false
    @State private var inlineMeasure = ""
    @Environment(\.compactPanelControls) private var compact
    @Environment(\.isEnabled) private var enabled
    private var palette: EditorPalette { EditorPalette(source: store.state["palette"]) }
    private var slider: Bool { control["kind"].string == "slider" }
    private var dial: Bool { control["kind"].string == "dial" }
    private var compactPanel: Bool { panel && (slider || dial) && !inline && !valueOnly && toolbar == nil }
    private var spinWidth: CGFloat {
        compact && !slider ? max(24, EditorTextMetrics.width(inlineMeasure, size: store.catalog["text_size_pt"].number * 4 / 3, weight: .regular, monospacedDigits: true)) : entryWidth
    }
    private var key: String { identifier.isEmpty ? label : identifier }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let toolbar {
                HStack(spacing: 4) {
                    if toolbar.slider { sliderTrack.frame(width: 56) }
                    if showsEntry || field.dirty { numericEntry } else { valueButton }
                }
            } else if inline {
                HStack(spacing: 4) {
                    sliderTrack
                    InlineNumberValueLayout(entrySize: showsEntry || field.dirty ? min(10, max(3, formatted["edit"].string.count)) : nil) {
                        Text(inlineMeasure).monospacedDigit().padding(.horizontal, 6).hidden().accessibilityHidden(true)
                        Text("8").monospacedDigit().hidden().accessibilityHidden(true)
                        if showsEntry || field.dirty { numericEntry }
                        else { valueButton }
                    }
                }
            } else if valueOnly {
                InlineNumberValueLayout(entrySize: showsEntry || field.dirty ? min(10, max(3, formatted["edit"].string.count)) : nil) {
                    Text(formatted["text"].string).monospacedDigit().padding(.horizontal, 6).hidden().accessibilityHidden(true)
                    Text("8").monospacedDigit().hidden().accessibilityHidden(true)
                    if showsEntry || field.dirty { numericEntry }
                    else { valueButton }
                }
            }
            else if let affixes {
                HStack(spacing: 0) {
                    if !affixes.prefix.isEmpty { Text(affixes.prefix).foregroundStyle(palette["text"].opacity(0.55)).padding(.leading, 10) }
                    numericEntry
                    if !affixes.suffix.isEmpty { Text(affixes.suffix).foregroundStyle(palette["text"].opacity(0.55)) }
                }.modifier(NumberControlMeasurement(id: key + ":header"))
            }
            else if compactPanel {
                ZStack {
                    Text(label).lineLimit(1).truncationMode(.tail)
                        .frame(maxWidth: .infinity, alignment: .leading).frame(height: 20)
                        .padding(.leading, compact ? 0 : 6).padding(.trailing, 76)
                        .modifier(NumberControlMeasurement(id: key + ":label"))
                        .onTapGesture(count: 2, perform: resetValue)
                        .frame(maxHeight: .infinity, alignment: .top)
                    if dial {
                        angleDial.padding(.trailing, 76)
                            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .trailing)
                    } else {
                        sliderTrack.padding(.leading, 36).padding(.trailing, 76)
                            .frame(maxHeight: .infinity, alignment: .bottom)
                    }
                    Group {
                        if showsEntry || field.dirty { numericEntry }
                        else { valueButton }
                    }.frame(width: showsEntry || field.dirty ? 68 : nil)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
                }.frame(height: 36).modifier(NumberControlMeasurement(id: key + ":header"))
            } else {
                HStack(spacing: compact ? store.panelMetrics.spacing : 6) {
                    Text(label).lineLimit(1).truncationMode(.tail)
                        .frame(maxWidth: .infinity, alignment: .leading).padding(.leading, compact ? 0 : 6)
                        .modifier(NumberControlMeasurement(id: key + ":label"))
                    if !slider {
                        HStack(spacing: 0) {
                            numericEntry
                            stepButton(-1)
                            stepButton(1)
                        }.background(palette["input"], in: SquircleShape.control).fixedSize()
                    } else if showsEntry || field.dirty {
                        numericEntry.fixedSize()
                    } else {
                        valueButton.fixedSize()
                    }
                }.modifier(NumberControlMeasurement(id: key + ":header"))
            }
            if slider && !valueOnly && !inline && !compactPanel && toolbar == nil {
                HStack(spacing: 6) {
                    stepButton(-1)
                    sliderTrack
                    stepButton(1)
                }
            }
            if let error = field.error, !inline && !valueOnly && affixes == nil {
                Text(error).font(.caption).foregroundStyle(.red).padding(.leading, 6)
                    .accessibilityIdentifier("number-error-" + key)
            }
        }
        .modifier(NumberControlMeasurement(id: key + ":root"))
        .onAppear { field.receive(value); format(); measureInlineRange(); registerAdmission?(key, admit) }
        .onChange(of: store.interfaceLanguage) { _, _ in numericLabels = NativeTextContext.numericLabels(label, language: store.interfaceLanguage); format() }
        .onChange(of: label, initial: true) { _, next in numericLabels = NativeTextContext.numericLabels(next, language: store.interfaceLanguage) }
        .onChange(of: value) { _, next in field.receive(next); format(); registerAdmission?(key, admit) }
        .onChange(of: presentedText) { _, _ in format() }
        .onChange(of: control.stableKey) { _, _ in registerAdmission?(key, admit) }
        .onChange(of: contact) { _, active in if !active { cancelDrag(); valueDragCancelled = false } }
        .onChange(of: enabled) { _, active in if !active { cancelDrag() } }
        .onDisappear { registerAdmission?(key, nil); cancelDrag() }
        .onChange(of: editing) { _, focused in
            if focused {
                if !field.dirty { field.text = formatted["edit"].string }
            } else if commit() { showsEntry = false; format() }
        }
    }
    @ViewBuilder private var valueButton: some View {
        if compactPanel {
            valueReadout.focusable().focusEffectDisabled().focused($scrubFocused)
                .highPriorityGesture(valueDrag)
                .onKeyPress(.escape) {
                    guard valueScrub != nil else { return .ignored }
                    cancelDrag(); return .handled
                }
        } else { valueReadout }
    }
    private var valueReadout: some View {
        Button { showsEntry = true } label: {
            Text(formatted[valueScrub == nil ? "text" : "scrub_text"].string).monospacedDigit().lineLimit(1)
                .frame(maxWidth: inline || valueOnly || toolbar != nil ? .infinity : nil, alignment: valueOnly ? .center : .trailing)
                .padding(.horizontal, 6).frame(height: 24)
        }.buttonStyle(EditorControlButtonStyle()).opacity(enabled ? 1 : 0.36)
            // An asynchronous rejection can arrive after text entry closes.
            // Keep compact errors visible without expanding the layer header.
            .overlay {
                if (inline || valueOnly) && field.error != nil {
                    SquircleShape.control.stroke(.red, lineWidth: 1).allowsHitTesting(false)
                }
            }
            .modifier(NumberControlMeasurement(id: key + ":value"))
            .accessibilityLabel(label).accessibilityValue(formatted["text"].string)
            .accessibilityHint(field.error ?? "").help(field.error ?? label)
            .accessibilityIdentifier("number-value-" + key)
    }
    private var sliderTrack: some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Capsule().fill(palette["input"])
                Rectangle().fill(palette["panel"])
                    .overlay(palette["text"].opacity(0.5))
                    .frame(width: max(0, geometry.size.width * formatted["fill"].number))
            }.frame(height: 4).clipShape(Capsule()).frame(maxHeight: .infinity)
                .contentShape(Rectangle())
                // Preserve vertical scrolling through long tool-settings lists.
                .simultaneousGesture(DragGesture(minimumDistance: 6).updating($contact) { _, active, _ in active = true }.onChanged { event in
                    guard enabled else { cancelDrag(); return }
                    if horizontalDrag == nil {
                        horizontalDrag = abs(event.translation.width) > abs(event.translation.height)
                        if horizontalDrag == true { gestureChange?("down", field.value) { field.error = $0 } }
                    }
                    if horizontalDrag == true {
                        position(event.location.x / max(1, geometry.size.width), phase: "move")
                    }
                }.onEnded { event in
                    guard enabled else { cancelDrag(); return }
                    if horizontalDrag == true { position(event.location.x / max(1, geometry.size.width), phase: "up") }
                    horizontalDrag = nil
                }.exclusively(before: SpatialTapGesture().onEnded { tap in position(tap.location.x / max(1, geometry.size.width)) }))
        }.frame(height: compactPanel ? 16 : 24).modifier(NumberControlMeasurement(id: key + ":track"))
            .accessibilityElement().accessibilityLabel(label)
            .accessibilityValue(formatted["text"].string)
            .accessibilityAdjustableAction { direction in
                switch direction {
                case .increment: step(1)
                case .decrement: step(-1)
                @unknown default: break
                }
            }.accessibilityIdentifier("number-track-" + key)
    }
    private var angleDial: some View {
        let diameter = store.panelMetrics.sliderHeight - 2 * store.panelMetrics.spacing
        let angle = CGFloat(formatted["value"].number * control["scale"].number) * .pi / 180
        let face = palette["input"], panelColor = palette["panel"], ink: Color = palette["text"].opacity(0.5)
        return Canvas { context, size in
            let radius = min(size.width, size.height) / 2
            let dot = CGPoint(x: size.width / 2 + cos(angle) * radius * 0.55, y: size.height / 2 + sin(angle) * radius * 0.55)
            let mark = Path(ellipseIn: CGRect(x: dot.x - radius * 0.22, y: dot.y - radius * 0.22, width: radius * 0.44, height: radius * 0.44))
            context.fill(Path(ellipseIn: CGRect(x: size.width / 2 - radius, y: size.height / 2 - radius, width: radius * 2, height: radius * 2)), with: .color(face))
            context.fill(mark, with: .color(panelColor))
            context.fill(mark, with: .color(ink))
        }.frame(width: diameter, height: diameter).contentShape(Rectangle())
            .gesture(DragGesture(minimumDistance: 0).updating($contact) { _, active, _ in active = true }.onChanged { event in
                guard enabled else { cancelDrag(); return }
                if dialDrag == nil {
                    dialDrag = commit()
                    if dialDrag == true { gestureChange?("down", field.value) { field.error = $0 } }
                }
                if dialDrag == true { pointDial(event.location, diameter: diameter, phase: "move") }
            }.onEnded { event in
                guard enabled else { cancelDrag(); return }
                if dialDrag == true { pointDial(event.location, diameter: diameter, phase: "up") }
                dialDrag = nil
            })
            .simultaneousGesture(TapGesture(count: 2).onEnded { resetValue() })
            .modifier(NumberControlMeasurement(id: key + ":dial"))
            .accessibilityElement().accessibilityLabel(label)
            .accessibilityValue(formatted["text"].string)
            .accessibilityAdjustableAction { direction in
                switch direction {
                case .increment: step(1)
                case .decrement: step(-1)
                @unknown default: break
                }
            }.accessibilityIdentifier("number-dial-" + key)
    }
    private var numericEntry: some View {
        NumericTextField(label: numericLabels["edit"].string,
            text: Binding(get: { field.text }, set: { field.text = $0; field.dirty = true }),
            focused: $editing, fontSize: max(1, store.catalog["text_size_pt"].number * 4 / 3),
            color: palette["text"], identifier: "number-entry-" + key,
            submit: finish, cancel: cancel, step: step)
            .focusedValue(\.editorTextCommit, { _ = commit() })
            .frame(width: valueOnly || inline || toolbar != nil || affixes != nil ? nil : compactPanel ? 56 : slider ? 80 : spinWidth)
            .padding(.horizontal, 6).frame(height: compact ? store.panelMetrics.height : valueOnly || slider || compactPanel ? 24 : 32)
            .background(palette["input"], in: SquircleShape.control)
            .modifier(NumberControlMeasurement(id: key + ":entry"))
            .accessibilityHint(field.error ?? "")
            .help(field.error ?? label)
            .overlay(SquircleShape.control.stroke(field.error == nil ? Color.clear : Color.red, lineWidth: 1)
                .allowsHitTesting(false))
            .onAppear { if showsEntry { editing = true } }
    }
    private func finish(returnToCanvas: Bool) -> Bool {
        guard commit() else { return false }
        endEditing(returnToCanvas: returnToCanvas)
        return true
    }
    private func cancel() {
        field.dirty = false; field.error = nil; endEditing(returnToCanvas: true); format()
    }
    private func endEditing(returnToCanvas: Bool) {
        showsEntry = false; editing = false
        // Resume canvas keys after SwiftUI removes or resigns the text field.
        // Tab keeps the native focus traversal instead.
        if returnToCanvas { DispatchQueue.main.async { store.focusCanvas?() } }
    }
    private func stepButton(_ direction: Int) -> some View {
        let active = enabled && (direction < 0 ? field.value > control["min"].number : field.value < control["max"].number)
        return Button { step(direction) } label: {
            SharedIcon(name: direction < 0 ? "minus" : "plus").frame(width: compact ? store.panelMetrics.height : slider ? 24 : 32, height: compact ? store.panelMetrics.height : slider ? 24 : 32)
                .contentShape(Rectangle())
        }.buttonStyle(EditorControlButtonStyle()).opacity(active ? 1 : 0.36)
            .modifier(NumberControlMeasurement(id: key + (direction < 0 ? ":minus" : ":plus")))
            .disabled(!active)
            .accessibilityLabel(numericLabels[direction < 0 ? "decrease" : "increase"].string)
            .accessibilityIdentifier("number-\(direction < 0 ? "decrease" : "increase")-" + key)
    }
    private func format() {
        do {
            formatted = try resolveNumber(value: field.value, operation: ["type": "format"])
            if let presentedText { formatted = formatted.replacing("text", with: JSON(presentedText)).replacing("edit", with: JSON(presentedText)) }
            if !field.dirty && !editing { field.text = formatted[slider || compactPanel ? "edit" : "text"].string }
        } catch { field.error = error.localizedDescription }
    }
    private func measureInlineRange() {
        guard inline || (compact && !slider) else { return }
        do {
            let minimum = try resolveNumber(value: control["min"].number, operation: ["type": "format"])["text"].string
            let maximum = try resolveNumber(value: control["max"].number, operation: ["type": "format"])["text"].string
            inlineMeasure = (minimum.count >= maximum.count ? minimum : maximum).map { $0.isNumber ? "8" : String($0) }.joined()
        } catch { field.error = error.localizedDescription }
    }
    private func resolveNumber(value: Double, operation: [String: Any]) throws -> JSON {
        guard toolbar != nil else { return try store.resolveNumber(control, value: value, operation: operation) }
        let result = ToolbarUI.resolve(["type": "number", "request": ["control": control.raw, "value": value, "operation": operation],
            "compact": true, "units": true], language: store.interfaceLanguage)
        if !result["error"].isNull { throw HostFailure(message: result["error"].string) }
        return result
    }
    private func admit(_ discard: Bool) -> Bool {
        if discard { cancel(); return true }
        return commit()
    }
    @discardableResult private func commit() -> Bool {
        guard !NativeTextContext.composing else { return false }
        guard field.dirty else { return true }
        return resolve(["type": "expression", "text": field.text]) != nil
    }
    private func step(_ direction: Int) {
        guard commit() else { return }
        _ = resolve(["type": "step", "steps": direction])
    }
    private func cancelDrag() {
        if valueScrub != nil { valueDragCancelled = true; settleValueDrag(cancel: true) }
        if horizontalDrag == true || dialDrag == true { gestureChange?("cancel", field.value) { field.error = $0 } }
        horizontalDrag = nil; dialDrag = nil
    }
    private var valueDrag: some Gesture {
        DragGesture(minimumDistance: 6, coordinateSpace: .global)
            .updating($contact) { _, active, _ in active = true }
            .onChanged { event in
                guard enabled, !valueDragCancelled else { return }
                if valueScrub == nil {
                    guard commit() else { valueDragCancelled = true; return }
                    valueScrub = ValueScrub(start: field.value, fine: ThumbnailSelectionLoad.modifiers().shift); scrubFocused = true
                    gestureChange?("down", field.value) { field.error = $0 }
                }
                scrub(-Double(event.translation.height), phase: "move")
            }.onEnded { event in
                if valueScrub != nil, !valueDragCancelled {
                    scrub(-Double(event.translation.height), phase: "up")
                    valueScrub = nil; format()
                }
                valueDragCancelled = false
            }
    }
    private func scrub(_ pixels: Double, phase: String) {
        guard var current = valueScrub else { return }
        if let next = resolve(current.operation(pixels: pixels, fine: ThumbnailSelectionLoad.modifiers().shift), phase: phase) { current.value = next }
        valueScrub = current
    }
    private func settleValueDrag(cancel: Bool) {
        guard let start = valueScrub?.start else { return }
        valueScrub = nil
        if cancel {
            if let gestureChange { gestureChange("cancel", start) { field.error = $0 } }
            else if Float(start) != Float(field.value) {
                let token = field.submit(start)
                change(start) { error in field.complete(token, error: error); format() }
            }
        }
        format()
    }
    private func resetValue() {
        guard enabled else { return }
        cancelDrag(); cancel()
        if let reset { reset() }
        else if !control["default_value"].isNull { _ = resolve(["type": "expression", "text": ""]) }
    }
    private func position(_ position: Double, phase: String? = nil) {
        guard enabled else { return }
        field.dirty = false; showsEntry = false; editing = false
        _ = resolve(["type": "position", "position": min(1, max(0, position))], phase: phase)
    }
    private func pointDial(_ point: CGPoint, diameter: CGFloat, phase: String) {
        guard enabled else { return }
        field.dirty = false; showsEntry = false; editing = false
        let radius = diameter / 2
        _ = resolve(["type": "dial", "x": Double(point.x - radius), "y": Double(point.y - radius), "radius": Double(radius)], phase: phase)
    }
    @discardableResult private func resolve(_ operation: [String: Any], phase: String? = nil) -> Double? {
        do {
            let result = try resolveNumber(value: field.value, operation: operation)
            field.dirty = false; field.error = nil
            field.text = result["edit"].string
            let next = result["value"].number
            if let phase, let gestureChange {
                // Gesture previews follow the published shared value. Only
                // discrete edits need optimistic receipt/acknowledgment state.
                gestureChange(phase, next) { error in field.error = error }
            } else if Float(next) != Float(field.value) {
                let token = field.submit(next)
                change(next) { error in field.complete(token, error: error); format() }
            }
            format()
            return next
        } catch {
            field.error = error.localizedDescription
            return nil
        }
    }
    private struct ValueScrub {
        let start: Double
        var value: Double
        private var origin: Double
        private var fine: Bool
        private var base = 0.0
        private var last = 0.0
        init(start: Double, fine: Bool) { self.start = start; value = start; origin = start; self.fine = fine }
        mutating func operation(pixels: Double, fine: Bool) -> [String: Any] {
            if fine != self.fine { origin = value; base = last; self.fine = fine }
            last = pixels
            return ["type": "scrub", "origin": origin, "pixels": pixels - base, "fine": fine]
        }
    }
}

/// Reserve the formatted range's widest digit string in readout mode. Text
/// entry additionally reserves its initial character count, like input.size.
private struct InlineNumberValueLayout: Layout {
    let entrySize: Int?
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        guard subviews.count == 3 else { return .zero }
        let measure = subviews[0].sizeThatFits(.unspecified).width
        let entry = entrySize.map { CGFloat($0) * ceil(subviews[1].sizeThatFits(.unspecified).width) + 12 } ?? 0
        return CGSize(width: max(measure, entry), height: 24)
    }
    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        for view in subviews {
            view.place(at: bounds.origin, anchor: .topLeading,
                proposal: ProposedViewSize(width: bounds.width, height: bounds.height))
        }
    }
}

// Disabled in ordinary editors; direct component captures opt in without
// opening a window or relying on platform accessibility traversal.
private struct MeasureNumberControls: EnvironmentKey { static let defaultValue = false }
extension EnvironmentValues {
    var measureNumberControls: Bool {
        get { self[MeasureNumberControls.self] }
        set { self[MeasureNumberControls.self] = newValue }
    }
}
struct NumberControlFrames: PreferenceKey {
    static let defaultValue: [String: CGRect] = [:]
    static func reduce(value: inout [String: CGRect], nextValue: () -> [String: CGRect]) {
        value.merge(nextValue()) { _, new in new }
    }
}
struct NumberControlMeasurement: ViewModifier {
    let id: String
    @Environment(\.measureNumberControls) private var enabled
    func body(content: Content) -> some View {
        if enabled {
            content.background(GeometryReader { proxy in
                Color.clear.preference(key: NumberControlFrames.self,
                    value: [id: proxy.frame(in: .named("number-capture"))])
            })
        } else { content }
    }
}
