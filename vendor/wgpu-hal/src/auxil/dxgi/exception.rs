use alloc::{borrow::Cow, string::String};

use parking_lot::Mutex;
use windows::Win32::{Foundation, System::Diagnostics::Debug};

static EXCEPTION_HANDLER: Mutex<Handler> = Mutex::new(Handler { users: 0, token: 0 });

struct Handler { users: usize, token: usize }
impl Handler {
    fn acquire(&mut self) -> bool {
        if self.users == 0 {
            self.token = unsafe { Debug::AddVectoredExceptionHandler(0, Some(output_debug_string_handler)) } as usize;
            if self.token == 0 { return false; }
        }
        self.users += 1;
        true
    }
    fn release(&mut self) {
        self.users -= 1;
        if self.users == 0 {
            unsafe { Debug::RemoveVectoredExceptionHandler(self.token as *const _) };
            self.token = 0;
        }
    }
}

pub struct Registration;
impl Registration {
    pub fn new() -> Option<Self> { EXCEPTION_HANDLER.lock().acquire().then(|| Self) }
}
impl Drop for Registration {
    fn drop(&mut self) { EXCEPTION_HANDLER.lock().release(); }
}

const MESSAGE_PREFIXES: &[(&str, log::Level)] = &[
    ("CORRUPTION", log::Level::Error),
    ("ERROR", log::Level::Error),
    ("WARNING", log::Level::Warn),
    // We intentionally suppress "INFO" messages down to debug
    // so that users are not innundated with info messages from the runtime.
    ("INFO", log::Level::Debug),
    ("MESSAGE", log::Level::Trace),
];

unsafe extern "system" fn output_debug_string_handler(
    exception_info: *mut Debug::EXCEPTION_POINTERS,
) -> i32 {
    std::panic::catch_unwind(|| unsafe { handle_debug_string(exception_info) })
        .unwrap_or(Debug::EXCEPTION_CONTINUE_SEARCH)
}

unsafe fn debug_message(record: &Debug::EXCEPTION_RECORD) -> Option<Cow<'_, str>> {
    if record.NumberParameters < 2 { return None; }
    let [length, pointer, ..] = record.ExceptionInformation;
    if length == 0 || pointer == 0 { return None; }
    match record.ExceptionCode {
        Foundation::DBG_PRINTEXCEPTION_C if length <= isize::MAX as usize => {
            Some(String::from_utf8_lossy(unsafe { core::slice::from_raw_parts(pointer as *const u8, length) }))
        }
        Foundation::DBG_PRINTEXCEPTION_WIDE_C if length <= isize::MAX as usize / 2 && pointer.is_multiple_of(2) => {
            Some(Cow::Owned(String::from_utf16_lossy(unsafe { core::slice::from_raw_parts(pointer as *const u16, length) })))
        }
        _ => None,
    }
}

fn message_parts(message: &str) -> Option<(&str, log::Level)> {
    let message = message.strip_prefix("D3D12 ")?
        .trim_end_matches(['\n', '\r', '\0'])
        .trim_end_matches("[ STATE_CREATION WARNING #0: UNKNOWN]");
    let (message, level) = MESSAGE_PREFIXES.iter().find_map(|&(prefix, level)| {
        message.strip_prefix(prefix)?.strip_prefix(": ").map(|message| (message, level))
    }).unwrap_or((message, log::Level::Debug));
    if level == log::Level::Warn && (message.contains("#82") || message.contains("DRAW_EMPTY_SCISSOR_RECTANGLE")) {
        return None;
    }
    Some((message, level))
}

unsafe fn handle_debug_string(exception_info: *mut Debug::EXCEPTION_POINTERS) -> i32 {
    let record = unsafe { &*(*exception_info).ExceptionRecord };
    let Some(message) = (unsafe { debug_message(record) }) else {
        return Debug::EXCEPTION_CONTINUE_SEARCH;
    };
    let Some((message, level)) = message_parts(&message) else {
        return Debug::EXCEPTION_CONTINUE_SEARCH;
    };
    log::log!(level, "{message}");

    #[cfg(feature = "validation_canary")]
    if cfg!(debug_assertions) && level == log::Level::Error {
        use alloc::string::ToString as _;

        // Set canary and continue
        crate::VALIDATION_CANARY.add(message.to_string());
    }

    Debug::EXCEPTION_CONTINUE_EXECUTION
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn registration_is_shared_and_released_by_token() {
        let mut handler = Handler { users: 0, token: 0 };
        assert!(handler.acquire());
        let token = handler.token;
        assert_ne!(token, 0);
        assert!(handler.acquire());
        assert_eq!(handler.token, token);
        handler.release();
        assert_eq!((handler.users, handler.token), (1, token));
        handler.release();
        assert_eq!((handler.users, handler.token), (0, 0));
        assert_eq!(unsafe { Debug::RemoveVectoredExceptionHandler(token as *const _) }, 0);
    }

    #[test]
    fn diagnostic_payloads_use_the_pointed_to_ansi_or_utf16_text() {
        let ansi = b"D3D12 ERROR: broken\0";
        let wide: Vec<_> = "D3D12 WARNING: 猫 🦫\0".encode_utf16().collect();
        let mut record = Debug::EXCEPTION_RECORD { ExceptionCode: Foundation::DBG_PRINTEXCEPTION_C,
            NumberParameters: 2, ..Default::default() };
        record.ExceptionInformation[..2].copy_from_slice(&[ansi.len(), ansi.as_ptr() as usize]);
        assert_eq!(unsafe { debug_message(&record) }.unwrap(), "D3D12 ERROR: broken\0");
        record.ExceptionCode = Foundation::DBG_PRINTEXCEPTION_WIDE_C;
        record.NumberParameters = 4;
        record.ExceptionInformation[..2].copy_from_slice(&[wide.len(), wide.as_ptr() as usize]);
        assert_eq!(unsafe { debug_message(&record) }.unwrap(), "D3D12 WARNING: 猫 🦫\0");
        record.ExceptionInformation[0] = usize::MAX;
        assert!(unsafe { debug_message(&record) }.is_none());
    }

    #[test]
    fn diagnostic_prefixes_cannot_slice_past_short_or_unicode_messages() {
        for message in ["D3D12 ERROR", "D3D12 ERROR:", "D3D12 ERRORé", "D3D12 "] {
            assert!(message_parts(message).is_some());
        }
        assert_eq!(message_parts("D3D12 ERROR: 猫\n\0"), Some(("猫", log::Level::Error)));
        assert!(message_parts("unrelated").is_none());
        assert!(message_parts("D3D12 WARNING: #820").is_none());
        assert!(message_parts("D3D12 WARNING: DRAW_EMPTY_SCISSOR_RECTANGLE").is_none());
    }
}
