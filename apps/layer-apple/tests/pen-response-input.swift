import Foundation

@main struct PenResponseInputChecks {
    static func main() throws {
        precondition(NativePenDevice.appKitKey(vendor: 9, unique: 123, serial: 456) == "appkit:9:tool:123")
        precondition(NativePenDevice.appKitKey(vendor: 9, unique: 0, serial: 456) == "appkit:9:serial:456")
        precondition(NativePenDevice.appKitKey(vendor: 9, unique: 0, serial: 0) == "appkit:pen")
        precondition(NativePenDevice.appKitKey(vendor: 0, unique: 0, serial: 0) == "appkit:pen")
        let frames: [(CGPoint, CGPoint, Bool, Bool, Bool)] = [
            (CGPoint(x: 1, y: 0), CGPoint(x: 0, y: 1), false, false, false),
            (CGPoint(x: 0, y: 1), CGPoint(x: -1, y: 0), true, false, true),
            (CGPoint(x: -1, y: 0), CGPoint(x: 0, y: -1), true, true, false),
            (CGPoint(x: 0, y: -1), CGPoint(x: 1, y: 0), false, true, true)
        ]
        for (x, y, horizontal, vertical, swap) in frames {
            var device = NativePenDevice(id: 0, key: "apple:pencil")
            device.align(nativeX: x, nativeY: y)
            let registration = try JSON.decode(device.action().encoded())
            precondition(registration["type"].string == "pen_input" && registration["sample"].isNull)
            let info = registration["device"], axes = info["native_to_surface"]
            precondition(info["id"].uint == 0 && info["key"].string == "apple:pencil" && info["label"].string.isEmpty)
            precondition(axes["reverse_horizontal"].bool == horizontal && axes["reverse_vertical"].bool == vertical && axes["swap"].bool == swap)
            let sample = [400.0, 300.0, 0.6, 0.4, 0.2, 0.0, 0.0, 1_000_000_000.0, 1.0]
            let preview = try JSON.decode(device.action(sample: sample).encoded())
            precondition(preview["sample"].array.map(\.number) == sample && preview["device"].stableKey == info.stableKey)
        }
        let diagonal = StylusTilt.towardBarrel(altitude: .pi / 6, azimuth: .pi / 4)
        precondition(abs(diagonal.x - 0.8860771237926136) < 1e-9 && abs(diagonal.y - 0.8860771237926136) < 1e-9)
        for azimuth in stride(from: -Double.pi, through: Double.pi, by: Double.pi / 4) {
            let altitude = Double.pi / 6
            let tilt = StylusTilt.towardBarrel(altitude: altitude, azimuth: azimuth)
            precondition(abs(tan(tilt.x) - cos(azimuth) / tan(altitude)) < 1e-9)
            precondition(abs(tan(tilt.y) - sin(azimuth) / tan(altitude)) < 1e-9)
        }
        print("PASS pen response: owner action encoding, sensor records, stable source keys and four native-to-view coordinate frames")
    }
}
