import Foundation

/// Fixed, versioned synthetic input. Delivery is driven by wall time, independently
/// of frame admission, so slow rendering cannot silently reduce the input load.
struct DrawingWorkloadPlan {
    static let samplesPerSecond = 240
    static let warmupSeconds = 10.0
    static let strokeSamples = 360
    static let cycleSamples = 384 // 1.5 s drawing, then 0.1 s with the pen lifted.
    let name: String
    let identifier: UInt64
    let extent: UInt32
    let paintLayers: Int
    let brush: UInt32
    let diameter: Double
    let predicts: Bool
    let seconds: Double
    let color: [String: String]
    let document: String?
    let theme: String?
    var documentExtent: [UInt32] { name == "photo" ? [9504, 6336] : [extent, extent] }
    var contactSamples: Int { name == "photo" ? 6 * Self.samplesPerSecond : Self.strokeSamples }
    var contactCycleSamples: Int { name == "photo" ? 7 * Self.samplesPerSecond : Self.cycleSamples }

    init(name: String, seconds: Double, color: [String: String] = ["space": "Srgb", "depth": "U8"],
        document: String? = nil, brush photoBrush: UInt32? = nil, diameter photoDiameter: Double? = nil, theme: String? = nil) throws {
        guard seconds.isFinite, seconds >= 1, seconds <= 1800 else {
            throw PlanError(errorDescription: "CAPY_WORKLOAD_SECONDS must be between 1 and 1800")
        }
        self.name = name; self.seconds = seconds; self.color = color
        self.document = name == "photo" ? document : nil
        guard theme == nil || theme == "light" || theme == "dark" else {
            throw PlanError(errorDescription: "CAPY_WORKLOAD_THEME must be light or dark")
        }
        self.theme = theme
        switch name {
        case "ink": identifier = 1; extent = 2048; paintLayers = 1; brush = 1; diameter = 24; predicts = false
        case "ink-predicted": identifier = 2; extent = 2048; paintLayers = 1; brush = 1; diameter = 24; predicts = true
        case "wet-watercolor": identifier = 3; extent = 2048; paintLayers = 1; brush = 21; diameter = 320; predicts = true
        case "layered-4k": identifier = 4; extent = 4096; paintLayers = 8; brush = 1; diameter = 24; predicts = true
        case "wet-watercolor-4k": identifier = 5; extent = 4096; paintLayers = 8; brush = 21; diameter = 320; predicts = true
        case "photo":
            guard seconds == 20, let document, document.hasPrefix("/"),
                URL(fileURLWithPath: document).pathExtension.lowercased() == "capy",
                let brush = photoBrush, brush > 0, let diameter = photoDiameter, diameter.isFinite, (0.5...2048).contains(diameter) else {
                throw PlanError(errorDescription: "photo requires 20 seconds, an absolute CAPY_WORKLOAD_DOCUMENT .capy path, CAPY_WORKLOAD_BRUSH and CAPY_WORKLOAD_DIAMETER between 0.5 and 2048")
            }
            identifier = 6; extent = 9504; paintLayers = 2
            self.brush = brush; self.diameter = diameter; predicts = false
        default: throw PlanError(errorDescription: "Unknown CAPY_WORKLOAD profile: \(name)")
        }
    }
    static func configured(environment: [String: String] = ProcessInfo.processInfo.environment) throws -> DrawingWorkloadPlan? {
        guard let name = environment["CAPY_WORKLOAD"] else { return nil }
        guard let seconds = Double(environment["CAPY_WORKLOAD_SECONDS"] ?? (name == "photo" ? "20" : "600")) else {
            throw PlanError(errorDescription: "CAPY_WORKLOAD_SECONDS must be a number")
        }
        if name == "photo" && (environment["CAPY_WORKLOAD_SPACE"] != nil || environment["CAPY_WORKLOAD_DEPTH"] != nil) {
            throw PlanError(errorDescription: "The photo workload uses the package document color")
        }
        return try DrawingWorkloadPlan(name: name, seconds: seconds, color: [
            "space": environment["CAPY_WORKLOAD_SPACE"] ?? "Srgb",
            "depth": environment["CAPY_WORKLOAD_DEPTH"] ?? "U8"],
            document: environment["CAPY_WORKLOAD_DOCUMENT"],
            brush: environment["CAPY_WORKLOAD_BRUSH"].flatMap(UInt32.init),
            diameter: environment["CAPY_WORKLOAD_DIAMETER"].flatMap(Double.init), theme: environment["CAPY_WORKLOAD_THEME"])
    }
    private struct PlanError: LocalizedError { let errorDescription: String? }
    var metadata: [String: Any] {
        var result: [String: Any] = ["version": 1, "name": name, "document_pixels": [extent, extent], "paint_layers": paintLayers,
         "document_color": color,
         "brush_id": brush, "diameter": diameter, "prediction": predicts, "sample_hz": Self.samplesPerSecond,
         "warmup_seconds": Self.warmupSeconds, "measurement_seconds": seconds, "input_source": "synthetic"]
        if let theme { result["theme"] = theme }
        if name == "photo" {
            result["document_pixels"] = documentExtent
            result.removeValue(forKey: "document_color")
            result["document_color_source"] = "package"
            result["document_source"] = "reference-photo-package"
            result["warmup_seconds"] = 0
            result["priming_contacts"] = 1
            result["priming_requires_undo"] = true
            result["measured_contacts"] = 3
            result["contact_seconds"] = 6
            result["lift_seconds"] = 1
            result["pressure"] = 1
        }
        return result
    }
    struct Sample {
        let contact: UInt64
        let x: Double, y: Double, pressure: Double, phase: Double
    }
    func sample(_ index: Int) -> Sample? {
        if name == "photo" && (index < 0 || index >= 20 * Self.samplesPerSecond) { return nil }
        let point = index % contactCycleSamples
        guard point < contactSamples else { return nil }
        let stroke = index / contactCycleSamples
        let t = Double(point) / Double(contactSamples - 1)
        let shift = Double(stroke % 13) * 0.37
        return Sample(contact: UInt64(stroke + 1),
            x: Double(extent) * (0.5 + 0.32 * sin(t * .pi * 2 + shift)),
            y: Double(name == "photo" ? 6336 : extent) * (0.5 + 0.27 * sin(t * .pi * 3 + shift * 0.7)),
            pressure: name == "photo" ? 1 : 0.25 + 0.75 * sin(t * .pi),
            phase: point == 0 ? 1 : point == contactSamples - 1 ? 3 : 2)
    }
}
