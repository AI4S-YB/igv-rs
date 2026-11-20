//! Session persistence (`.toml`) and desktop IGV `.json` session import.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::app::state::AppState;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionFile {
    pub genome: Option<String>,
    pub fasta: Option<String>,
    pub regions: Vec<String>,
    pub pedigree: Option<String>,
    pub bams: Vec<String>,
    pub vcfs: Vec<String>,
    pub annotations: Vec<String>,
    pub signals: Vec<String>,
    pub links: Vec<String>,
    pub segs: Vec<String>,
    pub gwas: Vec<String>,
    pub color_by: Option<String>,
    pub tag: Option<String>,
    pub sort_by: Option<String>,
    pub group_by: Option<String>,
    pub view_as_pairs: Option<bool>,
    pub show_soft_clips: Option<bool>,
    pub max_depth: Option<usize>,
    pub show_sashimi: Option<bool>,
    pub signal_summary: Option<String>,
    pub motif: Option<String>,
}

impl SessionFile {
    pub fn from_state(state: &AppState, fasta_or_genome: Option<String>) -> Self {
        Self {
            genome: state.genome_id.clone(),
            fasta: fasta_or_genome,
            regions: state.panes.iter().map(|r| r.to_string()).collect(),
            pedigree: None,
            bams: state
                .bams
                .iter()
                .map(|t| t.path.to_string_lossy().into_owned())
                .collect(),
            vcfs: state
                .vcf_tracks
                .iter()
                .map(|t| t.path.to_string_lossy().into_owned())
                .collect(),
            annotations: state
                .annotations
                .iter()
                .map(|t| t.path.to_string_lossy().into_owned())
                .collect(),
            signals: state
                .signals
                .iter()
                .map(|t| t.path.to_string_lossy().into_owned())
                .collect(),
            links: state
                .links
                .iter()
                .map(|t| t.path.to_string_lossy().into_owned())
                .collect(),
            segs: state
                .segs
                .iter()
                .map(|t| t.path.to_string_lossy().into_owned())
                .collect(),
            gwas: state
                .gwas
                .iter()
                .map(|t| t.path.to_string_lossy().into_owned())
                .collect(),
            color_by: Some(state.alignment_opts.color_mode.name().to_string()),
            tag: state.bam_tag.clone(),
            sort_by: Some(state.alignment_opts.sort_mode.name().to_string()),
            group_by: Some(state.alignment_opts.group_mode.name().to_string()),
            view_as_pairs: Some(state.alignment_opts.view_as_pairs),
            show_soft_clips: Some(state.alignment_opts.show_soft_clips),
            max_depth: state.alignment_opts.max_depth,
            show_sashimi: Some(state.show_sashimi),
            signal_summary: None,
            motif: state.sequence_opts.motif_pattern.clone(),
        }
    }

    pub fn save_toml(&self, path: &Path) -> Result<()> {
        let body = toml::to_string_pretty(self).context("serialize session toml")?;
        std::fs::write(path, body).with_context(|| format!("write session {}", path.display()))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("read session {}", path.display()))?;
        let is_json = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
            || raw.trim_start().starts_with('{');
        if is_json {
            parse_igv_json_session(&raw)
        } else {
            toml::from_str(&raw).with_context(|| format!("parse TOML session {}", path.display()))
        }
    }
}

#[derive(Debug, Deserialize)]
struct IgvJsonSession {
    genome: Option<String>,
    locus: Option<serde_json::Value>,
    reference: Option<IgvJsonRef>,
    #[serde(default)]
    tracks: Vec<IgvJsonTrack>,
}

#[derive(Debug, Deserialize)]
struct IgvJsonRef {
    id: Option<String>,
    #[serde(rename = "fastaURL")]
    fasta_url: Option<String>,
    #[serde(rename = "twoBitURL")]
    two_bit_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IgvJsonTrack {
    url: Option<String>,
    path: Option<String>,
    format: Option<String>,
    #[serde(rename = "type")]
    track_type: Option<String>,
}

fn parse_igv_json_session(raw: &str) -> Result<SessionFile> {
    let j: IgvJsonSession = serde_json::from_str(raw).context("parse IGV JSON session")?;
    let mut sess = SessionFile::default();
    if let Some(g) = j.genome {
        sess.genome = Some(g);
    } else if let Some(r) = j.reference {
        sess.fasta = r.fasta_url.or(r.two_bit_url);
        if sess.fasta.is_none() {
            sess.genome = r.id;
        }
    }
    if let Some(loc) = j.locus {
        match loc {
            serde_json::Value::String(s) => sess.regions.push(s),
            serde_json::Value::Array(arr) => {
                for v in arr {
                    if let Some(s) = v.as_str() {
                        sess.regions.push(s.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    for tr in j.tracks {
        let Some(url) = tr.url.or(tr.path) else {
            continue;
        };
        let hint = tr
            .format
            .or(tr.track_type)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(hint.as_str(), "bam" | "cram" | "sam" | "paf" | "alignment") {
            sess.bams.push(url);
        } else if matches!(hint.as_str(), "vcf" | "bcf" | "variant") {
            sess.vcfs.push(url);
        } else if matches!(hint.as_str(), "bw" | "bigwig" | "wig" | "bedgraph") {
            sess.signals.push(url);
        } else if matches!(hint.as_str(), "bedpe" | "interact" | "biginteract") {
            sess.links.push(url);
        } else if matches!(hint.as_str(), "seg" | "cn" | "cbs") {
            sess.segs.push(url);
        } else if matches!(hint.as_str(), "gwas" | "assoc") {
            sess.gwas.push(url);
        } else if matches!(hint.as_str(), "ped" | "pedigree") {
            sess.pedigree = Some(url);
        } else {
            sess.annotations.push(url);
        }
    }
    Ok(sess)
}
