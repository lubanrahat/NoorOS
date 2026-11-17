//! Active x86_64 page tables.
//!
//! The bootloader leaves CR3 pointing at a four-level page-table hierarchy and
//! maps all physical memory at a fixed virtual offset. That offset lets the
//! kernel edit page tables by physical address:
//!
//! ```text
//! virtual address of a table = physical_memory_offset + table_physical_address
//! ```
//!
//! [`init`] reads CR3, and [`map_page`] / [`unmap_page`] / [`translate_addr`]
//! operate on that *active* hierarchy. Intermediate tables are allocated from
//! the bitmap frame allocator on demand; the mapper zeroes every frame before
//! it is used as a page table.
//!
//! All access is serialised by one mutex. NoorOS is single-core, and no IRQ
//! handler maps or translates pages, so a spin mutex is sufficient.

use spin::Mutex;
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::mapper::{MapToError, UnmapError};
use x86_64::structures::paging::{
    FrameAllocator, FrameDeallocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags,
    PhysFrame, Size4KiB, Translate,
};
use x86_64::{PhysAddr, VirtAddr};

use super::frame_allocator;

/// Standard page/frame size used by the kernel mappings.
pub const PAGE_SIZE: u64 = 4096;

/// A dedicated, currently unused virtual page used by the boot-time self-test.
/// It is in PML4 slot 96, far away from the physical mapping (slot 32), the
/// heap (slot 128) and the bootloader's dynamic mappings (low slots).
const SELF_TEST_PAGE: u64 = 0x0000_3000_0000_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagingError {
    NotInitialized,
    FrameAllocationFailed,
    PageAlreadyMapped,
    ParentEntryHugePage,
    PageNotMapped,
    InvalidFrameAddress,
}

struct PageTables {
    phys_offset: VirtAddr,
    pml4_phys: PhysAddr,
}

static TABLES: Mutex<Option<PageTables>> = Mutex::new(None);

/// Captures the active level-4 table from CR3.
pub fn init(phys_offset: VirtAddr) -> Result<(), PagingError> {
    if phys_offset.is_null() {
        return Err(PagingError::NotInitialized);
    }
    let (pml4_frame, _flags) = Cr3::read();
    let pml4_phys = pml4_frame.start_address();
    if pml4_phys.is_null() {
        return Err(PagingError::NotInitialized);
    }
    *TABLES.lock() = Some(PageTables {
        phys_offset,
        pml4_phys,
    });
    Ok(())
}

impl PageTables {
    /// Creates a mapper over the active hierarchy. The `'static` reference is
    /// sound because page-table frames live for the kernel's lifetime and every
    /// caller holds the `TABLES` mutex for the whole operation.
    fn mapper(&mut self) -> OffsetPageTable<'static> {
        let pml4_virtual = self.phys_offset + self.pml4_phys.as_u64();
        // SAFETY: CR3 points at a valid PML4, and the bootloader mapped all
        // physical memory at `phys_offset`.
        let pml4: &'static mut PageTable = unsafe { &mut *pml4_virtual.as_mut_ptr::<PageTable>() };
        // SAFETY: `pml4` is the active level-4 table and `phys_offset` is the
        // physical-memory mapping supplied by the bootloader.
        unsafe { OffsetPageTable::new(pml4, self.phys_offset) }
    }
}

/// Maps `virtual_page` to `physical_frame` with the given flags.
///
/// Any missing intermediate page tables are allocated from the bitmap frame
/// allocator and zeroed. The TLB entry for the page is flushed on success.
pub fn map_page(
    virtual_page: Page<Size4KiB>,
    physical_frame: PhysFrame<Size4KiB>,
    flags: PageTableFlags,
) -> Result<(), PagingError> {
    let mut guard = TABLES.lock();
    let tables = guard.as_mut().ok_or(PagingError::NotInitialized)?;
    let mut frames = frame_allocator::lock();
    let mut mapper = tables.mapper();

    // SAFETY: the frame allocator returns unique, unused frames; the caller
    // guarantees the virtual page is not used for another mapping.
    match unsafe { mapper.map_to(virtual_page, physical_frame, flags, &mut *frames) } {
        Ok(flush) => {
            flush.flush();
            Ok(())
        }
        Err(error) => Err(map_error(error)),
    }
}

/// Allocates a fresh frame, maps it at `virtual_page`, and returns it.
///
/// If mapping fails, the frame is returned to the allocator immediately.
pub fn map_page_alloc(
    virtual_page: Page<Size4KiB>,
    flags: PageTableFlags,
) -> Result<PhysFrame<Size4KiB>, PagingError> {
    let mut guard = TABLES.lock();
    let tables = guard.as_mut().ok_or(PagingError::NotInitialized)?;
    let mut frames = frame_allocator::lock();
    let frame = frames
        .allocate_frame()
        .ok_or(PagingError::FrameAllocationFailed)?;

    let mut mapper = tables.mapper();
    // SAFETY: `frame` is unique and unused; the caller owns `virtual_page`.
    match unsafe { mapper.map_to(virtual_page, frame, flags, &mut *frames) } {
        Ok(flush) => {
            flush.flush();
            Ok(frame)
        }
        Err(error) => {
            // SAFETY: the frame was never mapped because `map_to` failed.
            unsafe { frames.deallocate_frame(frame) };
            Err(map_error(error))
        }
    }
}

/// Removes the mapping for `virtual_page` and returns the physical frame.
///
/// The caller decides whether to free the frame. Page-table frames are kept for
/// reuse; this kernel never tears down a whole hierarchy.
pub fn unmap_page(virtual_page: Page<Size4KiB>) -> Result<PhysFrame<Size4KiB>, PagingError> {
    let mut guard = TABLES.lock();
    let tables = guard.as_mut().ok_or(PagingError::NotInitialized)?;
    let mut mapper = tables.mapper();

    match mapper.unmap(virtual_page) {
        Ok((frame, flush)) => {
            flush.flush();
            Ok(frame)
        }
        Err(error) => Err(unmap_error(error)),
    }
}

/// Translates a virtual address through the active hierarchy.
///
/// Huge pages are handled by the `x86_64` crate's `Translate` implementation.
pub fn translate_addr(addr: VirtAddr) -> Option<PhysAddr> {
    let mut guard = TABLES.lock();
    let tables = guard.as_mut()?;
    tables.mapper().translate_addr(addr)
}

/// Physical address of the active PML4 (for diagnostics).
pub fn pml4_phys() -> Option<PhysAddr> {
    TABLES.lock().as_ref().map(|tables| tables.pml4_phys)
}

/// Maps, translates and unmaps a scratch page, proving the active hierarchy is
/// writable and that TLB invalidation works.
pub fn self_test() -> Result<(), &'static str> {
    let page = Page::containing_address(VirtAddr::new(SELF_TEST_PAGE));
    if translate_addr(page.start_address()).is_some() {
        return Err("paging self-test page is already mapped");
    }

    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
    let frame = frame_allocator::allocate_frame().ok_or("paging self-test: no free data frame")?;
    if map_page(page, frame, flags).is_err() {
        // SAFETY: mapping failed, so the frame was never installed.
        unsafe { frame_allocator::deallocate_frame(frame) };
        return Err("paging self-test map failed");
    }

    let translated = translate_addr(page.start_address());
    if unmap_page(page).is_err() {
        // Keep the data frame allocated: it may still be mapped.
        return Err("paging self-test unmap failed");
    }

    // SAFETY: the mapping was removed and the frame came from `allocate_frame`.
    unsafe { frame_allocator::deallocate_frame(frame) };

    if translated != Some(frame.start_address()) {
        return Err("paging self-test translation mismatch");
    }
    if translate_addr(page.start_address()).is_some() {
        return Err("paging self-test left a stale mapping");
    }
    Ok(())
}

fn map_error(error: MapToError<Size4KiB>) -> PagingError {
    match error {
        MapToError::FrameAllocationFailed => PagingError::FrameAllocationFailed,
        MapToError::ParentEntryHugePage => PagingError::ParentEntryHugePage,
        MapToError::PageAlreadyMapped(_) => PagingError::PageAlreadyMapped,
    }
}

fn unmap_error(error: UnmapError) -> PagingError {
    match error {
        UnmapError::ParentEntryHugePage => PagingError::ParentEntryHugePage,
        UnmapError::PageNotMapped => PagingError::PageNotMapped,
        UnmapError::InvalidFrameAddress(_) => PagingError::InvalidFrameAddress,
    }
}
