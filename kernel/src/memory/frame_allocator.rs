//! Bitmap-based physical frame allocator.
//!
//! The bootloader hands the kernel a map of physical memory. Every frame that
//! the map does not mark `Usable` (kernel image, bootloader data, firmware
//! reservations, MMIO) is set to "used" in the bitmap. The allocator then owns
//! the remaining 4 KiB frames and can hand them out for page tables and the
//! kernel heap.
//!
//! The bitmap itself is not a static array: its size depends on the machine's
//! highest usable frame, so it is placed into a reserved run of usable RAM
//! during [`init`]. Those frames are marked used before the first allocation,
//! which means the bitmap can never be handed back to a caller.
//!
//! Bit set = used, bit clear = free. All state is behind a spin mutex; NoorOS
//! runs on one CPU, and allocation happens with interrupts enabled only in the
//! desktop loop, where no IRQ handler allocates memory.

use core::ptr;

use bootloader_api::info::{MemoryRegion, MemoryRegionKind};
use spin::{Mutex, MutexGuard};
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator, PhysFrame, Size4KiB};
use x86_64::{PhysAddr, VirtAddr};

/// Every frame managed by this allocator is a standard 4 KiB page.
pub const FRAME_SIZE: u64 = 4096;

/// The first MiB holds the real-mode IVT/BDA, the EBDA and VGA memory on many
/// machines. Firmware usually reports parts of it as usable, but the kernel
/// never needs it, so it is kept out of the allocator.
const RESERVED_LOW_MEMORY: u64 = 0x10_0000;

pub struct FrameStats {
    /// Frames covered by the bitmap (up to the highest usable frame).
    pub total: u64,
    /// Frames that were free after the bitmap's own storage was reserved.
    pub usable: u64,
    /// Frames currently handed out by the kernel.
    pub used: u64,
    /// Frames still available.
    pub free: u64,
}

pub(crate) struct BitmapAllocator {
    /// Virtual address of the first bitmap word (through the phys mapping).
    words_addr: usize,
    words_len: usize,
    total_frames: usize,
    usable_frames: usize,
    free_frames: usize,
    /// Rotating search start, so repeated allocations do not rescan frame 0.
    hint: usize,
    phys_offset: VirtAddr,
    ready: bool,
}

impl BitmapAllocator {
    const fn empty() -> Self {
        Self {
            words_addr: 0,
            words_len: 0,
            total_frames: 0,
            usable_frames: 0,
            free_frames: 0,
            hint: 0,
            phys_offset: VirtAddr::new(0),
            ready: false,
        }
    }

    fn init(
        &mut self,
        regions: &[MemoryRegion],
        phys_offset: VirtAddr,
    ) -> Result<(), &'static str> {
        if regions.is_empty() {
            return Err("the bootloader supplied an empty memory map");
        }
        if phys_offset.is_null() {
            return Err("the physical memory offset is zero");
        }

        // Only usable RAM needs a bitmap. Firmware can report huge reserved
        // MMIO ranges (QEMU reports a 12 GiB PCI hole ending at 1 TiB); sizing
        // the bitmap to those addresses would waste tens of MiB and make the
        // free-frame scan needlessly long. The managed range therefore ends at
        // the highest *usable* frame; reserved holes below it stay marked used.
        let total_frames = regions
            .iter()
            .filter(|region| region.kind == MemoryRegionKind::Usable)
            .map(|region| region.end / FRAME_SIZE)
            .max()
            .unwrap_or(0) as usize;
        if total_frames == 0 {
            return Err("the memory map contains no addressable frames");
        }

        let words_len = total_frames.div_ceil(64);
        let bitmap_bytes = words_len * core::mem::size_of::<u64>();

        // Pick the largest usable region. It is the least likely to sit in the
        // low 1 MiB and it can certainly hold the bitmap on normal machines.
        let mut chosen: Option<(u64, u64)> = None; // (aligned physical start, region length)
        for region in regions {
            if region.kind != MemoryRegionKind::Usable {
                continue;
            }
            let start = align_up(region.start, core::mem::align_of::<u64>() as u64);
            let Some(end) = start.checked_add(bitmap_bytes as u64) else {
                continue;
            };
            if end > region.end {
                continue;
            }
            if chosen.is_none_or(|(_, best_len)| region.end - region.start > best_len) {
                chosen = Some((start, region.end - region.start));
            }
        }
        let Some((bitmap_phys, _)) = chosen else {
            return Err("no usable region is large enough for the frame bitmap");
        };

        serial_println!(
            "[kernel] frame bitmap: {} frames, {} bytes at physical {:#X}",
            total_frames,
            bitmap_bytes,
            bitmap_phys
        );

        let bitmap_virt = phys_offset + bitmap_phys;
        // Start with every frame marked used, then free the usable ranges.
        // SAFETY: `bitmap_virt` points into a usable physical region that this
        // allocator reserves below; no other code writes to those frames.
        unsafe {
            ptr::write_bytes(bitmap_virt.as_mut_ptr::<u8>(), 0xFF, bitmap_bytes);
        }

        self.words_addr = bitmap_virt.as_u64() as usize;
        self.words_len = words_len;
        self.total_frames = total_frames;
        self.phys_offset = phys_offset;
        self.hint = 0;
        self.ready = true;

        for region in regions {
            if region.kind != MemoryRegionKind::Usable {
                continue;
            }
            let first = align_up(region.start, FRAME_SIZE) / FRAME_SIZE;
            let end = (region.end / FRAME_SIZE).min(total_frames as u64);
            for frame in first..end {
                self.set_used(frame as usize, false);
            }
        }

        // Never allocate page zero or the legacy low-memory area.
        self.mark_range_used(0, (RESERVED_LOW_MEMORY / FRAME_SIZE) as usize);

        // Reserve the frames that hold the bitmap itself.
        let bitmap_first = (bitmap_phys / FRAME_SIZE) as usize;
        let bitmap_last = ((bitmap_phys + bitmap_bytes as u64).div_ceil(FRAME_SIZE)) as usize;
        self.mark_range_used(bitmap_first, bitmap_last);

        self.free_frames = (0..self.total_frames)
            .filter(|&frame| !self.is_used(frame))
            .count();
        self.usable_frames = self.free_frames;

        Ok(())
    }

    #[inline]
    fn words(&self) -> &[u64] {
        // SAFETY: only called while `ready`, when `words_addr`/`words_len`
        // describe the reserved bitmap. The allocator never frees it.
        unsafe { core::slice::from_raw_parts(self.words_addr as *const u64, self.words_len) }
    }

    #[inline]
    fn words_mut(&mut self) -> &mut [u64] {
        // SAFETY: as above; `&mut self` guarantees exclusive access.
        unsafe { core::slice::from_raw_parts_mut(self.words_addr as *mut u64, self.words_len) }
    }

    #[inline]
    fn is_used(&self, frame: usize) -> bool {
        let word = self.words()[frame / 64];
        word & (1u64 << (frame % 64)) != 0
    }

    #[inline]
    fn set_used(&mut self, frame: usize, used: bool) {
        let word = frame / 64;
        let bit = 1u64 << (frame % 64);
        if used {
            self.words_mut()[word] |= bit;
        } else {
            self.words_mut()[word] &= !bit;
        }
    }

    fn mark_range_used(&mut self, first: usize, end: usize) {
        for frame in first..end.min(self.total_frames) {
            self.set_used(frame, true);
        }
    }

    fn allocate(&mut self) -> Option<PhysFrame<Size4KiB>> {
        if !self.ready || self.free_frames == 0 {
            return None;
        }

        let mut found = None;
        for frame in self.hint..self.total_frames {
            if !self.is_used(frame) {
                found = Some(frame);
                break;
            }
        }
        if found.is_none() {
            for frame in 0..self.hint {
                if !self.is_used(frame) {
                    found = Some(frame);
                    break;
                }
            }
        }

        let frame = found?;
        self.set_used(frame, true);
        self.free_frames -= 1;
        self.hint = if frame + 1 < self.total_frames {
            frame + 1
        } else {
            0
        };

        let physical = PhysFrame::containing_address(PhysAddr::new(frame as u64 * FRAME_SIZE));
        self.zero(physical);
        Some(physical)
    }

    /// Clears a frame before it is reused as a page table or heap page.
    fn zero(&self, frame: PhysFrame<Size4KiB>) {
        let virtual_addr = self.phys_offset + frame.start_address().as_u64();
        // SAFETY: the frame was just marked used and is only reachable through
        // the physical-memory mapping until a caller maps it somewhere else.
        unsafe {
            ptr::write_bytes(virtual_addr.as_mut_ptr::<u8>(), 0, FRAME_SIZE as usize);
        }
    }

    fn deallocate(&mut self, frame: PhysFrame<Size4KiB>) -> bool {
        if !self.ready {
            return false;
        }
        let index = (frame.start_address().as_u64() / FRAME_SIZE) as usize;
        if index >= self.total_frames || !self.is_used(index) {
            return false;
        }
        self.set_used(index, false);
        self.free_frames += 1;
        self.hint = self.hint.min(index);
        true
    }

    fn stats(&self) -> FrameStats {
        FrameStats {
            total: self.total_frames as u64,
            usable: self.usable_frames as u64,
            used: self.usable_frames.saturating_sub(self.free_frames) as u64,
            free: self.free_frames as u64,
        }
    }
}

// SAFETY: `allocate` only returns frames that are marked used, and marking is
// atomic with respect to the allocator's mutex. The bitmap storage itself is
// permanently reserved during `init`.
unsafe impl FrameAllocator<Size4KiB> for BitmapAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.allocate()
    }
}

// SAFETY: the caller promises the frame is no longer mapped or used.
impl FrameDeallocator<Size4KiB> for BitmapAllocator {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        self.deallocate(frame);
    }
}

static ALLOCATOR: Mutex<BitmapAllocator> = Mutex::new(BitmapAllocator::empty());

/// Builds the bitmap from the bootloader's physical memory map.
pub fn init(regions: &[MemoryRegion], phys_offset: VirtAddr) -> Result<(), &'static str> {
    ALLOCATOR.lock().init(regions, phys_offset)
}

/// Allocates one zeroed 4 KiB frame.
pub fn allocate_frame() -> Option<PhysFrame<Size4KiB>> {
    ALLOCATOR.lock().allocate()
}

/// Returns a frame to the allocator.
///
/// # Safety
///
/// The caller must pass a frame previously returned by [`allocate_frame`] or
/// [`map_page_alloc`](super::paging::map_page_alloc), and that frame must no
/// longer be mapped or referenced.
pub unsafe fn deallocate_frame(frame: PhysFrame<Size4KiB>) {
    ALLOCATOR.lock().deallocate(frame);
}

pub fn stats() -> FrameStats {
    ALLOCATOR.lock().stats()
}

/// Allocates and immediately returns one frame, checking the accounting.
pub fn self_test() -> Result<(), &'static str> {
    let before = stats().free;
    let frame = allocate_frame().ok_or("frame allocator self-test: no free frame")?;
    let after_allocate = stats().free;
    if after_allocate != before.saturating_sub(1) {
        // SAFETY: the frame was just allocated and is not mapped.
        unsafe { deallocate_frame(frame) };
        return Err("frame allocator self-test: free count did not decrease");
    }

    // SAFETY: the frame was just allocated and is not mapped.
    unsafe { deallocate_frame(frame) };
    if stats().free != before {
        return Err("frame allocator self-test: free count was not restored");
    }
    Ok(())
}

/// Used by the page-table code, which needs to pass a real `FrameAllocator`
/// into the `x86_64` mapper while holding the lock for the whole mapping.
pub(crate) fn lock() -> MutexGuard<'static, BitmapAllocator> {
    ALLOCATOR.lock()
}

const fn align_up(value: u64, align: u64) -> u64 {
    (value + align - 1) & !(align - 1)
}
