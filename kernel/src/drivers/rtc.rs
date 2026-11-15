//! CMOS real-time clock.
//!
//! The RTC is read directly over ports 0x70/0x71. This gives the taskbar a real
//! wall clock without enabling the RTC periodic interrupt.

use x86_64::instructions::port::Port;

#[derive(Clone, Copy, Debug)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl DateTime {
    pub fn clock(&self) -> alloc::string::String {
        alloc::format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }

    /// Minute-resolution clock used by the status bar and taskbar.
    pub fn clock_hm(&self) -> alloc::string::String {
        alloc::format!("{:02}:{:02}", self.hour, self.minute)
    }

    pub fn date(&self) -> alloc::string::String {
        alloc::format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

const CMOS_ADDR: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;

fn cmos_read(reg: u8, addr: &mut Port<u8>, data: &mut Port<u8>) -> u8 {
    // SAFETY: CMOS index/data ports; bit 7 of the index disables NMI.
    unsafe {
        addr.write(0x80 | reg);
        data.read()
    }
}

fn bcd(value: u8) -> u8 {
    (value & 0x0f) + ((value >> 4) * 10)
}

/// Reads the current wall-clock time.
pub fn now() -> Option<DateTime> {
    let mut addr: Port<u8> = Port::new(CMOS_ADDR);
    let mut data: Port<u8> = Port::new(CMOS_DATA);

    // Wait until the update-in-progress flag clears (bounded retry).
    let mut guard = 0;
    while cmos_read(0x0A, &mut addr, &mut data) & 0x80 != 0 {
        guard += 1;
        if guard > 100_000 {
            return None;
        }
    }

    let second = cmos_read(0x00, &mut addr, &mut data);
    let minute = cmos_read(0x02, &mut addr, &mut data);
    let hour = cmos_read(0x04, &mut addr, &mut data);
    let day = cmos_read(0x07, &mut addr, &mut data);
    let month = cmos_read(0x08, &mut addr, &mut data);
    let year = cmos_read(0x09, &mut addr, &mut data);
    let status_b = cmos_read(0x0B, &mut addr, &mut data);

    let binary = status_b & 0x04 != 0;
    let hour24 = status_b & 0x02 != 0;

    let second = if binary { second } else { bcd(second) };
    let minute = if binary { minute } else { bcd(minute) };
    let mut hour = if binary { hour } else { bcd(hour & 0x7f) };
    let day = if binary { day } else { bcd(day) };
    let month = if binary { month } else { bcd(month) };
    let year = if binary { year } else { bcd(year) };

    if !hour24 {
        let pm = hour & 0x80 != 0;
        hour &= 0x7f;
        if pm && hour < 12 {
            hour += 12;
        }
        if !pm && hour == 12 {
            hour = 0;
        }
    }

    if !(1..=12).contains(&month)
        || month == 0
        || day == 0
        || day > 31
        || second > 59
        || minute > 59
    {
        return None;
    }

    Some(DateTime {
        year: 2000 + year as u16,
        month,
        day,
        hour: hour.min(23),
        minute,
        second: second.min(59),
    })
}
