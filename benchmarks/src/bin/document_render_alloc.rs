use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use markdown_it::MarkdownIt;
use markdown_it_benchmarks::corpus;

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

fn parser() -> MarkdownIt {
    let mut md = MarkdownIt::empty();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::html::add(&mut md);
    md
}

fn print_row(corpus: &str, phase: &str, path: &str, stats: AllocationStats, html: &str) {
    println!(
        "| {corpus} | {phase} | {path} | {} | {} | {} |",
        stats.allocations,
        stats.allocated_bytes,
        html.len(),
    );
}

fn main() {
    let md = parser();
    println!("| Corpus | Phase | Path | Allocations | Requested bytes | HTML bytes |");
    println!("| --- | --- | --- | ---: | ---: | ---: |");

    for corpus in corpus::standard() {
        let source = corpus.source();
        let document = md.parse_document(source);
        let legacy = md.parse(source);

        // Warm lazy parser/renderer state before enabling the counter.
        let expected = legacy.render();
        assert_eq!(md.render_document(&document).unwrap(), expected);

        let (direct, direct_stats) = measure(|| md.render_document(black_box(&document)).unwrap());
        assert_eq!(direct, expected);
        print_row(corpus.name, "render", "arena-direct", direct_stats, &direct);

        let bridge_document = md.parse_document(source);
        let (bridge, bridge_stats) = measure(|| bridge_document.into_legacy().render());
        assert_eq!(bridge, expected);
        print_row(corpus.name, "render", "arena-bridge", bridge_stats, &bridge);

        let (legacy_html, legacy_stats) = measure(|| legacy.render());
        assert_eq!(legacy_html, expected);
        print_row(
            corpus.name,
            "render",
            "legacy-tree",
            legacy_stats,
            &legacy_html,
        );

        let (direct, direct_stats) = measure(|| {
            let document = md.parse_document(black_box(source));
            md.render_document(&document).unwrap()
        });
        assert_eq!(direct, expected);
        print_row(
            corpus.name,
            "end-to-end",
            "arena-direct",
            direct_stats,
            &direct,
        );

        let (bridge, bridge_stats) = measure(|| {
            md.parse_document(black_box(source))
                .into_legacy()
                .render()
        });
        assert_eq!(bridge, expected);
        print_row(
            corpus.name,
            "end-to-end",
            "arena-bridge",
            bridge_stats,
            &bridge,
        );

        let (legacy_html, legacy_stats) = measure(|| md.parse(black_box(source)).render());
        assert_eq!(legacy_html, expected);
        print_row(
            corpus.name,
            "end-to-end",
            "legacy-tree",
            legacy_stats,
            &legacy_html,
        );
    }
}
