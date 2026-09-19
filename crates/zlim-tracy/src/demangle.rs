//! Symbol demangling support.
//!
//! With the `tracy_demangle` cargo feature enabled, the profiler asks the program to demangle the
//! symbols it displays, because the demangler Tracy ships with understands the C++ ABI and not the
//! Rust one. The `___tracy_demangle` function below is the entry point the C++ client calls; this
//! crate exports it, so the profiled program does not have to register anything.
//!
//! Symbols are demangled with `rustc-demangle`. Anything that is not a Rust symbol is reported as
//! undemanglable, which makes the profiler display it exactly as it came out of the binary.

use core::ffi::{CStr, c_char};
use core::fmt;
use core::ptr::null;

// -----------------------------------------------------------------------------
// Buffer

/// The buffer a demangled symbol is written into.
struct Buffer(String);

impl fmt::Write for Buffer {
    #[inline]
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.push_str(s);
        Ok(())
    }

    #[inline]
    fn write_char(&mut self, c: char) -> fmt::Result {
        self.0.push(c);
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Demangling

/// Writes the demangled form of the Rust symbol `symbol` into `buffer`.
fn demangle(symbol: &str, buffer: &mut Buffer) -> fmt::Result {
    use core::fmt::Write;

    let Ok(demangled) = rustc_demangle::try_demangle(symbol) else {
        return Err(fmt::Error);
    };

    // The alternate flag elides the hash `rustc-demangle` appends by default.
    write!(buffer, "{demangled:#}")
}

/// Demangles a symbol for the profiler.
///
/// The profiler reuses the string it is handed and does not preserve its contents, so a single
/// buffer shared by all calls is enough. A null pointer, or a symbol that cannot be demangled, tells
/// the profiler to display the symbol as it is.
///
/// # Safety
///
/// `mangled` must be either null, or a pointer to a null-terminated string that stays valid for the
/// duration of the call.
#[unsafe(no_mangle)]
unsafe extern "C" fn ___tracy_demangle(mangled: *const c_char) -> *const c_char {
    static mut BUFFER: Buffer = Buffer(String::new());

    if mangled.is_null() {
        return null();
    }

    // SAFETY: the caller guarantees that `mangled` is a null-terminated string.
    let Ok(symbol) = unsafe { CStr::from_ptr(mangled) }.to_str() else {
        return null();
    };

    // SAFETY: `BUFFER` is only ever accessed here, and no reference to it escapes.
    let buffer = unsafe { &mut *core::ptr::addr_of_mut!(BUFFER) };
    buffer.0.clear();

    let result = || -> Result<(), fmt::Error> {
        demangle(symbol, buffer)?;

        // The profiler expects a null-terminated string without interior nulls. Anything else is
        // treated as a failure, so that the symbol is shown as it is.
        match buffer.0.as_bytes().split_last() {
            None | Some((&0, [])) => return Err(fmt::Error),
            Some((_, rest)) if rest.contains(&0) => return Err(fmt::Error),
            Some((&0, _)) => return Ok(()),
            Some(_) => {}
        }
        buffer.0.push('\0');
        Ok(())
    }();

    match result {
        Ok(()) => {
            debug_assert_eq!(buffer.0.as_bytes().last().copied(), Some(0));
            buffer.0.as_ptr().cast()
        }
        Err(fmt::Error) => {
            buffer.0.clear();
            null()
        }
    }
}
