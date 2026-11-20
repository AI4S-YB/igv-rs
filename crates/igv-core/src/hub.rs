//! UCSC Track Hub (`hub.txt` -> `genomes.txt` -> `trackDb.txt` + `useOneFile on`) parser.

use std::collections::HashMap;

use crate::error::Result;
use crate::io::{fetch_text_maybe_gz, Location};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HubTrackKind {
    Signal,
    Annotation,
    Alignment,
    Variant,
    Link,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HubTrackDescriptor {
    pub name: String,
    pub short_label: String,
    pub track_type: String,
    pub kind: HubTrackKind,
    pub data_url: String,
}

#[derive(Debug, Clone, Default)]
pub struct HubDescriptor {
    pub hub_name: String,
    pub short_label: String,
    pub genome_id: Option<String>,
    pub two_bit_url: Option<String>,
    pub tracks: Vec<HubTrackDescriptor>,
}

/// Parse a UCSC Track Hub (`hub.txt`), supporting both single-file (`useOneFile on`)
/// and multi-file (`genomesFile` + `trackDb`) hubs.
pub async fn parse_hub(spec: &str, auth_token: Option<&str>) -> Result<HubDescriptor> {
    let hub_loc = Location::parse(spec);
    let hub_text = fetch_text_maybe_gz(&hub_loc, auth_token).await?;
    let stanzas = parse_stanzas(&hub_text);

    let mut desc = HubDescriptor::default();
    let mut genomes_file: Option<String> = None;
    let mut use_one_file = false;

    for stanza in &stanzas {
        if let Some(hub_name) = stanza.get("hub") {
            desc.hub_name = hub_name.clone();
            desc.short_label = stanza
                .get("shortLabel")
                .cloned()
                .unwrap_or_else(|| hub_name.clone());
            if let Some(gf) = stanza.get("genomesFile") {
                genomes_file = Some(gf.clone());
            }
            if stanza
                .get("useOneFile")
                .is_some_and(|v| v.eq_ignore_ascii_case("on"))
            {
                use_one_file = true;
            }
        }
    }

    if use_one_file {
        collect_genome_and_tracks(&stanzas, &hub_loc, &mut desc);
        return Ok(desc);
    }

    if let Some(gf) = genomes_file {
        let gf_loc = hub_loc.resolve_relative(&gf);
        let gf_text = fetch_text_maybe_gz(&gf_loc, auth_token).await?;
        let g_stanzas = parse_stanzas(&gf_text);
        collect_genome_and_tracks(&g_stanzas, &gf_loc, &mut desc);

        for g in &g_stanzas {
            if let Some(tdb) = g.get("trackDb") {
                let tdb_loc = gf_loc.resolve_relative(tdb);
                if let Ok(tdb_text) = fetch_text_maybe_gz(&tdb_loc, auth_token).await {
                    let t_stanzas = parse_stanzas(&tdb_text);
                    collect_genome_and_tracks(&t_stanzas, &tdb_loc, &mut desc);
                }
            }
        }
    } else {
        // Fallback: check if caller passed a trackDb.txt directly
        collect_genome_and_tracks(&stanzas, &hub_loc, &mut desc);
    }

    Ok(desc)
}

fn collect_genome_and_tracks(
    stanzas: &[HashMap<String, String>],
    base_loc: &Location,
    desc: &mut HubDescriptor,
) {
    for stanza in stanzas {
        if let Some(g) = stanza.get("genome") {
            if desc.genome_id.is_none() {
                desc.genome_id = Some(g.clone());
            }
            if let Some(tb) = stanza.get("twoBitPath") {
                desc.two_bit_url = Some(base_loc.resolve_relative(tb).as_str());
            }
        }
        if let (Some(track_name), Some(big_url)) = (stanza.get("track"), stanza.get("bigDataUrl")) {
            let track_type = stanza.get("type").cloned().unwrap_or_default();
            let Some(kind) = classify_track_type(&track_type, big_url) else {
                continue;
            };
            let short_label = stanza
                .get("shortLabel")
                .cloned()
                .unwrap_or_else(|| track_name.clone());
            let data_url = base_loc.resolve_relative(big_url).as_str();
            desc.tracks.push(HubTrackDescriptor {
                name: track_name.clone(),
                short_label,
                track_type,
                kind,
                data_url,
            });
        }
    }
}

fn classify_track_type(track_type: &str, url: &str) -> Option<HubTrackKind> {
    let tl = track_type.to_ascii_lowercase();
    let ul = url.to_ascii_lowercase();
    if tl.starts_with("bigwig") || ul.ends_with(".bw") || ul.ends_with(".bigwig") {
        Some(HubTrackKind::Signal)
    } else if tl.starts_with("biginteract") || ul.ends_with(".inter.bb") {
        Some(HubTrackKind::Link)
    } else if tl.starts_with("bigbed")
        || tl.starts_with("biggenepred")
        || tl.starts_with("bigpsl")
        || ul.ends_with(".bb")
    {
        Some(HubTrackKind::Annotation)
    } else if tl.starts_with("bam") || tl.starts_with("cram") {
        Some(HubTrackKind::Alignment)
    } else if tl.starts_with("vcf") {
        Some(HubTrackKind::Variant)
    } else {
        None
    }
}

fn parse_stanzas(text: &str) -> Vec<HashMap<String, String>> {
    let mut out = Vec::new();
    let mut cur: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once(char::is_whitespace) {
            cur.insert(k.trim().to_string(), v.trim().to_string());
        } else {
            cur.insert(trimmed.to_string(), String::new());
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parses_igv_hub_fixture_if_present() {
        let base = std::env::var_os("IGV_TEST_DATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../test/data"));
        let p = base.join("hubs/hub.txt");
        if p.exists() {
            let hub = parse_hub(p.to_str().unwrap(), None).await.unwrap();
            assert_eq!(hub.genome_id.as_deref(), Some("GCF_000186305.1"));
            assert!(hub.hub_name.contains("GCF_000186305.1"));
            assert!(hub.two_bit_url.is_some());
            assert!(!hub.tracks.is_empty());
        }
    }
}
