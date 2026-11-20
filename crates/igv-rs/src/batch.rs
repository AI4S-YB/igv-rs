//! Headless IGV `.txt`/`.igv` batch script runner (`--batch script.igv`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use igv_core::alignment::{AlignmentColorMode, AlignmentGroupMode, AlignmentRenderOpts, AlignmentSortMode};
use igv_core::collect_render_inputs;
use igv_core::genome::Cytoband;
use igv_core::pedigree::Pedigree;
use igv_core::region::Region;
use igv_core::source::annotation::find_by_name_union;
use igv_core::source::gwas::GwasSource;
use igv_core::source::link::LinkSource;
use igv_core::source::seg::SegSource;
use igv_core::source::{
    AnnotationSource, BamSource, FastaSource, FetchSignalOpts, RefMeta, SignalSource, SignalSummary,
    VcfSource,
};
use igv_core::{CollectOpts, Sources};
use igv_render::{render_circular_png, render_circular_svg, render_png, render_svg, CircularOptions, SvgOptions};

use crate::app::action::SnapshotFormat;
use crate::app::open::{detect_kind, open_genome_spec, open_track_spec, DetectedTrackKind, OpenedTrack};
use crate::snapshot::naming::auto_name;

pub async fn run_batch_script(
    script_path: &Path,
    auth_token: Option<&str>,
    width_px: u32,
) -> Result<()> {
    let text = std::fs::read_to_string(script_path)
        .with_context(|| format!("read batch script {}", script_path.display()))?;

    let mut fasta: Option<Arc<dyn FastaSource>> = None;
    let mut references: Vec<RefMeta> = Vec::new();
    let mut cytobands: Vec<Cytoband> = Vec::new();
    let mut chrom_aliases = igv_core::genome::ChromAliases::default();
    let mut genome_id: Option<String> = None;
    let mut pedigree: Option<Arc<Pedigree>> = None;

    let mut vcfs: Vec<(String, Arc<dyn VcfSource>)> = Vec::new();
    let mut bams: Vec<(String, Arc<dyn BamSource>)> = Vec::new();
    let mut annotations: Vec<(String, Arc<dyn AnnotationSource>)> = Vec::new();
    let mut signals: Vec<(String, Arc<dyn SignalSource>)> = Vec::new();
    let mut links: Vec<(String, Arc<dyn LinkSource>)> = Vec::new();
    let mut segs: Vec<(String, Arc<dyn SegSource>)> = Vec::new();
    let mut gwas: Vec<(String, Arc<dyn GwasSource>)> = Vec::new();

    let mut region: Option<Region> = None;
    let mut snapshot_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut alignment_opts = AlignmentRenderOpts::default();
    let mut tag_name: Option<String> = None;
    let mut show_sashimi = false;
    let mut signal_summary = SignalSummary::Max;
    let mut width = width_px.max(300);

    for (line_no, raw_line) in text.lines().enumerate() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        let tokens = split_shell_words(trimmed);
        if tokens.is_empty() {
            continue;
        }
        let cmd = tokens[0].as_str();
        let arg1 = tokens.get(1).map(String::as_str).unwrap_or("");

        match cmd {
            "new" => {
                vcfs.clear();
                bams.clear();
                annotations.clear();
                signals.clear();
                links.clear();
                segs.clear();
                gwas.clear();
                alignment_opts = AlignmentRenderOpts::default();
                show_sashimi = false;
            }
            "genome" => {
                if arg1.is_empty() {
                    anyhow::bail!("batch line {}: genome requires an argument", line_no + 1);
                }
                let loaded = open_genome_spec(arg1, auth_token).await?;
                genome_id = Some(loaded.genome_id);
                cytobands = loaded.cytobands;
                chrom_aliases = loaded.aliases;
                references = loaded.references;
                fasta = Some(loaded.fasta);
                if region.is_none() {
                    if let Some(first) = references.first() {
                        region = Some(Region::new(first.name.clone(), 1, first.length.min(1000))?);
                    }
                }
            }
            "load" => {
                for spec in &tokens[1..] {
                    if detect_kind(spec) == Some(DetectedTrackKind::Hub) {
                        let hub = igv_core::hub::parse_hub(spec, auth_token).await?;
                        if fasta.is_none() {
                            if let Some(tb) = &hub.two_bit_url {
                                let loaded = open_genome_spec(tb, auth_token).await?;
                                chrom_aliases = loaded.aliases;
                                references = loaded.references;
                                fasta = Some(loaded.fasta);
                            }
                        }
                        if let Some(fa) = fasta.clone() {
                            for ht in hub.tracks {
                                if let Ok(opened) = open_track_spec(
                                    &ht.data_url,
                                    &fa,
                                    tag_name.as_deref(),
                                    pedigree.clone(),
                                    None,
                                    None,
                                    None,
                                    auth_token,
                                )
                                .await
                                {
                                    match opened {
                                        OpenedTrack::Alignment(d, s) => bams.push((d, s)),
                                        OpenedTrack::Variant(d, s) => vcfs.push((d, s)),
                                        OpenedTrack::Annotation(d, s) => annotations.push((d, s)),
                                        OpenedTrack::Signal(d, s) => signals.push((d, s)),
                                        OpenedTrack::Link(d, s) => links.push((d, s)),
                                        OpenedTrack::Seg(d, s) => segs.push((d, s)),
                                        OpenedTrack::Gwas(d, s) => gwas.push((d, s)),
                                        OpenedTrack::Pedigree(p) => pedigree = Some(p),
                                        OpenedTrack::Hub(_) => {}
                                    }
                                }
                            }
                        }
                        continue;
                    }
                    let fa = fasta.clone().ok_or_else(|| {
                        anyhow::anyhow!("batch line {}: load called before genome", line_no + 1)
                    })?;
                    match open_track_spec(
                        spec,
                        &fa,
                        tag_name.as_deref(),
                        pedigree.clone(),
                        None,
                        None,
                        None,
                        auth_token,
                    )
                    .await?
                    {
                        OpenedTrack::Alignment(d, s) => bams.push((d, s)),
                        OpenedTrack::Variant(d, s) => vcfs.push((d, s)),
                        OpenedTrack::Annotation(d, s) => annotations.push((d, s)),
                        OpenedTrack::Signal(d, s) => signals.push((d, s)),
                        OpenedTrack::Link(d, s) => links.push((d, s)),
                        OpenedTrack::Seg(d, s) => segs.push((d, s)),
                        OpenedTrack::Gwas(d, s) => gwas.push((d, s)),
                        OpenedTrack::Pedigree(p) => pedigree = Some(p),
                        OpenedTrack::Hub(_) => {}
                    }
                }
            }
            "goto" => {
                if arg1.is_empty() {
                    continue;
                }
                let avail: Vec<String> = references.iter().map(|m| m.name.clone()).collect();
                let parsed = Region::parse(arg1).ok();
                let resolved = parsed.as_ref().and_then(|r| {
                    chrom_aliases.resolve(&r.chrom, &avail).map(|canon| {
                        let mut out = r.clone();
                        out.chrom = canon.to_string();
                        out
                    })
                });
                if let Some(r) = resolved {
                    region = Some(r);
                } else {
                    let srcs: Vec<_> = annotations.iter().map(|(_, s)| Arc::clone(s)).collect();
                    if let Some((r, _)) = find_by_name_union(&srcs, arg1) {
                        region = Some(r);
                    } else if avail.is_empty() {
                        if let Some(r) = parsed {
                            region = Some(r);
                        } else {
                            anyhow::bail!("batch line {}: cannot resolve locus '{}'", line_no + 1, arg1);
                        }
                    } else {
                        anyhow::bail!("batch line {}: cannot resolve locus '{}'", line_no + 1, arg1);
                    }
                }
            }
            "snapshotDirectory" | "snapshotdirectory" => {
                if !arg1.is_empty() {
                    snapshot_dir = PathBuf::from(arg1);
                    std::fs::create_dir_all(&snapshot_dir)?;
                }
            }
            "snapshot" => {
                let fa = fasta.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("batch line {}: snapshot without genome", line_no + 1)
                })?;
                let r = match &region {
                    Some(r) => r.clone(),
                    None => {
                        let first = references.first().ok_or_else(|| {
                            anyhow::anyhow!("batch line {}: no chromosomes in genome", line_no + 1)
                        })?;
                        Region::new(first.name.clone(), 1, first.length.min(1000))?
                    }
                };
                std::fs::create_dir_all(&snapshot_dir)?;
                let out_path = if arg1.is_empty() {
                    snapshot_dir.join(auto_name(&r, SnapshotFormat::Svg))
                } else {
                    let p = PathBuf::from(arg1);
                    if p.is_absolute() {
                        p
                    } else {
                        snapshot_dir.join(p)
                    }
                };
                let fmt = SnapshotFormat::from_path(&out_path);
                let sources = Sources {
                    fasta: Arc::clone(fa),
                    vcf: None,
                    vcfs: vcfs.clone(),
                    bams: bams.clone(),
                    annotations: annotations.clone(),
                    signals: signals.clone(),
                    links: links.clone(),
                    segs: segs.clone(),
                    gwas: gwas.clone(),
                    references: references.clone(),
                    cytobands: cytobands.clone(),
                };
                let mode = igv_core::render::Thresholds::default().classify(r.width());
                let copts = CollectOpts {
                    signal_opts: FetchSignalOpts {
                        max_bins: width,
                        summary: signal_summary,
                    },
                    alignment_opts: alignment_opts.clone(),
                    render_mode: mode,
                    ..CollectOpts::default()
                };
                let inputs = collect_render_inputs(&sources, &r, &copts).await?;
                let svg_opts = SvgOptions {
                    width_px: width,
                    show_ideogram: !cytobands.is_empty(),
                    show_sequence: !inputs.reference_seq.is_empty() && r.width() <= 240,
                    show_coverage_snv: true,
                    show_sashimi,
                    ..Default::default()
                };
                match fmt {
                    SnapshotFormat::Svg => {
                        std::fs::write(&out_path, render_svg(&inputs, &svg_opts))?;
                    }
                    SnapshotFormat::Png => {
                        std::fs::write(&out_path, render_png(&inputs, &svg_opts)?)?;
                    }
                }
            }
            "circular" => {
                std::fs::create_dir_all(&snapshot_dir)?;
                let name = if arg1.is_empty() {
                    "circular.svg"
                } else {
                    arg1
                };
                let out_path = snapshot_dir.join(name);
                let mut all_links = Vec::new();
                for (_, lk) in &links {
                    all_links.extend(lk.all_records());
                }
                let copts = CircularOptions {
                    title: genome_id.clone(),
                    ..Default::default()
                };
                match SnapshotFormat::from_path(&out_path) {
                    SnapshotFormat::Svg => {
                        std::fs::write(
                            &out_path,
                            render_circular_svg(&references, &all_links, &[], &copts),
                        )?;
                    }
                    SnapshotFormat::Png => {
                        std::fs::write(
                            &out_path,
                            render_circular_png(&references, &all_links, &[], &copts)?,
                        )?;
                    }
                }
            }
            "sort" => {
                if let Some(m) = AlignmentSortMode::parse(arg1) {
                    alignment_opts.sort_mode = m;
                }
            }
            "group" => {
                if let Some(m) = AlignmentGroupMode::parse(arg1) {
                    alignment_opts.group_mode = m;
                }
            }
            "colorBy" | "colorby" | "color" => {
                if let Some(m) = AlignmentColorMode::parse(arg1) {
                    alignment_opts.color_mode = m;
                }
                if let Some(t) = tokens.get(2) {
                    tag_name = Some(t.clone());
                }
            }
            "viewaspairs" | "pairs" => {
                alignment_opts.view_as_pairs = !matches!(arg1, "false" | "off" | "0");
            }
            "showSoftClips" | "softclip" => {
                alignment_opts.show_soft_clips = !matches!(arg1, "false" | "off" | "0");
            }
            "sashimi" => {
                show_sashimi = !matches!(arg1, "false" | "off" | "0");
            }
            "summary" => {
                if let Some(s) = SignalSummary::parse(arg1) {
                    signal_summary = s;
                }
            }
            "maxPanelHeight" | "snapshotWidth" => {
                if let Ok(w) = arg1.parse::<u32>() {
                    width = w.max(300);
                }
            }
            "squish" | "expand" | "collapse" | "preference" | "setSleepInterval" => {}
            "exit" | "quit" => break,
            other => {
                tracing::warn!("batch line {}: ignoring unknown command '{}'", line_no + 1, other);
            }
        }
    }
    Ok(())
}

fn split_shell_words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes: Option<char> = None;
    for ch in s.chars() {
        match (in_quotes, ch) {
            (Some(q), c) if c == q => in_quotes = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => in_quotes = Some(ch),
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}
