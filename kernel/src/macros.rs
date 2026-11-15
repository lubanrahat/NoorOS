//! Console macros available everywhere in the kernel.
//!
//! They write to COM1 (see [`crate::drivers::serial`]). Logging never panics: if
//! the UART was not initialised the message is dropped.

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::drivers::serial::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! serial_println {
    () => {
        $crate::drivers::serial::_print(format_args!("\n"))
    };
    ($($arg:tt)*) => {
        $crate::drivers::serial::_print(format_args!("{}\n", format_args!($($arg)*)))
    };
}
