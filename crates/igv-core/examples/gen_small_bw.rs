//! Generate `tests/data/small.bw` — a minimal bigWig fixture for Phase 2 tests.
//!
//! Fixture spec (DO NOT change — Phase 2 tests assert these values):
//!   chr1  length 1000  — linear ramp: each base i has value i (0.0 .. 999.0)
//!   chr2  length 500   — square wave: value 10.0 at [100,200) and [300,400),
//!                        undefined (no data) elsewhere
//!
//! Run with:
//!   cargo run -p igv-core --example gen_small_bw

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bigtools::beddata::BedParserStreamingIterator;
use bigtools::{BigWigRead, BigWigWrite, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Locate output path relative to this file's crate root.
    let out_path = {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("tests/data/small.bw");
        p
    };

    // Make sure the parent directory exists.
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    write_fixture(&out_path)?;
    sanity_check(&out_path)?;

    println!("Done. Fixture written to: {}", out_path.display());
    Ok(())
}

/// Write the fixture bigWig file.
fn write_fixture(out_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // ------------------------------------------------------------------
    // Build the iterator of (chrom_name, Value) pairs.
    // Data MUST be grouped by chromosome (all chr1 values before chr2).
    // Within each chromosome values must be sorted by start and non-overlapping.
    // ------------------------------------------------------------------

    // chr1: one Value per base, value = start as f32 (linear ramp 0..1000)
    let chr1_iter = (0u32..1000u32).map(|i| {
        let v = Value {
            start: i,
            end: i + 1,
            value: i as f32,
        };
        ("chr1", v)
    });

    // chr2: two intervals with value 10.0
    let chr2_vals = vec![
        ("chr2", Value { start: 100, end: 200, value: 10.0 }),
        ("chr2", Value { start: 300, end: 400, value: 10.0 }),
    ];
    let chr2_iter = chr2_vals.into_iter();

    let all_iter = chr1_iter.chain(chr2_iter);

    // Wrap in BedParserStreamingIterator via wrap_infallible_iter.
    // allow_out_of_order_chroms = false: data is sorted.
    let vals = BedParserStreamingIterator::wrap_infallible_iter(all_iter, false);

    // Chromosome sizes map.
    let mut chrom_sizes = HashMap::new();
    chrom_sizes.insert("chr1".to_string(), 1000u32);
    chrom_sizes.insert("chr2".to_string(), 500u32);

    // Build a tokio runtime for bigtools internal processing.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()
        .expect("Unable to create tokio runtime");

    // Create writer and write.
    let writer = BigWigWrite::create_file(out_path, chrom_sizes)?;
    writer.write(vals, runtime)?;

    let size = std::fs::metadata(out_path)?.len();
    println!(
        "Wrote fixture: {} ({} bytes / {:.1} KB)",
        out_path.display(),
        size,
        size as f64 / 1024.0
    );

    Ok(())
}

/// Read back the fixture and print a summary to verify correctness.
fn sanity_check(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut bw = BigWigRead::open_file(path)?;

    // Print chrom info.
    let chroms: Vec<_> = bw.chroms().to_vec();
    println!("Chromosomes in file:");
    for c in &chroms {
        println!("  {} length={}", c.name, c.length);
    }

    // Verify chr1 and chr2 present with correct lengths.
    let chr1_info = chroms.iter().find(|c| c.name == "chr1");
    let chr2_info = chroms.iter().find(|c| c.name == "chr2");

    match chr1_info {
        Some(c) => assert_eq!(c.length, 1000, "chr1 length mismatch"),
        None => panic!("chr1 not found in fixture"),
    }
    match chr2_info {
        Some(c) => assert_eq!(c.length, 500, "chr2 length mismatch"),
        None => panic!("chr2 not found in fixture"),
    }

    // Sample chr1 [0, 10) — expect values 0.0 .. 9.0
    let chr1_sample: Vec<Value> = bw
        .get_interval("chr1", 0, 10)?
        .collect::<Result<Vec<_>, _>>()?;
    let chr1_vals: Vec<f32> = chr1_sample.iter().map(|v| v.value).collect();
    println!("chr1 sample [0,10): {:?}", chr1_vals);

    // Sample chr1 tail [990, 1000) — expect values ~990..999
    let chr1_tail: Vec<Value> = bw
        .get_interval("chr1", 990, 1000)?
        .collect::<Result<Vec<_>, _>>()?;
    let chr1_tail_vals: Vec<f32> = chr1_tail.iter().map(|v| v.value).collect();
    println!("chr1 tail  [990,1000): {:?}", chr1_tail_vals);
    if let (Some(first), Some(last)) = (chr1_sample.first(), chr1_tail.last()) {
        println!("  first.value={} last.value={}", first.value, last.value);
        assert!(
            first.value < 1.0,
            "first bin value should be < 1.0, got {}",
            first.value
        );
        assert!(
            last.value > 900.0,
            "last bin value should be > 900.0, got {}",
            last.value
        );
    }

    // Sample chr2 [100, 200) — expect value 10.0
    let chr2_vals: Vec<Value> = bw
        .get_interval("chr2", 100, 200)?
        .collect::<Result<Vec<_>, _>>()?;
    let chr2_v: Vec<f32> = chr2_vals.iter().map(|v| v.value).collect();
    println!("chr2 [100,200): {:?}", chr2_v);
    assert!(
        chr2_vals.iter().all(|v| (v.value - 10.0).abs() < 0.001),
        "chr2 [100,200) should all be 10.0"
    );

    // Sample chr2 [300, 400) — expect value 10.0
    let chr2_vals2: Vec<Value> = bw
        .get_interval("chr2", 300, 400)?
        .collect::<Result<Vec<_>, _>>()?;
    let chr2_v2: Vec<f32> = chr2_vals2.iter().map(|v| v.value).collect();
    println!("chr2 [300,400): {:?}", chr2_v2);
    assert!(
        chr2_vals2.iter().all(|v| (v.value - 10.0).abs() < 0.001),
        "chr2 [300,400) should all be 10.0"
    );

    // Sample chr2 [0, 100) — expect empty (no data)
    let chr2_empty: Vec<Value> = bw
        .get_interval("chr2", 0, 100)?
        .collect::<Result<Vec<_>, _>>()?;
    println!("chr2 [0,100) (should be empty): {:?}", chr2_empty);
    assert!(chr2_empty.is_empty(), "chr2 [0,100) should have no data");

    println!("Sanity check passed.");
    Ok(())
}
