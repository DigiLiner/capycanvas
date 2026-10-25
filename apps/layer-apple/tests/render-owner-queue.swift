import Foundation

#if os(Linux)
func autoreleasepool<T>(invoking work: () throws -> T) rethrows -> T { try work() }
#endif

private final class Calls: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [Int] = []
    private var threads = Set<ObjectIdentifier>()
    func record(_ value: Int) {
        lock.lock(); defer { lock.unlock() }
        precondition(!Thread.isMainThread)
        values.append(value); threads.insert(ObjectIdentifier(Thread.current))
    }
    func check(_ expected: [Int]) {
        lock.lock(); defer { lock.unlock() }
        precondition(values == expected, "Unexpected owner order: \(values)")
        precondition(threads.count == 1, "Native owner moved between threads")
    }
}

@main enum RenderOwnerQueueChecks {
    static func wait(_ signal: DispatchSemaphore) {
        precondition(signal.wait(timeout: .now() + .seconds(5)) == .success, "Owner did not finish")
    }
    static func main() {
        var queue: RenderOwnerQueue? = RenderOwnerQueue()
        let calls = Calls(), entered = DispatchSemaphore(value: 0), release = DispatchSemaphore(value: 0)
        queue!.suspend()
        queue!.async { calls.record(0); entered.signal(); wait(release) }
        precondition(entered.wait(timeout: .now() + .milliseconds(50)) == .timedOut)
        queue!.resume(); wait(entered)
        for value in 1...512 { queue!.async { calls.record(value) } }
        release.signal()
        let delayed = DispatchSemaphore(value: 0)
        queue!.asyncAfter(deadline: .now() + .milliseconds(10)) { calls.record(513); delayed.signal() }
        wait(delayed)
        let finished = DispatchSemaphore(value: 0)
        queue!.finish { calls.record(514); finished.signal() }
        queue!.async { calls.record(-1) }
        wait(finished); calls.check(Array(0...514))
        let released = { [weak queue] in queue == nil }
        queue = nil; precondition(released())
        let drained = DispatchSemaphore(value: 0), cleanup = Calls()
        var suspended: RenderOwnerQueue? = RenderOwnerQueue()
        suspended!.suspend(); suspended!.async { cleanup.record(1); drained.signal() }
        suspended = nil; wait(drained); cleanup.check([1])
        print("PASS render owner: fixed thread, startup gate, FIFO, delayed work, final cleanup and suspended retirement")
    }
}
