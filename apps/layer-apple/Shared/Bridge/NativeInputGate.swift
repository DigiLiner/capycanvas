import os
import Synchronization

final class NativeInputGate: @unchecked Sendable {
    private let state: AnyObject
    init() {
        if #available(iOS 18.0, macOS 15.0, *) { state = AtomicInputGate() }
        else { state = LockedInputGate() }
    }
    func claim() -> Bool {
        if #available(iOS 18.0, macOS 15.0, *) {
            return unsafeDowncast(state, to: AtomicInputGate.self).value
                .compareExchange(expected: false, desired: true, ordering: .acquiringAndReleasing).exchanged
        }
        return unsafeDowncast(state, to: LockedInputGate.self).value.withLock {
            if $0 { return false }; $0 = true; return true
        }
    }
    func release() {
        if #available(iOS 18.0, macOS 15.0, *) {
            unsafeDowncast(state, to: AtomicInputGate.self).value.store(false, ordering: .releasing)
        } else {
            unsafeDowncast(state, to: LockedInputGate.self).value.withLock { $0 = false }
        }
    }
}

@available(iOS 18.0, macOS 15.0, *)
private final class AtomicInputGate {
    let value = Atomic<Bool>(false)
}

private final class LockedInputGate {
    let value = OSAllocatedUnfairLock(initialState: false)
}
