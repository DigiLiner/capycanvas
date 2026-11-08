import Foundation

/// The OS owns destination-profile conversion. These are observations of the
/// window's current screen, never document or export preferences.
struct DisplayDetails: Equatable {
    var screen = ""
    var destination = ""
}

extension Double {
    var clampedHeadroom: Double { isFinite ? min(100, max(1, self)) : 1 }
}
