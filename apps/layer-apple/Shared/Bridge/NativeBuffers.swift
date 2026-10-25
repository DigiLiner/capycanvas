import Foundation

func nativeHandle(_ value: UInt64) -> UInt64? { value == 0 ? nil : value }
func nativeValue(_ value: UInt64?) -> UInt64 { value ?? 0 }

func takeNativeText(_ value: UInt64) -> String? {
    guard value != 0 else { return nil }
    defer { capy_apple_string_free(value) }
    let count = capy_apple_string_length(value)
    guard count >= 0, capy_apple_string_copy(value, count, nil, 0) else { return nil }
    var valid = true
    let text = String(unsafeUninitializedCapacity: count) { buffer in
        var offset = 0
        while offset < count {
            let length = min(8 * 1024 * 1024, count - offset)
            guard let base = buffer.baseAddress,
                capy_apple_string_copy(value, offset, base.advanced(by: offset), length) else {
                valid = false
                return 0
            }
            offset += length
        }
        return count
    }
    return valid ? text : nil
}

func copyNativeData(_ count: Int, copy: (Int, UnsafeMutablePointer<UInt8>?, Int) -> Bool) -> Data? {
    guard count >= 0, copy(count, nil, 0) else { return nil }
    var data = Data(count: count)
    let valid = data.withUnsafeMutableBytes { bytes in
        var offset = 0
        while offset < count {
            let length = min(8 * 1024 * 1024, count - offset)
            guard let base = bytes.bindMemory(to: UInt8.self).baseAddress,
                copy(offset, base.advanced(by: offset), length) else { return false }
            offset += length
        }
        return true
    }
    return valid ? data : nil
}
