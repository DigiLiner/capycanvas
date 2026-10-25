import SwiftUI

@MainActor final class GlassRegistry {
    private weak var store: EditorStore?
    private var boxes: [UUID: [CGFloat]] = [:]
    private var connections: [String: JSON] = [:]
    private var arcs: [UUID: [CGFloat]] = [:]
    private var colorBubbles: [UUID: [CGFloat]] = [:]
    private var scheduled = false
    private var sent = ""
    init(store: EditorStore) { self.store = store }
    func box(_ id: UUID, _ value: [CGFloat]?) {
        guard boxes[id] != value else { return }
        boxes[id] = value; schedule()
    }
    func connection(_ key: String, _ value: JSON?) {
        guard connections[key]?.stableKey != value?.stableKey else { return }
        connections[key] = value; schedule()
    }
    func arc(_ id: UUID, _ value: [CGFloat]?) {
        guard arcs[id] != value else { return }; arcs[id] = value; schedule()
    }
    func colorBubble(_ id: UUID, _ value: [CGFloat]?) {
        guard colorBubbles[id] != value else { return }; colorBubbles[id] = value; schedule()
    }
    private func schedule() {
        guard !scheduled else { return }
        scheduled = true
        DispatchQueue.main.async { [weak self] in self?.flush() }
    }
    private func flush() {
        scheduled = false
        let payload = JSON(["regions": boxes.sorted { $0.key.uuidString < $1.key.uuidString }.map { $0.value.map(Double.init) },
            "connections": connections.keys.sorted().compactMap { connections[$0]?.raw },
            "arcs": arcs.sorted { $0.key.uuidString < $1.key.uuidString }.map { $0.value.map(Double.init) },
            "color_bubbles": colorBubbles.sorted { $0.key.uuidString < $1.key.uuidString }.map { $0.value.map(Double.init) }])
        let key = payload.stableKey
        guard key != sent else { return }
        sent = key
        store?.native?.glassRegions(payload)
    }
}

private struct GlassRegistryKey: EnvironmentKey { static let defaultValue: GlassRegistry? = nil }
extension EnvironmentValues {
    var glassRegistry: GlassRegistry? {
        get { self[GlassRegistryKey.self] }
        set { self[GlassRegistryKey.self] = newValue }
    }
}

private struct GlassGeometry: Equatable {
    var bounds = CGRect.zero
    var size = CGSize.zero
    init() {}
    init(_ proxy: GeometryProxy) { bounds = proxy.frame(in: .named("editor-workspace")); size = proxy.size }
    var scale: CGFloat { bounds.width / max(1, size.width) }
}

struct GlassRegistration: ViewModifier {
    let shape: SquircleShape
    var active = true
    var circular = false
    var rounded = false
    var strength: Double = 1
    @Environment(\.glassRegistry) private var registry
    @Environment(\.workspaceChromeOpacity) private var opacity
    @State private var id = UUID()
    @State private var geometry = GlassGeometry()
    private var region: [CGFloat] {
        let bounds = geometry.bounds
        guard active && opacity > 0 && strength > 0 && !bounds.isEmpty else { return [] }
        let radii = circular ? Array(repeating: min(bounds.width, bounds.height) / 2, count: 4)
            : shape.radii(in: CGRect(origin: .zero, size: geometry.size)).map { $0 * geometry.scale }
        return [bounds.minX, bounds.minY, bounds.width, bounds.height] + radii + [circular || rounded ? 0 : 1, strength * opacity]
    }
    func body(content: Content) -> some View {
        content.onGeometryChange(for: GlassGeometry.self) { GlassGeometry($0) } action: { geometry = $0 }
            .onChange(of: region, initial: true) { _, box in registry?.box(id, box.isEmpty ? nil : box) }
            .onDisappear { registry?.box(id, nil) }
    }
}

struct GlassArcRegistration: ViewModifier {
    let arc: JSON
    let strength: Double
    @Environment(\.glassRegistry) private var registry
    @Environment(\.workspaceChromeOpacity) private var opacity
    @State private var id = UUID()
    @State private var geometry = GlassGeometry()
    private var region: [CGFloat] {
        let bounds = geometry.bounds
        guard opacity > 0, strength > 0, !bounds.isEmpty, arc["radius"].number > 0 else { return [] }
        let radius = arc["radius"].number, scale = geometry.scale
        let extra = 21 / radius * 180 / .pi, direction = arc["sweep"].number < 0 ? -1.0 : 1.0
        return [bounds.midX, bounds.midY, radius * scale, arc["band"].number * scale,
            arc["start"].number - direction * extra, arc["sweep"].number + direction * extra, strength * opacity]
    }
    func body(content: Content) -> some View {
        content.onGeometryChange(for: GlassGeometry.self) { GlassGeometry($0) } action: { geometry = $0 }
            .onChange(of: region, initial: true) { _, value in registry?.arc(id, value.isEmpty ? nil : value) }
            .onDisappear { registry?.arc(id, nil) }
    }
}

struct GlassColorBubbleRegistration: ViewModifier {
    let color: JSON
    let strength: Double
    @Environment(\.glassRegistry) private var registry
    @Environment(\.workspaceChromeOpacity) private var opacity
    @State private var id = UUID()
    @State private var geometry = GlassGeometry()
    private var region: [CGFloat] {
        let bounds = geometry.bounds, source = color["bounds"], shape = color["glass"], scale = geometry.scale
        guard opacity > 0, strength > 0, !bounds.isEmpty, !shape.isNull else { return [] }
        return [bounds.minX - source["x"].number * scale, bounds.minY - source["y"].number * scale,
            shape[0].number * scale, shape[1].number * scale, shape[2].number * scale, shape[3].number, strength * opacity]
    }
    func body(content: Content) -> some View {
        content.onGeometryChange(for: GlassGeometry.self) { GlassGeometry($0) } action: { geometry = $0 }
            .onChange(of: region, initial: true) { _, value in registry?.colorBubble(id, value.isEmpty ? nil : value) }
            .onDisappear { registry?.colorBubble(id, nil) }
    }
}

struct GlassConnection: ViewModifier {
    let key: String
    let connection: JSON
    @Environment(\.glassRegistry) private var registry
    @Environment(\.workspaceChromeOpacity) private var opacity
    private var projected: JSON { connection.replacing("strength", with: JSON(opacity)) }
    func body(content: Content) -> some View {
        content.onChange(of: projected.stableKey, initial: true) { _, _ in registry?.connection(key, connection.isNull || opacity <= 0 ? nil : projected) }
            .onDisappear { registry?.connection(key, nil) }
    }
}

extension View {
    func glassSurface(_ shape: SquircleShape, fill: Color, register: Bool = true) -> some View {
        background { shape.fill(fill) }.modifier(GlassRegistration(shape: shape, active: register))
    }
}

struct OutsideShadow<S: Shape>: View {
    let shape: S
    let opacity: Double
    let radius: CGFloat
    let y: CGFloat
    var cuts: [CGRect] = []
    var body: some View {
        GeometryReader { geometry in
            let margin = ceil(radius * 3 + abs(y))
            Canvas { context, size in
                let rect = CGRect(x: margin, y: margin, width: size.width - margin * 2, height: size.height - margin * 2)
                let path = shape.path(in: rect)
                context.drawLayer { layer in
                    layer.addFilter(.shadow(color: .black.opacity(opacity), radius: radius, x: 0, y: y, options: .shadowOnly))
                    layer.fill(path, with: .color(.black))
                }
                context.blendMode = .clear
                context.fill(path, with: .color(.black))
                for cut in cuts { context.fill(Path(cut.offsetBy(dx: margin, dy: margin)), with: .color(.black)) }
            }.frame(width: geometry.size.width + margin * 2, height: geometry.size.height + margin * 2)
                .offset(x: -margin, y: -margin)
        }.allowsHitTesting(false).accessibilityHidden(true)
    }
}
