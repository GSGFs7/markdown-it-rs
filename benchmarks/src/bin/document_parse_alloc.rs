use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use markdown_it_benchmarks::{corpus, parser};

struct CountingAllocator;

static ENABLED: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let result = unsafe { System.alloc(layout) };
        record_allocation(layout.size(), result);
        result
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let result = unsafe { System.alloc_zeroed(layout) };
        record_allocation(layout.size(), result);
        result
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(ptr, layout, new_size) };
        record_allocation(new_size, result);
        result
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn record_allocation(bytes: usize, pointer: *mut u8) {
    if !pointer.is_null() && ENABLED.load(Ordering::Relaxed) {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy)]
struct AllocationStats {
    allocations: usize,
    allocated_bytes: usize,
}

fn measure<T>(operation: impl FnOnce() -> T) -> (T, AllocationStats) {
    ENABLED.store(false, Ordering::SeqCst);
    ALLOCATIONS.store(0, Ordering::SeqCst);
    ALLOCATED_BYTES.store(0, Ordering::SeqCst);
    ENABLED.store(true, Ordering::SeqCst);
    let result = operation();
    ENABLED.store(false, Ordering::SeqCst);

    let stats = AllocationStats {
        allocations: ALLOCATIONS.load(Ordering::SeqCst),
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::SeqCst),
    };
    (result, stats)
}

fn print_row(configuration: &str, corpus: &str, path: &str, stats: AllocationStats) {
    println!(
        "| {configuration} | {corpus} | {path} | {} | {} |",
        stats.allocations, stats.allocated_bytes,
    );
}

fn main() {
    println!("| Configuration | Corpus | Path | Allocations | Requested bytes |");
    println!("| --- | --- | --- | ---: | ---: |");

    for configuration in parser::document_parse_configurations() {
        let configuration_name = configuration.name;
        let md = configuration.parser;
        for corpus in corpus::parser_emphasis_checkpoint() {
            let source = corpus.source();

            // Warm lazy parser state before enabling the counter.
            let expected = md.parse(source).render();
            assert_eq!(
                md.render_document(&md.parse_document_direct(source).unwrap())
                    .unwrap(),
                expected
            );

            let (legacy, legacy_stats) = measure(|| md.parse(black_box(source)));
            print_row(configuration_name, corpus.name, "legacy-tree", legacy_stats);

            let (bridged, bridge_stats) = measure(|| md.parse_document(black_box(source)));
            let bridge_nodes = bridged.len();
            assert_eq!(md.render_document(&bridged).unwrap(), expected);
            print_row(
                configuration_name,
                corpus.name,
                "legacy-arena-bridge",
                bridge_stats,
            );

            let (direct, direct_stats) =
                measure(|| md.parse_document_direct(black_box(source)).unwrap());
            let direct_nodes = direct.len();
            assert_eq!(md.render_document(&direct).unwrap(), expected);
            assert_eq!(direct_nodes, bridge_nodes);
            print_row(
                configuration_name,
                corpus.name,
                "arena-direct",
                direct_stats,
            );

            black_box(legacy);
        }
    }
}
