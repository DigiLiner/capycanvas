import Foundation

struct BubbleRamp {
    private(set) var value: Double
    private(set) var target: Double
    private var from: Double
    private var start: TimeInterval?
    private var duration: TimeInterval = 0
    private var delay: TimeInterval = 0
    var active: Bool { value != target }
    init(value: Double) { self.value = value; target = value; from = value }
    mutating func set(_ target: Double, duration: TimeInterval, delay: TimeInterval = 0) {
        guard self.target != target else { return }
        from = value; self.target = target; self.duration = duration; self.delay = delay; start = nil
    }
    mutating func tick(_ now: TimeInterval, reduced: Bool) -> Double {
        guard active else { return value }
        if start == nil { start = now }
        let t = reduced || duration <= 0 ? 1 : min(1, max(0, (now - (start ?? now) - delay) / duration))
        value = t == 1 ? target : from + (target - from) * (1 - pow(1 - t, 3))
        return value
    }
}
