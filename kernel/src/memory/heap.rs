//! Dynamically mapped kernel heap.
//!
//! The heap is a contiguous virtual range at [`HEAP_BASE`]. Only the first
//! [`INITIAL_PAGES`] pages are mapped at boot; when `linked_list_allocator`
//! cannot satisfy an allocation, [`grow`] asks the page-table layer for more
//! frames, maps them directly after the existing heap, and extends the
//! allocator. Physical memory is therefore consumed only when the kernel
//! actually needs it.
//!
//! The global allocator holds the heap mutex while it searches for a hole. If
//! the search fails, the mutex is released before mapping new pages: mapping
//! takes the page-table and frame-allocator locks, and no code path may take
//! them in the opposite order.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::NonNull;

use linked_list_allocator::Heap;
use spin::Mutex;
use x86_64::VirtAddr;
use x86_64::structures::paging::{Page, PageTableFlags};

use super::paging;

/// Base of the kernel heap. PML4 slot 128, well away from the physical-memory
/// mapping (slot 32), the paging self-test (slot 96) and the bootloader's
/// dynamic mappings (low slots).
pub const HEAP_BASE: u64 = 0x0000_4000_0000_0000;

/// 2 MiB are mapped before the first allocation.
const INITIAL_PAGES: u64 = 512;

/// Smallest growth step. Larger requests map exactly what they need.
const GROW_MIN: usize = 1024 * 1024;

const PAGE_SIZE: u64 = paging::PAGE_SIZE;

const HEAP_FLAGS: PageTableFlags = PageTableFlags::PRESENT
    .union(PageTableFlags::WRITABLE)
    .union(PageTableFlags::NO_EXECUTE);

struct HeapState {
    heap: Heap,
    /// Next virtual page that has not been mapped for the heap yet.
    next_page: u64,
    /// Bytes mapped for the heap, including bytes not yet handed to `heap`.
    mapped_bytes: u64,
    initialized: bool,
}

impl HeapState {
    const fn empty() -> Self {
        Self {
            heap: Heap::empty(),
            next_page: HEAP_BASE,
            mapped_bytes: 0,
            initialized: false,
        }
    }
}

struct KernelHeap {
    state: Mutex<HeapState>,
}

#[global_allocator]
static ALLOCATOR: KernelHeap = KernelHeap {
    state: Mutex::new(HeapState::empty()),
};

pub struct HeapStats {
    pub used: u64,
    pub free: u64,
    /// Usable bytes inside the linked-list heap.
    pub size: u64,
    /// Bytes of virtual address space mapped for the heap.
    pub mapped: u64,
    pub base: u64,
}

/// Maps the initial heap pages and initialises the linked-list allocator.
pub fn init() -> Result<(), &'static str> {
    let mut state = ALLOCATOR.state.lock();
    if state.initialized {
        return Ok(());
    }

    let bytes = INITIAL_PAGES * PAGE_SIZE;
    for page_index in 0..INITIAL_PAGES {
        let address = HEAP_BASE + page_index * PAGE_SIZE;
        if paging::translate_addr(VirtAddr::new(address)).is_some() {
            return Err("the heap virtual range is already mapped");
        }
        paging::map_page_alloc(Page::containing_address(VirtAddr::new(address)), HEAP_FLAGS)
            .map_err(|_| "cannot map an initial heap page")?;
    }

    // SAFETY: the whole range was just mapped writable and is not used by any
    // other subsystem. `Heap::init` is called exactly once.
    unsafe {
        state
            .heap
            .init(VirtAddr::new(HEAP_BASE).as_mut_ptr::<u8>(), bytes as usize);
    }
    state.next_page = HEAP_BASE + bytes;
    state.mapped_bytes = bytes;
    state.initialized = true;
    Ok(())
}

/// Maps at least `min_bytes` more bytes and extends the heap.
///
/// Returns `false` if the frame allocator or page-table layer cannot satisfy
/// the request. Any partially created mapping is rolled back so a later call
/// can retry from the same virtual page.
fn grow(min_bytes: usize) -> bool {
    let mut state = ALLOCATOR.state.lock();
    if !state.initialized {
        return false;
    }

    let requested = min_bytes.max(GROW_MIN) as u64;
    let bytes = align_up(requested, PAGE_SIZE);
    let pages = bytes / PAGE_SIZE;
    let start = state.next_page;
    let mut mapped_pages = 0u64;

    for page_index in 0..pages {
        let address = start + page_index * PAGE_SIZE;
        let page = Page::containing_address(VirtAddr::new(address));
        match paging::map_page_alloc(page, HEAP_FLAGS) {
            Ok(_) => mapped_pages += 1,
            Err(_) => {
                // Roll back mappings from this attempt. The data frames are
                // unmapped and returned to the frame allocator.
                for rollback in 0..mapped_pages {
                    let address = start + rollback * PAGE_SIZE;
                    let page = Page::containing_address(VirtAddr::new(address));
                    if let Ok(frame) = paging::unmap_page(page) {
                        // SAFETY: the frame was just unmapped and came from
                        // `map_page_alloc` in this rollback loop.
                        unsafe { super::frame_allocator::deallocate_frame(frame) };
                    }
                }
                return false;
            }
        }
    }

    // SAFETY: the new pages directly follow the existing heap range and were
    // just mapped writable. `extend` connects them to the linked list.
    unsafe { state.heap.extend(bytes as usize) };
    state.next_page = start + bytes;
    state.mapped_bytes += bytes;
    true
}

unsafe impl GlobalAlloc for KernelHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 0 {
            return layout.align() as *mut u8;
        }

        // The guard is dropped at the end of this statement, before `grow`
        // takes the page-table and frame-allocator locks.
        let first_try = self.state.lock().heap.allocate_first_fit(layout);
        if let Ok(pointer) = first_try {
            return pointer.as_ptr();
        }

        if !grow(layout.size()) {
            return core::ptr::null_mut();
        }

        match self.state.lock().heap.allocate_first_fit(layout) {
            Ok(pointer) => pointer.as_ptr(),
            Err(_) => core::ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if layout.size() == 0 {
            return;
        }
        // SAFETY: the caller passes a pointer previously returned by `alloc`
        // with the same layout.
        unsafe {
            self.state
                .lock()
                .heap
                .deallocate(NonNull::new_unchecked(pointer), layout);
        }
    }
}

pub fn stats() -> HeapStats {
    let state = ALLOCATOR.state.lock();
    HeapStats {
        used: state.heap.used() as u64,
        free: state.heap.free() as u64,
        size: state.heap.size() as u64,
        mapped: state.mapped_bytes,
        base: HEAP_BASE,
    }
}

const fn align_up(value: u64, align: u64) -> u64 {
    (value + align - 1) & !(align - 1)
}
