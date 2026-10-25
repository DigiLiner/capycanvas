import Foundation

struct NativePenDevice: Equatable, Sendable {
    let id: UInt64
    let key: String
    var label = ""
    var reverseHorizontal = false
    var reverseVertical = false
    var swap = false
    static func appKitKey(vendor: UInt64, unique: UInt64, serial: UInt64) -> String {
        if unique != 0 { return "appkit:\(vendor):tool:\(unique)" }
        if serial != 0 { return "appkit:\(vendor):serial:\(serial)" }
        return "appkit:pen"
    }
    mutating func align(nativeX: CGPoint, nativeY: CGPoint) {
        swap = abs(nativeX.y) > abs(nativeX.x)
        reverseHorizontal = swap ? nativeY.x < 0 : nativeX.x < 0
        reverseVertical = swap ? nativeX.y < 0 : nativeY.y < 0
    }
    func action(sample: [Double]? = nil) -> JSON {
        JSON(["type": "pen_input", "device": ["id": id, "key": key, "label": label,
            "native_to_surface": ["reverse_horizontal": reverseHorizontal, "reverse_vertical": reverseVertical, "swap": swap]],
            "sample": sample.map { $0 as Any } ?? NSNull()])
    }
}

enum StylusTilt {
    static func towardBarrel(altitude: CGFloat, azimuth: CGFloat) -> (x: Double, y: Double) {
        (Double(atan2(cos(altitude) * cos(azimuth), sin(altitude))), Double(atan2(cos(altitude) * sin(azimuth), sin(altitude))))
    }
    static func towardBarrel(appKit tilt: CGPoint) -> (x: Double, y: Double) {
        (Double(tilt.x) * .pi / 2, -Double(tilt.y) * .pi / 2)
    }
}
