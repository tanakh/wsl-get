fn main() {
    windows::build! {
        Windows::Win32::System::Com::CoTaskMemFree,
        Windows::Win32::System::LibraryLoader::{
            FreeLibrary,
            GetProcAddress,
            LoadLibraryExW,
        },
        Windows::Win32::System::SubsystemForLinux::*,
    };
}
