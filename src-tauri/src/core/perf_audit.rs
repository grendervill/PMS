//! Allocation / reallocation / SIMD / parallelism audit harness.
//!
//! This module is compiled only for tests. It installs a counting global
//! allocator, then measures the hot paths of the vault: import parsing, AEAD
//! sealing, base64, and the parallel headroom of the same work.
//!
//! Run it in release mode, with a single test thread, so no other test
//! pollutes the counters:
//!
//! ```sh
//! cargo test --release perf_audit -- --ignored --nocapture --test-threads=1
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use std::time::Instant;

use base64::Engine as _;
use sha2::{Digest, Sha256};

use super::crypto::{open_payload, seal_payload};
use super::import;
use super::models::CipherSuite;

struct CountingAllocator;

static ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static REALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static DEALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static ALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);
static REALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicIsize = AtomicIsize::new(0);
static PEAK_BYTES: AtomicIsize = AtomicIsize::new(0);
static REALLOC_BUCKETS: [AtomicUsize; 6] = [
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
];
/// Counting costs five atomic RMWs per allocation, which turns into cache-line
/// contention as soon as several threads allocate. Timings therefore run with
/// counting disabled; allocation counts are collected single-threaded.
static COUNTING: AtomicUsize = AtomicUsize::new(1);

const BUCKET_LIMITS: [usize; 5] = [32, 64, 256, 1024, 8192];
const BUCKET_LABELS: [&str; 6] = ["<=32 B", "<=64 B", "<=256 B", "<=1 KiB", "<=8 KiB", ">8 KiB"];

#[inline]
fn counting() -> bool {
    COUNTING.load(Ordering::Relaxed) == 1
}

#[inline]
fn record_realloc_size(new_size: usize) {
    let bucket = BUCKET_LIMITS
        .iter()
        .position(|limit| new_size <= *limit)
        .unwrap_or(BUCKET_LIMITS.len());
    REALLOC_BUCKETS[bucket].fetch_add(1, Ordering::Relaxed);
}

#[inline]
fn track_live(delta: isize) {
    let live = LIVE_BYTES.fetch_add(delta, Ordering::Relaxed) + delta;
    PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !counting() {
            return System.alloc(layout);
        }
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        track_live(layout.size() as isize);
        System.alloc(layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if !counting() {
            return System.alloc_zeroed(layout);
        }
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        track_live(layout.size() as isize);
        System.alloc_zeroed(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if !counting() {
            return System.dealloc(ptr, layout);
        }
        DEALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        track_live(-(layout.size() as isize));
        System.dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if !counting() {
            return System.realloc(ptr, layout, new_size);
        }
        REALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        record_realloc_size(new_size);
        REALLOC_BYTES.fetch_add(new_size, Ordering::Relaxed);
        track_live(new_size as isize - layout.size() as isize);
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[derive(Clone, Copy, Default)]
struct Stats {
    alloc_calls: usize,
    realloc_calls: usize,
    dealloc_calls: usize,
    alloc_bytes: usize,
    realloc_bytes: usize,
    peak_bytes: usize,
    realloc_buckets: [usize; 6],
}

impl Stats {
    fn snapshot() -> Self {
        Self {
            alloc_calls: ALLOC_CALLS.load(Ordering::Relaxed),
            realloc_calls: REALLOC_CALLS.load(Ordering::Relaxed),
            dealloc_calls: DEALLOC_CALLS.load(Ordering::Relaxed),
            alloc_bytes: ALLOC_BYTES.load(Ordering::Relaxed),
            realloc_bytes: REALLOC_BYTES.load(Ordering::Relaxed),
            peak_bytes: PEAK_BYTES.load(Ordering::Relaxed).max(0) as usize,
            realloc_buckets: std::array::from_fn(|index| {
                REALLOC_BUCKETS[index].load(Ordering::Relaxed)
            }),
        }
    }

    fn since(&self, base: &Stats) -> Self {
        Self {
            alloc_calls: self.alloc_calls.saturating_sub(base.alloc_calls),
            realloc_calls: self.realloc_calls.saturating_sub(base.realloc_calls),
            dealloc_calls: self.dealloc_calls.saturating_sub(base.dealloc_calls),
            alloc_bytes: self.alloc_bytes.saturating_sub(base.alloc_bytes),
            realloc_bytes: self.realloc_bytes.saturating_sub(base.realloc_bytes),
            peak_bytes: self.peak_bytes.saturating_sub(base.peak_bytes),
            realloc_buckets: std::array::from_fn(|index| {
                self.realloc_buckets[index].saturating_sub(base.realloc_buckets[index])
            }),
        }
    }
}

struct Measurement<T> {
    value: T,
    stats: Stats,
    best: f64,
}

/// Runs `f` `runs` times and keeps the fastest run. Allocation counting is
/// turned on or off for the whole measurement.
fn measure_with<T>(runs: usize, count: bool, mut f: impl FnMut() -> T) -> Measurement<T> {
    COUNTING.store(usize::from(count), Ordering::Relaxed);
    black_box(f());
    let mut stats = Stats::default();
    let mut best = f64::MAX;
    let mut value = None;
    for _ in 0..runs {
        // Peak is measured relative to the live bytes at the start of the run,
        // otherwise leftovers from previous runs hide the real high-water mark.
        PEAK_BYTES.store(LIVE_BYTES.load(Ordering::Relaxed), Ordering::Relaxed);
        let base = Stats::snapshot();
        let start = Instant::now();
        let out = f();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let snapshot = Stats::snapshot();
        stats = snapshot.since(&base);
        best = best.min(elapsed);
        value = Some(out);
    }
    COUNTING.store(1, Ordering::Relaxed);
    Measurement {
        value: value.expect("at least one run"),
        stats,
        best,
    }
}

fn measure<T>(runs: usize, f: impl FnMut() -> T) -> Measurement<T> {
    measure_with(runs, true, f)
}

fn measure_time<T>(runs: usize, f: impl FnMut() -> T) -> Measurement<T> {
    measure_with(runs, false, f)
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn mib_per_sec(bytes: usize, ms: f64) -> f64 {
    if ms <= 0.0 {
        return 0.0;
    }
    mib(bytes) / (ms / 1000.0)
}

fn bitwarden_json(items: usize) -> Vec<u8> {
    bitwarden_json_range(0, items)
}

fn bitwarden_json_range(from: usize, to: usize) -> Vec<u8> {
    let count = to.saturating_sub(from);
    let mut out = String::with_capacity(count * 1024 + 128);
    out.push_str("{\"encrypted\":false,\"folders\":[{\"id\":\"f1\",\"name\":\"Social\"}],\"items\":[");
    for index in from..to {
        if index > from {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"id\":\"i{index}\",\"type\":1,\"name\":\"Кингуру магазин {index}\",\"folderId\":\"f1\",\
             \"notes\":\"Заметка к записи номер {index}: адрес, договор и прочие детали средней длины.\",\
             \"fields\":[{{\"name\":\"PIN\",\"value\":\"12{index}\"}},\
             {{\"name\":\"Контрольный вопрос\",\"value\":\"Ответ {index}\"}},\
             {{\"name\":\"Договор\",\"value\":\"№{index}-2026\"}}],\
             \"login\":{{\"username\":\"alex{index}@example.com\",\"password\":\"s3cret-{index}-password\",\
             \"totp\":\"JBSWY3DPEHPK3PXP\",\
             \"uris\":[{{\"uri\":\"https://example{index}.com\"}},{{\"uri\":\"https://login.example{index}.com\"}}]}},\
             \"passwordHistory\":[\
             {{\"lastUsedDate\":\"2023-01-01T00:00:00.000Z\",\"password\":\"old-a-{index}\"}},\
             {{\"lastUsedDate\":\"2022-01-01T00:00:00.000Z\",\"password\":\"old-b-{index}\"}},\
             {{\"lastUsedDate\":\"2021-01-01T00:00:00.000Z\",\"password\":\"old-c-{index}\"}}]}}"
        ));
    }
    out.push_str("]}");
    out.into_bytes()
}

/// Parsing only: the bundle borrows from the input, no strings are copied.
fn parse_bitwarden(json: &[u8]) -> usize {
    let bundle = import::parse(json, None, None).expect("parses");
    bundle.items.len()
}

/// What the `import_parse` command does: parse and stream the bundle to JSON.
fn import_to_bytes(json: &[u8]) -> usize {
    let bundle = import::parse(json, None, None).expect("parses");
    let items = bundle.items.len();
    let mut payload = Vec::with_capacity(json.len().clamp(8 * 1024, 32 * 1024 * 1024));
    serde_json::to_writer(&mut payload, &bundle).expect("serializes");
    black_box(payload.len());
    items
}

fn parse_chunks(chunks: &[Vec<u8>]) -> usize {
    chunks.iter().map(|chunk| parse_bitwarden(chunk)).sum()
}

fn import_chunks(chunks: &[Vec<u8>]) -> usize {
    chunks.iter().map(|chunk| import_to_bytes(chunk)).sum()
}

fn import_chunks_parallel(chunks: &[Vec<u8>], threads: usize) -> usize {
    let per_thread = chunks.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let handles = chunks
            .chunks(per_thread)
            .map(|group| scope.spawn(move || import_chunks(group)))
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("worker"))
            .sum()
    })
}

fn seal_and_open(cipher: CipherSuite, plaintext: &[u8]) -> usize {
    let key = [7u8; 32];
    let envelope = seal_payload(&key, cipher, b"aad", plaintext).expect("seal");
    let opened = open_payload(&key, cipher, b"aad", &envelope).expect("open");
    opened.len()
}

fn seal_only(cipher: CipherSuite, plaintext: &[u8]) -> usize {
    let key = [7u8; 32];
    seal_payload(&key, cipher, b"aad", plaintext)
        .expect("seal")
        .len()
}

fn feature(name: &str, on: bool) -> String {
    format!("{name}={}", if on { "yes" } else { "no" })
}

fn cpu_features() -> String {
    #[cfg(target_arch = "aarch64")]
    {
        use std::arch::is_aarch64_feature_detected as detect;
        [
            feature("neon", detect!("neon")),
            feature("aes", detect!("aes")),
            feature("sha2", detect!("sha2")),
            feature("sha3", detect!("sha3")),
            feature("pmull", detect!("pmull")),
            feature("crc", detect!("crc")),
        ]
        .join(" ")
    }
    #[cfg(target_arch = "x86_64")]
    {
        [
            feature("sse2", std::is_x86_feature_detected!("sse2")),
            feature("avx2", std::is_x86_feature_detected!("avx2")),
            feature("aes", std::is_x86_feature_detected!("aes")),
            feature("sha", std::is_x86_feature_detected!("sha")),
        ]
        .join(" ")
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        "unknown architecture".to_string()
    }
}

fn row(cells: &[String]) -> String {
    let mut line = String::from("|");
    for cell in cells {
        line.push(' ');
        line.push_str(cell);
        line.push_str(" |");
    }
    line
}

fn header_row(cells: &[&str]) -> String {
    let mut line = row(&cells.iter().map(|cell| cell.to_string()).collect::<Vec<_>>());
    line.push('\n');
    line.push('|');
    for _ in cells {
        line.push_str("---|");
    }
    line
}

#[test]
#[ignore = "audit harness: cargo test --release perf_audit -- --ignored --nocapture --test-threads=1"]
fn perf_audit() {
    let threads = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1);

    println!("\n### Environment");
    println!("profile: {}", if cfg!(debug_assertions) { "debug (timings are not representative)" } else { "release" });
    println!("arch: {} | threads: {threads} | backend: {}", std::env::consts::ARCH, cpu_features());
    println!("allocator: counting wrapper over System (counts include the harness itself)");

    // ---------- import parsing ----------
    println!("\n### Bitwarden JSON import");
    println!("{}", header_row(&["items", "json KiB", "alloc", "realloc", "dealloc", "allocated KiB", "reallocated KiB", "peak KiB", "ms", "items/s"]));

    let mut datasets = Vec::new();
    for items in [200usize, 1_000, 5_000, 20_000] {
        let json = bitwarden_json(items);
        datasets.push((items, json));
    }

    let mut import_rows = Vec::new();
    for (items, json) in &datasets {
        let runtime = measure(3, || import_to_bytes(json));
        assert_eq!(runtime.value, *items);
        let s = runtime.stats;
        println!("{}", row(&[
            items.to_string(),
            format!("{:.1}", json.len() as f64 / 1024.0),
            s.alloc_calls.to_string(),
            s.realloc_calls.to_string(),
            s.dealloc_calls.to_string(),
            format!("{:.0}", s.alloc_bytes as f64 / 1024.0),
            format!("{:.0}", s.realloc_bytes as f64 / 1024.0),
            format!("{:.0}", s.peak_bytes as f64 / 1024.0),
            format!("{:.2}", runtime.best),
            format!("{:.0}", *items as f64 / (runtime.best / 1000.0)),
        ]));
        import_rows.push((*items, runtime.best, s));
    }

    // serde_json tree only, to attribute the cost between serde and our mapping
    println!("\n### Where the import cost comes from (5 000 items)");
    println!("{}", header_row(&["step", "alloc", "realloc", "allocated KiB", "peak KiB", "ms"]));
    let (_, json) = datasets[2].clone();
    let tree = measure(3, || {
        let value: serde_json::Value = serde_json::from_slice(&json).expect("json");
        value["items"].as_array().map(Vec::len).unwrap_or(0)
    });
    let parse = measure(3, || parse_bitwarden(&json));
    let command = measure(3, || import_to_bytes(&json));
    for (label, runtime) in [
        ("serde_json::Value tree (was)", &tree),
        ("parse only (now)", &parse),
        ("parse + JSON out (now)", &command),
    ] {
        let s = runtime.stats;
        println!("{}", row(&[
            label.to_string(),
            s.alloc_calls.to_string(),
            s.realloc_calls.to_string(),
            format!("{:.0}", s.alloc_bytes as f64 / 1024.0),
            format!("{:.0}", s.peak_bytes as f64 / 1024.0),
            format!("{:.2}", runtime.best),
        ]));
    }

    // ---------- crypto ----------
    println!("\n### AEAD seal + open (16 MiB payload)");
    println!("{}", header_row(&["cipher", "alloc", "realloc", "allocated KiB", "peak KiB", "ms", "MiB/s"]));
    let payload = vec![0xABu8; 16 * 1024 * 1024];
    for (label, cipher) in [
        ("XChaCha20-Poly1305", CipherSuite::XChaCha20Poly1305),
        ("AES-256-GCM", CipherSuite::Aes256Gcm),
    ] {
        let runtime = measure(3, || seal_and_open(cipher, &payload));
        assert_eq!(runtime.value, payload.len());
        let s = runtime.stats;
        println!("{}", row(&[
            label.to_string(),
            s.alloc_calls.to_string(),
            s.realloc_calls.to_string(),
            format!("{:.0}", s.alloc_bytes as f64 / 1024.0),
            format!("{:.0}", s.peak_bytes as f64 / 1024.0),
            format!("{:.2}", runtime.best),
            format!("{:.0}", mib_per_sec(payload.len() * 2, runtime.best)),
        ]));
    }

    // ---------- hashing / base64 ----------
    println!("\n### Scalar-helper throughput (16 MiB)");
    println!("{}", header_row(&["path", "alloc", "realloc", "peak KiB", "ms", "MiB/s"]));
    let digest = measure(3, || {
        let mut hasher = Sha256::new();
        hasher.update(&payload);
        hasher.finalize().len()
    });
    assert_eq!(digest.value, 32);
    let b64 = measure(3, || {
        let engine = base64::engine::general_purpose::STANDARD;
        let encoded = engine.encode(&payload);
        let bytes = engine.decode(&encoded).expect("decode");
        bytes.len()
    });
    assert_eq!(b64.value, payload.len());
    for (label, runtime, bytes) in [
        ("SHA-256 digest", &digest, payload.len()),
        ("base64 encode + decode", &b64, payload.len() * 2),
    ] {
        let s = runtime.stats;
        println!("{}", row(&[
            label.to_string(),
            s.alloc_calls.to_string(),
            s.realloc_calls.to_string(),
            format!("{:.0}", s.peak_bytes as f64 / 1024.0),
            format!("{:.2}", runtime.best),
            format!("{:.0}", mib_per_sec(bytes, runtime.best)),
        ]));
    }

    // ---------- parallelism ----------
    println!("\n### Parallel headroom (the current code path is single-threaded)");
    println!("{}", header_row(&["scenario", "threads", "ms", "speedup", "rate"]));
    for items in [1_000usize, 5_000] {
        let chunk_sets = [1usize, 2, 4].map(|count| {
            let per_chunk = items.div_ceil(count).max(1);
            (0..count)
                .map(|index| {
                    let start = index * per_chunk;
                    let end = ((index + 1) * per_chunk).min(items);
                    if start >= end {
                        Vec::new()
                    } else {
                        bitwarden_json_range(start, end)
                    }
                })
                .filter(|chunk| !chunk.is_empty())
                .collect::<Vec<_>>()
        });
        let mut baseline_ms = 0.0;
        for (threads, chunks) in [1usize, 2, 4].into_iter().zip(chunk_sets.iter()) {
            let runtime = if threads == 1 {
                measure_time(3, || import_chunks(chunks))
            } else {
                measure_time(3, || import_chunks_parallel(chunks, threads))
            };
            assert_eq!(runtime.value, items);
            if threads == 1 {
                baseline_ms = runtime.best;
            }
            println!("{}", row(&[
                format!("import {items} items"),
                threads.to_string(),
                format!("{:.2}", runtime.best),
                format!("{:.2}x", baseline_ms / runtime.best),
                format!("{:.1}", items as f64 / runtime.best),
            ]));
        }
    }

    let crypto_buffers: [Vec<u8>; 4] = std::array::from_fn(|_| vec![0x11u8; 8 * 1024 * 1024]);
    let sequential = measure_time(3, || {
        crypto_buffers
            .iter()
            .map(|buffer| seal_only(CipherSuite::XChaCha20Poly1305, buffer))
            .sum::<usize>()
    });
    let parallel = measure_time(3, || {
        std::thread::scope(|scope| {
            let handles = crypto_buffers
                .iter()
                .map(|buffer| scope.spawn(move || seal_only(CipherSuite::XChaCha20Poly1305, buffer)))
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("worker"))
                .sum::<usize>()
        })
    });
    let total = 4 * 8 * 1024 * 1024;
    let seq_rate = mib_per_sec(total, sequential.best);
    println!("{}", row(&[
        "seal 4 x 8 MiB".to_string(),
        "1".to_string(),
        format!("{:.2}", sequential.best),
        "1.00x".to_string(),
        format!("{:.0} MiB/s", seq_rate),
    ]));
    println!("{}", row(&[
        "seal 4 x 8 MiB".to_string(),
        "4".to_string(),
        format!("{:.2}", parallel.best),
        format!("{:.2}x", sequential.best / parallel.best),
        format!("{:.0} MiB/s", mib_per_sec(total, parallel.best)),
    ]));

    // ---------- reallocation size classes ----------
    println!("\n### Reallocation size classes (5 000-item import)");
    println!("{}", header_row(&["class", "reallocs", "share", "note"]));
    let (_, _, stats) = import_rows[2];
    let total: usize = stats.realloc_buckets.iter().sum();
    for (index, label) in BUCKET_LABELS.iter().enumerate() {
        let count = stats.realloc_buckets[index];
        println!("{}", row(&[
            label.to_string(),
            count.to_string(),
            format!("{:.1}%", if total == 0 { 0.0 } else { count as f64 / total as f64 * 100.0 }),
            if index <= 2 { "string/Vec growth".to_string() } else { "buffer growth".to_string() },
        ]));
    }

    // ---------- allocation shape of the import ----------
    println!("\n### Allocation shape of the import");
    for (items, ms, stats) in &import_rows {
        println!(
            "{items} items: {:.1} alloc/item, {:.2} realloc/item, {:.0} B allocated/item, {:.2} ms",
            stats.alloc_calls as f64 / *items as f64,
            stats.realloc_calls as f64 / *items as f64,
            stats.alloc_bytes as f64 / *items as f64,
            ms,
        );
    }
}

#[test]
#[ignore]
fn realloc_probe() {
    let base = bitwarden_json(1000);
    let root: serde_json::Value = serde_json::from_slice(&base).unwrap();
    let mut variants = Vec::new();
    variants.push(("full", root.clone()));
    let drop_key = |root: &mut serde_json::Value, key: &str| {
        for item in root["items"].as_array_mut().unwrap() {
            item.as_object_mut().unwrap().remove(key);
            if key == "totp" {
                item["login"].as_object_mut().unwrap().remove("totp");
            }
        }
    };
    let mut no_totp = root.clone();
    drop_key(&mut no_totp, "totp");
    variants.push(("no totp", no_totp));
    let mut no_fields = root.clone();
    drop_key(&mut no_fields, "fields");
    variants.push(("no fields", no_fields));
    let mut no_history = root.clone();
    drop_key(&mut no_history, "passwordHistory");
    variants.push(("no history", no_history));
    for (label, value) in variants {
        let json = serde_json::to_vec(&value).unwrap();
        let runtime = measure(3, || import_to_bytes(&json));
        println!(
            "{label:12} items={} alloc={} realloc={} peak={}KiB",
            runtime.value,
            runtime.stats.alloc_calls,
            runtime.stats.realloc_calls,
            runtime.stats.peak_bytes / 1024
        );
    }
}
