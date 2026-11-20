//! Shared format-dispatching track/genome/hub loaders reused across CLI
//! startup, runtime `:load`/`:genome`, `--session`, `--batch`, and `--port`.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use igv_core::genome::LoadedGenome;
use igv_core::hub::{parse_hub, HubDescriptor, HubTrackKind};
use igv_core::io::Location;
use igv_core::pedigree::Pedigree;
use igv_core::source::{
    open_alignment, open_annotation_location, open_link_location, open_signal_location,
    open_variant, AnnotationFormat, AnnotationSource, BamSource, FastaSource, FileGwasSource,
    FileSegSource, GwasSource, LinkFormat, LinkSource, SegSource, SignalFormat, SignalSource,
    VcfSource,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedTrackKind {
    Alignment,
    Variant,
    Annotation,
    Signal,
    Link,
    Seg,
    Gwas,
    Pedigree,
    Hub,
}

pub enum OpenedTrack {
    Alignment(String, Arc<dyn BamSource>),
    Variant(String, Arc<dyn VcfSource>),
    Annotation(String, Arc<dyn AnnotationSource>),
    Signal(String, Arc<dyn SignalSource>),
    Link(String, Arc<dyn LinkSource>),
    Seg(String, Arc<dyn SegSource>),
    Gwas(String, Arc<dyn GwasSource>),
    Pedigree(Arc<Pedigree>),
    Hub(HubDescriptor),
}

/// Detect track kind from a file path or remote URL.
pub fn detect_kind(spec: &str) -> Option<DetectedTrackKind> {
    let lower = Location::parse(spec).lowercase_name();
    let stripped = lower.strip_suffix(".gz").unwrap_or(&lower);
    if stripped.ends_with(".bam")
        || stripped.ends_with(".cram")
        || stripped.ends_with(".sam")
        || stripped.ends_with(".paf")
    {
        return Some(DetectedTrackKind::Alignment);
    }
    if stripped.ends_with(".vcf") || stripped.ends_with(".bcf") {
        return Some(DetectedTrackKind::Variant);
    }
    if stripped.ends_with(".gff")
        || stripped.ends_with(".gff3")
        || stripped.ends_with(".gtf")
        || stripped.ends_with(".bed")
        || stripped.ends_with(".narrowpeak")
        || stripped.ends_with(".broadpeak")
        || stripped.ends_with(".bb")
        || stripped.ends_with(".bigbed")
    {
        if stripped.ends_with(".inter.bb") {
            return Some(DetectedTrackKind::Link);
        }
        return Some(DetectedTrackKind::Annotation);
    }
    if stripped.ends_with(".bw")
        || stripped.ends_with(".bigwig")
        || stripped.ends_with(".wig")
        || stripped.ends_with(".bedgraph")
        || stripped.ends_with(".bg")
    {
        return Some(DetectedTrackKind::Signal);
    }
    if stripped.ends_with(".bedpe") || stripped.ends_with(".inter.bb") {
        return Some(DetectedTrackKind::Link);
    }
    if stripped.ends_with(".seg") || stripped.ends_with(".cbs") || stripped.ends_with(".cn") {
        return Some(DetectedTrackKind::Seg);
    }
    if stripped.ends_with(".gwas") || stripped.ends_with(".assoc") {
        return Some(DetectedTrackKind::Gwas);
    }
    if stripped.ends_with(".ped") {
        return Some(DetectedTrackKind::Pedigree);
    }
    if stripped.ends_with("hub.txt") {
        return Some(DetectedTrackKind::Hub);
    }
    None
}

pub async fn open_genome_spec(spec: &str, auth_token: Option<&str>) -> Result<LoadedGenome> {
    igv_core::genome::open_genome(spec, auth_token)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

#[allow(clippy::too_many_arguments)]
pub async fn open_track_spec(
    spec: &str,
    fasta: &Arc<dyn FastaSource>,
    tag: Option<&str>,
    pedigree: Option<Arc<Pedigree>>,
    ann_fmt: Option<AnnotationFormat>,
    sig_fmt: Option<SignalFormat>,
    link_fmt: Option<LinkFormat>,
    auth_token: Option<&str>,
) -> Result<OpenedTrack> {
    let loc = Location::parse(spec);
    let display = loc.display_name();
    let kind = detect_kind(spec).ok_or_else(|| {
        anyhow::anyhow!("unable to determine track format for '{spec}'")
    })?;

    match kind {
        DetectedTrackKind::Alignment => {
            let src = open_alignment(spec, Arc::clone(fasta), tag, auth_token).await?;
            Ok(OpenedTrack::Alignment(display, src))
        }
        DetectedTrackKind::Variant => {
            let src = open_variant(spec, pedigree, auth_token).await?;
            Ok(OpenedTrack::Variant(display, src))
        }
        DetectedTrackKind::Annotation => {
            let src = open_annotation_location(&loc, ann_fmt, auth_token).await?;
            Ok(OpenedTrack::Annotation(display, src))
        }
        DetectedTrackKind::Signal => {
            let src = open_signal_location(&loc, sig_fmt, auth_token).await?;
            Ok(OpenedTrack::Signal(display, src))
        }
        DetectedTrackKind::Link => {
            let src = open_link_location(&loc, link_fmt, auth_token).await?;
            Ok(OpenedTrack::Link(display, src))
        }
        DetectedTrackKind::Seg => {
            let src: Arc<dyn SegSource> =
                Arc::new(FileSegSource::open_location(&loc, auth_token).await?);
            Ok(OpenedTrack::Seg(display, src))
        }
        DetectedTrackKind::Gwas => {
            let src: Arc<dyn GwasSource> =
                Arc::new(FileGwasSource::open_location(&loc, auth_token).await?);
            Ok(OpenedTrack::Gwas(display, src))
        }
        DetectedTrackKind::Pedigree => {
            let ped = Arc::new(Pedigree::open(spec, auth_token).await?);
            Ok(OpenedTrack::Pedigree(ped))
        }
        DetectedTrackKind::Hub => {
            let hub = parse_hub(spec, auth_token).await?;
            Ok(OpenedTrack::Hub(hub))
        }
    }
}

pub fn spec_to_pathbuf(spec: &str) -> PathBuf {
    match Location::parse(spec) {
        Location::Local(p) => p,
        Location::Remote(u) => PathBuf::from(u),
    }
}

pub fn hub_track_kind_matches_fallback(kind: HubTrackKind, url: &str) -> DetectedTrackKind {
    match kind {
        HubTrackKind::Alignment => DetectedTrackKind::Alignment,
        HubTrackKind::Variant => DetectedTrackKind::Variant,
        HubTrackKind::Annotation => DetectedTrackKind::Annotation,
        HubTrackKind::Signal => DetectedTrackKind::Signal,
        HubTrackKind::Link => {
            let _ = url;
            DetectedTrackKind::Link
        }
    }
}
