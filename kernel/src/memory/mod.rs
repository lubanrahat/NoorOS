//! Physical memory inspection and the kernel heap.
//!
//! NoorOS does **not** implement a virtual memory manager or demand paging. What
//! it does implement:
//!
//! * reads the bootloader's physical memory map and classifies every region,
//! * reports usable/reserved totals (used by `memory` in Noor Terminal),
//! * manages a fixed 8 MiB kernel heap in `.bss` with a linked-list allocator,
//!   which backs `alloc` types (`String`, `Vec`, ...) throughout the kernel.
//!
//! The framebuffer and kernel image are mapped by the bootloader; the kernel
//! never creates page tables of its own. This limitation is stated in the
//! report and in `memory` output.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU64, Ordering};

use alloc::vec::Vec;
use bootloader_api::info::{MemoryRegion, MemoryRegionKind};
use linked_list_allocator::LockedHeap;
use spin::Mutex;

use crate::util;

const HEAP_SIZE: usize = 8 * 1024 * 1024;

#[repr(align(4096))]
struct HeapStorage(UnsafeCell<[u8; HEAP_SIZE]>);

// SAFETY: the heap is only reachable through `ALLOCATOR`, which serialises all
// access. There is a single CPU and no re-entrant allocation from IRQ handlers.
unsafe impl Sync for HeapStorage {}

static HEAP: HeapStorage = HeapStorage(UnsafeCell::new([0; HEAP_SIZE]));

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

static HEAP_SIZE_USED: AtomicU64 = AtomicU64::new(HEAP_SIZE as u64);

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
    pub regions: Vec<Region>,
    pub usable_bytes: u64,
    pub reserved_bytes: u64,
    pub total_bytes: u64,
}

static MAP: Mutex<Option<&'static MemoryMap>> = Mutex::new(None);

/// Initialises the kernel heap. Called before anything allocates.
pub fn init_heap() {
    // SAFETY: `HEAP` is a unique static buffer, initialised exactly once and
    // never used for anything else.
    unsafe {
        ALLOCATOR.lock().init(HEAP.0.get() as *mut u8, HEAP_SIZE);
    }
    // Read the free count *before* formatting: the allocator lock must be
    // released before `util::bytes` allocates its own `String`, otherwise the
    // non-reentrant spinlock would deadlock.
    let free = ALLOCATOR.lock().free() as u64;
    serial_println!(
        "[kernel] heap: {} at {:p}, {} free",
        util::bytes(HEAP_SIZE as u64),
        HEAP.0.get(),
        util::bytes(free)
    );
}

/// Builds an immutable snapshot of the bootloader memory map.
pub fn init_map(regions: &[MemoryRegion]) -> &'static MemoryMap {
    let mut snapshot: Vec<Region> = Vec::with_capacity(regions.len());
    let mut usable = 0u64;
    let mut total = 0u64;

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
        total += region.end - region.start;
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

pub struct HeapStats {
    pub used: u64,
    pub free: u64,
    pub size: u64,
}

pub fn heap_stats() -> HeapStats {
    let heap = ALLOCATOR.lock();
    HeapStats {
        used: heap.used() as u64,
        free: heap.free() as u64,
        size: HEAP_SIZE_USED.load(Ordering::Relaxed),
    }
}

/// Heap size in bytes (kept in an atomic so the UI never needs a lock).
pub fn heap_capacity() -> u64 {
    HEAP_SIZE_USED.load(Ordering::Relaxed)
}
