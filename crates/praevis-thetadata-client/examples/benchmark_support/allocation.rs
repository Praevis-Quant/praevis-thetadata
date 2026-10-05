//! Requested Rust heap accounting in a separate, thread-local probe.
//! Excludes ZSTD's C allocator, allocator metadata, OS memory and other threads.
use serde::{Deserialize, Serialize};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Counts {
    pub allocation_calls: u64,
    pub requested_bytes: u64,
    pub live_bytes: usize,
    pub peak_live_bytes: usize,
    pub invalid_scope: bool,
}
thread_local! { static COUNTS: Cell<Option<Counts>> = const { Cell::new(None) }; }
pub struct Allocator;

fn change(old: usize, new: usize, allocation: bool) {
    let _ = COUNTS.try_with(|cell| {
        if let Some(mut counts) = cell.get() {
            // Allocator callbacks must never unwind. Validate scope outside them.
            counts.invalid_scope |= old > counts.live_bytes;
            counts.live_bytes = counts.live_bytes.saturating_sub(old).saturating_add(new);
            counts.peak_live_bytes = counts.peak_live_bytes.max(counts.live_bytes);
            if allocation {
                counts.allocation_calls = counts.allocation_calls.saturating_add(1);
                counts.requested_bytes = counts.requested_bytes.saturating_add(new as u64);
            }
            cell.set(Some(counts));
        }
    });
}

// SAFETY: lifetime/alignment obligations are delegated unchanged to System.
// Accounting is allocation-free, thread-local and never dereferences pointers.
// Probes must only free/reallocate objects allocated inside their own scope.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            change(0, layout.size(), true);
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            change(0, layout.size(), true);
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        change(layout.size(), 0, false);
        unsafe {
            System.dealloc(pointer, layout);
        }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new: usize) -> *mut u8 {
        let result = unsafe { System.realloc(pointer, layout, new) };
        if !result.is_null() {
            change(layout.size(), new, true);
        }
        result
    }
}

pub fn probe<T>(operation: impl FnOnce() -> T) -> (T, Counts) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            COUNTS.with(|c| c.set(None));
        }
    }
    COUNTS.with(|c| {
        assert!(c.get().is_none());
        c.set(Some(Counts::default()));
    });
    let reset = Reset;
    let output = operation();
    let counts = COUNTS.with(|c| c.get().unwrap());
    drop(reset);
    assert!(
        !counts.invalid_scope,
        "allocation probe escaped its owned scope"
    );
    (output, counts)
}
