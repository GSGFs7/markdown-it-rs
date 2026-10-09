use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use markdown_it_benchmarks::{corpus, parser};

/// Tracks live requested bytes for the entire process, including allocations
/// made before a measurement. This makes freeing pre-existing allocations safe.
/// Measurements are serial; unrelated background allocations must be avoided.
struct CountingAllocator {
    enabled: AtomicBool,
    allocations: AtomicUsize,
    reallocations: AtomicUsize,
    requested_bytes: AtomicUsize,
    live_bytes: AtomicUsize,
    peak_bytes: AtomicUsize,
}

impl CountingAllocator {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocations: AtomicUsize::new(0),
            reallocations: AtomicUsize::new(0),
            requested_bytes: AtomicUsize::new(0),
            live_bytes: AtomicUsize::new(0),
            peak_bytes: AtomicUsize::new(0),
        }
    }

    fn record_allocation(&self, bytes: usize, pointer: *mut u8) {
        if pointer.is_null() {
            return;
        }
        let live = self.live_bytes.fetch_add(bytes, Ordering::Relaxed) + bytes;
        if self.enabled.load(Ordering::Relaxed) {
            self.allocations.fetch_add(1, Ordering::Relaxed);
            self.requested_bytes.fetch_add(bytes, Ordering::Relaxed);
            self.peak_bytes.fetch_max(live, Ordering::Relaxed);
        }
    }

    fn record_reallocation(&self, old_size: usize, new_size: usize, pointer: *mut u8) {
        // On failure the old allocation remains live.
        if pointer.is_null() {
            return;
        }
        let live = if new_size >= old_size {
            self.live_bytes
                .fetch_add(new_size - old_size, Ordering::Relaxed)
                + (new_size - old_size)
        } else {
            self.live_bytes
                .fetch_sub(old_size - new_size, Ordering::Relaxed)
                - (old_size - new_size)
        };
        if self.enabled.load(Ordering::Relaxed) {
            self.reallocations.fetch_add(1, Ordering::Relaxed);
            // Cumulative requests count the full new size, not only growth.
            self.requested_bytes.fetch_add(new_size, Ordering::Relaxed);
            self.peak_bytes.fetch_max(live, Ordering::Relaxed);
        }
    }

    fn measure<T>(&self, operation: impl FnOnce() -> T) -> (T, AllocationStats) {
        assert!(!self.enabled.load(Ordering::Relaxed), "nested measurement");
        let baseline = self.live_bytes.load(Ordering::Relaxed);
        self.allocations.store(0, Ordering::Relaxed);
        self.reallocations.store(0, Ordering::Relaxed);
        self.requested_bytes.store(0, Ordering::Relaxed);
        self.peak_bytes.store(baseline, Ordering::Relaxed);
        self.enabled.store(true, Ordering::Relaxed);
        let guard = MeasurementGuard(self);
        let result = operation();
        drop(guard);

        let stats = AllocationStats {
            allocations: self.allocations.load(Ordering::Relaxed),
            reallocations: self.reallocations.load(Ordering::Relaxed),
            requested_bytes: self.requested_bytes.load(Ordering::Relaxed),
            peak_live_bytes: self.peak_bytes.load(Ordering::Relaxed) - baseline,
            retained_bytes: self.live_bytes.load(Ordering::Relaxed) as i128 - baseline as i128,
        };
        (result, stats)
    }
}

struct MeasurementGuard<'a>(&'a CountingAllocator);

impl Drop for MeasurementGuard<'_> {
    fn drop(&mut self) {
        self.0.enabled.store(false, Ordering::Relaxed);
    }
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let result = unsafe { System.alloc(layout) };
        self.record_allocation(layout.size(), result);
        result
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let result = unsafe { System.alloc_zeroed(layout) };
        self.record_allocation(layout.size(), result);
        result
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        self.live_bytes.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(ptr, layout, new_size) };
        self.record_reallocation(layout.size(), new_size, result);
        result
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AllocationStats {
    allocations: usize,
    reallocations: usize,
    requested_bytes: usize,
    peak_live_bytes: usize,
    // Signed because a measurement may free memory allocated before it began.
    retained_bytes: i128,
}

fn print_row(configuration: &str, suite: &str, corpus: &str, stats: AllocationStats) {
    println!(
        "| {configuration} | {suite} | {corpus} | arena-direct | {} | {} | {} | {} | {} |",
        stats.allocations,
        stats.reallocations,
        stats.requested_bytes,
        stats.peak_live_bytes,
        stats.retained_bytes,
    );
}

fn main() {
    println!("| Configuration | Suite | Corpus | Path | Alloc | Realloc | Requested bytes | Peak live delta | Retained delta |");
    println!("| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: |");

    for configuration in parser::document_parse_configurations() {
        let configuration_name = configuration.name;
        let md = configuration.parser;
        for (suite, corpora) in [
            ("standard", corpus::standard()),
            ("emphasis", corpus::parser_emphasis_checkpoint()),
        ] {
            for corpus in corpora {
                let source = corpus.source();

                // Warm lazy parser and renderer state before enabling the counter.
                // Input and parser storage are part of the baseline, not the delta.
                let expected = md.render(source);
                let (document, stats) = ALLOCATOR.measure(|| md.parse_document(black_box(source)));
                assert_eq!(md.render_document(&document), expected);
                assert!(
                    stats.retained_bytes >= 0,
                    "unexpected warmed-state release: {stats:?}"
                );
                assert!(stats.peak_live_bytes as i128 >= stats.retained_bytes);
                print_row(configuration_name, suite, corpus.name, stats);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_live_memory_across_measurements_and_reallocation() {
        // A local allocator keeps test harness allocations outside these counts.
        let allocator = CountingAllocator::new();
        let layout = Layout::from_size_align(32, 8).unwrap();
        let pointer = unsafe { allocator.alloc(layout) };
        assert!(!pointer.is_null());
        let (pointer, stats) = allocator.measure(|| {
            let pointer = unsafe { allocator.realloc(pointer, layout, 96) };
            assert!(!pointer.is_null());
            let grown = Layout::from_size_align(96, 8).unwrap();
            let pointer = unsafe { allocator.realloc(pointer, grown, 16) };
            assert!(!pointer.is_null());
            let temporary = unsafe { allocator.alloc_zeroed(layout) };
            assert!(!temporary.is_null());
            unsafe { allocator.dealloc(temporary, layout) };
            pointer
        });
        assert_eq!(
            stats,
            AllocationStats {
                allocations: 1,
                reallocations: 2,
                requested_bytes: 96 + 16 + 32,
                peak_live_bytes: 64,
                retained_bytes: -16,
            }
        );
        let (_, stats) = allocator.measure(|| unsafe {
            allocator.dealloc(pointer, Layout::from_size_align(16, 8).unwrap());
        });
        assert_eq!(stats.peak_live_bytes, 0);
        assert_eq!(stats.retained_bytes, -16);
        assert_eq!(allocator.live_bytes.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn failed_requests_leave_old_allocation_live() {
        let allocator = CountingAllocator::new();
        // Exercise failure accounting without depending on the OS denying memory.
        allocator.live_bytes.store(32, Ordering::Relaxed);
        let (_, stats) = allocator.measure(|| {
            allocator.record_allocation(64, std::ptr::null_mut());
            allocator.record_reallocation(32, 128, std::ptr::null_mut());
        });
        assert_eq!(
            stats,
            AllocationStats {
                allocations: 0,
                reallocations: 0,
                requested_bytes: 0,
                peak_live_bytes: 0,
                retained_bytes: 0,
            }
        );
        assert_eq!(allocator.live_bytes.load(Ordering::Relaxed), 32);
    }

    #[test]
    fn panic_disables_measurement() {
        let allocator = CountingAllocator::new();
        let result = std::panic::catch_unwind(|| allocator.measure(|| panic!("operation failed")));
        assert!(result.is_err());
        assert!(!allocator.enabled.load(Ordering::Relaxed));
        let (_, stats) = allocator.measure(|| ());
        assert_eq!(stats.requested_bytes, 0);
    }
}
