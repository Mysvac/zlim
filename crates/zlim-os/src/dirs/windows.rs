extern crate windows_sys as windows;

use core::ffi::c_void;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows::Win32::UI::Shell;
use windows::Win32::UI::Shell::KF_FLAG_DEFAULT;
use windows::Win32::{Globalization, System};
use windows_sys::Win32::Foundation;

/// Returns the path to the directory used for application settings.
///
/// On Windows, this typically resolves to: `C:\Users\{UserName}\AppData\Roaming`
///
/// See <https://docs.rs/dirs/latest/dirs/fn.preference_dir.html> for details.
#[inline(never)] // Isolate system interface.
pub(super) fn preferences_dir() -> Option<PathBuf> {
    #[expect(unsafe_code, reason = "Uses unsafe Windows API functions")]
    unsafe {
        // rfid: The GUID that identifies the known folder.
        // FOLDERID_LocalAppData corresponds to the local (non-roaming) app data folder.
        const RFID: windows::core::GUID = Shell::FOLDERID_LocalAppData;

        // dwflags: Flags that specify special retrieval options for the known folder.
        // Using 0 (KF_FLAG_DEFAULT) retrieves the default path without any special behavior.
        const DWFLAGS: u32 = KF_FLAG_DEFAULT as u32;

        // hoken: A handle to an access token. Passing null indicates the current user.
        // Using null is equivalent to passing the token of the current thread's process.
        const HTOKEN: Foundation::HANDLE = core::ptr::null_mut();

        // ppszpath: Mutable pointer that will receive the allocated wide-string path.
        // The API will allocate memory on the heap; we must free it with CoTaskMemFree.
        let mut ppszpath: windows::core::PWSTR = core::ptr::null_mut();

        // Call the Windows Shell API to retrieve the known folder path.
        // Returns: HRESULT where 0 (S_OK) indicates success.
        let result = Shell::SHGetKnownFolderPath(&RFID, DWFLAGS, HTOKEN, &mut ppszpath);

        if result == 0 {
            // API call succeeded. The path_ptr is now a valid pointer to a UTF-16 null-terminated
            // string. Calculate the length (in characters) excluding the null terminator.
            let len = Globalization::lstrlenW(ppszpath) as usize;

            // Create a OsString from C Wide-String.
            let path: &[u16] = core::slice::from_raw_parts(ppszpath, len);
            let ostr: OsString = OsStringExt::from_wide(path);

            // Free the memory allocated by the Windows API.
            // CoTaskMemFree handles null pointers safely, so we don't need to check.
            System::Com::CoTaskMemFree(ppszpath as *const c_void);
            Some(PathBuf::from(ostr))
        } else {
            // Free the memory allocated by the Windows API.
            // CoTaskMemFree handles null pointers safely, so we don't need to check.
            System::Com::CoTaskMemFree(ppszpath as *const c_void);
            None
        }
    }
}
