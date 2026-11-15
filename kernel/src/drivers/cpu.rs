//! CPU identification through the `CPUID` instruction.
//!
//! Everything shown by Noor Monitor comes from CPUID itself - nothing is
//! hard-coded or estimated.

use core::arch::x86_64::__cpuid;

use alloc::string::ToString;
use spin::Once;

#[derive(Clone, Debug)]
pub struct CpuInfo {
    pub vendor: alloc::string::String,
    pub brand: alloc::string::String,
    pub logical_cpus: u32,
    pub family: u32,
    pub model: u32,
    pub hypervisor: bool,
}

static CPU_INFO: Once<CpuInfo> = Once::new();

fn cpuid(leaf: u32) -> core::arch::x86_64::CpuidResult {
    // SAFETY: CPUID is always available on x86_64 and has no side effects.
    __cpuid(leaf)
}

fn brand_string() -> alloc::string::String {
    use alloc::string::{String, ToString};
    let max = cpuid(0x8000_0000).eax;
    if max < 0x8000_0004 {
        return String::from("Not reported by CPUID");
    }
    let mut bytes = [0u8; 48];
    for (i, leaf) in (0x8000_0002..=0x8000_0004).enumerate() {
        let r = cpuid(leaf);
        for (j, word) in [r.eax, r.ebx, r.ecx, r.edx].iter().enumerate() {
            bytes[i * 16 + j * 4..i * 16 + j * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
    }
    let text = String::from_utf8_lossy(&bytes).trim().to_string();
    if text.is_empty() {
        String::from("Not reported by CPUID")
    } else {
        text
    }
}

fn detect() -> CpuInfo {
    let base = cpuid(0);
    let mut vendor = [0u8; 12];
    vendor[0..4].copy_from_slice(&base.ebx.to_le_bytes());
    vendor[4..8].copy_from_slice(&base.edx.to_le_bytes());
    vendor[8..12].copy_from_slice(&base.ecx.to_le_bytes());

    let features = cpuid(1);
    let logical_cpus = (features.ebx >> 16) & 0xff;
    let family = ((features.eax >> 8) & 0xf)
        + if (features.eax >> 8) & 0xf == 0xf {
            (features.eax >> 20) & 0xff
        } else {
            0
        };
    let model = {
        let base_model = (features.eax >> 4) & 0xf;
        let ext_model = (features.eax >> 16) & 0xf;
        if ((features.eax >> 8) & 0xf) == 0x6 || ((features.eax >> 8) & 0xf) == 0xf {
            (ext_model << 4) | base_model
        } else {
            base_model
        }
    };

    CpuInfo {
        vendor: alloc::string::String::from_utf8_lossy(&vendor).to_string(),
        brand: brand_string(),
        logical_cpus: logical_cpus.max(1),
        family,
        model,
        hypervisor: features.ecx & (1 << 31) != 0,
    }
}

pub fn info() -> &'static CpuInfo {
    CPU_INFO.call_once(detect)
}
