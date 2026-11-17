//! Physical frames, active page tables and the kernel heap.
//!
//! NoorOS now has a real memory subsystem:
//!
//! * [`frame_allocator`] parses the bootloader's `MemoryRegions` and manages
//!   4 KiB physical frames with a bitmap,
//! * [`paging`] edits the active level-4 page table through the bootloader's
//!   physical-memory offset (`map_page`, `unmap_page`, `translate_addr`),
//! * [`heap`] is a contiguous virtual range whose pages are mapped on demand,
//!   backing `alloc` types (`String`, `Vec`, ...) throughout the kernel.
//!
//! The framebuffer and kernel image are still mapped by the bootloader. The
//! kernel now creates its own page-table entries for the heap and any other
//! mapping requested through this module.

mod frame_allocator;
mod heap;
mod paging;

use alloc::vec::Vec;
use bootloader_api::BootInfo;
use bootloader_api::info::{MemoryRegion, MemoryRegionKind};
use spin::Mutex;
use x86_64::VirtAddr;

use crate::util;

#[allow(unused_imports)]
pub use frame_allocator::{FRAME_SIZE, FrameStats};
pub use heap::{HEAP_BASE, HeapStats};
#[allow(unused_imports)]
pub use paging::{
    PAGE_SIZE, PagingError, map_page, map_page_alloc, pml4_phys, translate_addr, unmap_page,
};
#[allow(unused_imports)]
pub use x86_64::structures::paging::{Page, PageTableFlags, PhysFrame, Size4KiB};

/// Fixed virtual offset at which the bootloader maps all physical memory.
/// Must match `BOOTLOADER_CONFIG` in `main.rs`.
pub const PHYSICAL_MEMORY_OFFSET: u64 = 0x0000_1000_0000_0000;

/// Classified view of one physical memory region.
#[derive(Clone, Copy)]
pub struct Region {
    pub start: u64,
    pub len: u64,
    pub kind: RegionKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    /// Conventional memory the kernel is allowed to use.
    Usable,
    /// Memory used by the bootloader (page tables, boot info, ...).
    Bootloader,
    /// Firmware-reserved memory; the tag is the raw E820/UEFI type.
    Reserved(u32),
}

impl RegionKind {
    pub fn label(&self) -> &'static str {
        match self {
            RegionKind::Usable => "Usable",
            RegionKind::Bootloader => "Bootloader",
            RegionKind::Reserved(_) => "Reserved",
        }
    }

    pub fn detail(&self) -> alloc::string::String {
        match self {
            RegionKind::Reserved(tag) => alloc::format!("Reserved (type {tag})"),
            other => alloc::string::String::from(other.label()),
        }
    }
}

pub struct MemoryMap {
    /// Every region reported by firmware, including reserved MMIO.
    pub regions: Vec<Region>,
    /// Sum of the usable regions.
    pub usable_bytes: u64,
    /// Installed RAM below `total_bytes` that is not usable.
    pub reserved_bytes: u64,
    /// Highest usable physical address: installed RAM, not the map's span.
    pub total_bytes: u64,
}

static MAP: Mutex<Option<&'static MemoryMap>> = Mutex::new(None);

/// Initialises the frame allocator, the active page tables and the heap.
///
/// Must run before anything allocates. The bootloader must be configured with a
/// physical-memory mapping; [`PHYSICAL_MEMORY_OFFSET`] is the address NoorOS
/// requests.
pub fn init(boot_info: &BootInfo) -> Result<(), &'static str> {
    let offset = boot_info
        .physical_memory_offset
        .into_option()
        .ok_or("the bootloader did not map physical memory")?;
    let phys_offset = VirtAddr::new(offset);
    serial_println!("[kernel] memory init: physical memory offset {:#X}", offset);

    frame_allocator::init(&boot_info.memory_regions, phys_offset)?;
    serial_println!("[kernel] frame allocator ready");
    frame_allocator::self_test()?;
    serial_println!("[kernel] frame allocator self-test: allocate/deallocate OK");
    paging::init(phys_offset).map_err(|_| "cannot initialise the active page tables")?;
    serial_println!("[kernel] active page tables captured from CR3");
    heap::init()?;
    serial_println!("[kernel] initial heap pages mapped");

    // Prove that mapping, translation and unmapping work on the live table.
    paging::self_test()?;

    let frames = frame_allocator::stats();
    let heap = heap::stats();
    serial_println!(
        "[kernel] paging: active PML4 at {:#X}, physical offset {:#X}",
        pml4_phys().map(|address| address.as_u64()).unwrap_or(0),
        phys_offset.as_u64()
    );
    serial_println!(
        "[kernel] frames: {} usable of {} addressable, {} used, {} free ({} KiB each)",
        frames.usable,
        frames.total,
        frames.used,
        frames.free,
        FRAME_SIZE / 1024
    );
    serial_println!(
        "[kernel] heap: {} mapped at {:#X}, {} usable, {} used",
        util::bytes(heap.mapped),
        heap.base,
        util::bytes(heap.size),
        util::bytes(heap.used)
    );
    serial_println!("[kernel] paging self-test: map/translate/unmap OK");
    Ok(())
}

/// Builds an immutable snapshot of the bootloader memory map.
pub fn init_map(regions: &[MemoryRegion]) -> &'static MemoryMap {
    let mut snapshot: Vec<Region> = Vec::with_capacity(regions.len());
    let mut usable = 0u64;
    // Physical RAM ends at the highest usable address. Firmware also reports
    // large reserved MMIO ranges (QEMU's PCI hole ends at 1 TiB); counting
    // those as "RAM" would make the totals meaningless.
    let total = regions
        .iter()
        .filter(|region| region.kind == MemoryRegionKind::Usable)
        .map(|region| region.end)
        .max()
        .unwrap_or(0);

    for region in regions {
        let kind = match region.kind {
            MemoryRegionKind::Usable => RegionKind::Usable,
            MemoryRegionKind::Bootloader => RegionKind::Bootloader,
            MemoryRegionKind::UnknownBios(tag) => RegionKind::Reserved(tag),
            MemoryRegionKind::UnknownUefi(tag) => RegionKind::Reserved(tag),
            _ => RegionKind::Reserved(u32::MAX),
        };
        if kind == RegionKind::Usable {
            usable += region.end - region.start;
        }
        snapshot.push(Region {
            start: region.start,
            len: region.end - region.start,
            kind,
        });
    }

    let map = MemoryMap {
        regions: snapshot,
        usable_bytes: usable,
        reserved_bytes: total.saturating_sub(usable),
        total_bytes: total,
    };
    let leaked: &'static MemoryMap = alloc::boxed::Box::leak(alloc::boxed::Box::new(map));
    *MAP.lock() = Some(leaked);

    serial_println!(
        "[kernel] memory map: {} regions, {} usable, {} reserved, {} total",
        leaked.regions.len(),
        util::bytes(leaked.usable_bytes),
        util::bytes(leaked.reserved_bytes),
        util::bytes(leaked.total_bytes)
    );
    for region in &leaked.regions {
        serial_println!(
            "  {:<16} {:#014X} - {:#014X}  {}",
            region.kind.detail(),
            region.start,
            region.start + region.len,
            util::bytes(region.len)
        );
    }
    leaked
}

pub fn map() -> Option<&'static MemoryMap> {
    *MAP.lock()
}

/// Physical-frame usage. `usable` is the number of frames that were free after
/// the bitmap's own storage was reserved; `used` counts frames handed out since.
pub fn frame_stats() -> FrameStats {
    frame_allocator::stats()
}

pub fn heap_stats() -> HeapStats {
    heap::stats()
}

/// Usable heap size in bytes. The heap grows on demand, so this can increase
/// while the kernel is running.
pub fn heap_capacity() -> u64 {
    heap::stats().size
}

pub fn heap_base() -> VirtAddr {
    VirtAddr::new(HEAP_BASE)
}
