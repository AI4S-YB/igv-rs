//! End-to-end integration tests for new igv-rs capabilities:
//! sessions, headless batch scripts, TCP socket server, runtime `:` commands,
//! split panes, mouse hit-testing, and TUI widgets on TestBackend.

use std::sync::Arc;

use async_trait::async_trait;
use igv_core::alignment::{AlignmentColorMode, AlignmentGroupMode, AlignmentSortMode};
use igv_core::region::Region;
use igv_core::sashimi::SpliceJunction;
use igv_core::source::annotation::Strand;
use igv_core::source::gwas::GwasPoint;
use igv_core::source::seg::SegInterval;
use igv_core::source::{FastaSource, RefMeta};
use igv_rs::app::action::Action;
use igv_rs::app::state::{AppState, InspectorPopup};
use igv_rs::batch::run_batch_script;
use igv_rs::session::SessionFile;
use igv_rs::socket::spawn_socket_server;
use igv_rs::ui::hit::{handle_click, handle_drag, FrameHitMap};
use igv_rs::ui::layout::{compute, split_panes, LayoutSpec};
use igv_rs::ui::theme::Theme;
use igv_rs::ui::widgets::{gwas::GwasWidget, inspector::InspectorWidget, sashimi::SashimiWidget, seg::SegWidget};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[derive(Debug)]
struct StubFasta;

#[async_trait]
impl FastaSource for StubFasta {
    async fn references(&self) -> igv_core::error::Result<Vec<RefMeta>> {
        Ok(vec![RefMeta {
            name: "chr1".into(),
            length: 100_000,
        }])
    }
    async fn fetch(&self, r: &Region) -> igv_core::error::Result<Vec<u8>> {
        Ok(vec![b'A'; r.width() as usize])
    }
}

#[test]
fn session_toml_roundtrip_and_igv_json_import() {
    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("sess.toml");
    let state = AppState {
        genome_id: Some("hg38".into()),
        panes: vec![Region::new("chr1", 100, 500).unwrap()],
        show_sashimi: true,
        ..AppState::default()
    };
    let s = SessionFile::from_state(&state, Some("hg38".into()));
    s.save_toml(&toml_path).unwrap();
    let reloaded = SessionFile::load(&toml_path).unwrap();
    assert_eq!(reloaded.genome.as_deref(), Some("hg38"));
    assert_eq!(reloaded.show_sashimi, Some(true));

    let json_path = dir.path().join("desktop.json");
    std::fs::write(
        &json_path,
        r#"{
            "genome": "mm39",
            "locus": ["chr1:100-500", "chr2:200-400"],
            "tracks": [
                { "url": "reads.bam", "format": "bam" },
                { "url": "vars.vcf.gz", "format": "vcf" },
                { "url": "cnv.seg", "format": "seg" }
            ]
        }"#,
    )
    .unwrap();
    let imported = SessionFile::load(&json_path).unwrap();
    assert_eq!(imported.genome.as_deref(), Some("mm39"));
    assert_eq!(imported.regions.len(), 2);
    assert_eq!(imported.bams, vec!["reads.bam"]);
    assert_eq!(imported.vcfs, vec!["vars.vcf.gz"]);
    assert_eq!(imported.segs, vec!["cnv.seg"]);
}

#[tokio::test]
async fn headless_batch_script_renders_svg_png_sashimi_and_circular() {
    let dir = tempfile::tempdir().unwrap();
    let fa_path = dir.path().join("ref.fa");
    std::fs::write(&fa_path, ">chr1\nATGCCATGGCTAGCTAGCTAGCTAGCTAGCTA\n").unwrap();
    std::fs::write(dir.path().join("ref.fa.fai"), "chr1\t32\t6\t32\t33\n").unwrap();

    let sam_path = dir.path().join("reads.sam");
    std::fs::write(
        &sam_path,
        "@HD\tVN:1.6\n@SQ\tSN:chr1\tLN:32\nr1\t0\tchr1\t2\t60\t8M10N8M\t*\t0\t0\t*\t*\tXS:A:+\n",
    )
    .unwrap();

    let bedpe_path = dir.path().join("loops.bedpe");
    std::fs::write(&bedpe_path, "chr1\t1\t5\tchr1\t20\t28\tloop1\t9\n").unwrap();

    let out_dir = dir.path().join("out");
    let script = dir.path().join("run.igv");
    std::fs::write(
        &script,
        format!(
            "new\ngenome {}\nload {} {}\nsashimi true\ncolorBy strand\nsort start\nsnapshotDirectory {}\ngoto chr1:1-32\nsnapshot view.svg\nsnapshot view.png\ncircular circ.svg\n",
            fa_path.display(),
            sam_path.display(),
            bedpe_path.display(),
            out_dir.display(),
        ),
    )
    .unwrap();

    run_batch_script(&script, None, 800).await.unwrap();
    let svg = std::fs::read_to_string(out_dir.join("view.svg")).unwrap();
    assert!(svg.contains("reads.sam (sashimi)"));
    let png = std::fs::read(out_dir.join("view.png")).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let circ = std::fs::read_to_string(out_dir.join("circ.svg")).unwrap();
    assert!(circ.contains(">chr1<"));
}

#[tokio::test]
async fn socket_listener_handles_echo_and_commands() {
    let (addr, mut rx, jh) = spawn_socket_server(0).await.unwrap();

    let server_handler = tokio::spawn(async move {
        if let Some(cmd) = rx.recv().await {
            assert_eq!(cmd.line, "getLocus");
            let _ = cmd.reply.send("chr1:100-500".into());
        }
        if let Some(cmd) = rx.recv().await {
            assert_eq!(cmd.line, "goto chr2:1-1000");
            let _ = cmd.reply.send("OK".into());
        }
    });

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let (rd, mut wr) = stream.split();
    let mut lines = BufReader::new(rd).lines();

    wr.write_all(b"echo hello\n").await.unwrap();
    assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("hello"));

    wr.write_all(b"getLocus\n").await.unwrap();
    assert_eq!(
        lines.next_line().await.unwrap().as_deref(),
        Some("chr1:100-500")
    );

    wr.write_all(b"goto chr2:1-1000\n").await.unwrap();
    assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("OK"));

    drop(stream);
    server_handler.await.unwrap();
    jh.abort();
}

#[test]
fn runtime_commands_split_panes_and_mouse_hit_work() {
    let mut state = AppState {
        fasta: Arc::new(StubFasta),
        references: vec![
            RefMeta { name: "chr1".into(), length: 100_000 },
            RefMeta { name: "chr2".into(), length: 50_000 },
        ],
        reference_seq: vec![b'A'; 50],
        region: Region::new("chr1", 1, 50).unwrap(),
        panes: vec![Region::new("chr1", 1, 50).unwrap()],
        gwas_points: vec![vec![GwasPoint {
            pos: 25,
            neg_log10_p: 8.4,
            p_raw: 4e-9,
            snp_id: "rsHit".into(),
        }]],
        ..AppState::default()
    };

    let _ = state.apply(Action::CommandSubmit("sort mapq".into()));
    assert_eq!(state.alignment_opts.sort_mode, AlignmentSortMode::MappingQuality);

    let _ = state.apply(Action::CommandSubmit("group rg".into()));
    assert_eq!(state.alignment_opts.group_mode, AlignmentGroupMode::ReadGroup);

    let _ = state.apply(Action::CommandSubmit("color mod".into()));
    assert_eq!(state.alignment_opts.color_mode, AlignmentColorMode::BaseModification);

    let _ = state.apply(Action::CommandSubmit("pairs on".into()));
    assert!(state.alignment_opts.view_as_pairs);

    let _ = state.apply(Action::CommandSubmit("motif AAAA".into()));
    assert!(!state.sequence_opts.motif_hits.is_empty());

    let _ = state.apply(Action::CommandSubmit("split chr2:10-40".into()));
    assert_eq!(state.panes.len(), 2);
    assert_eq!(state.active_pane, 1);

    let _ = state.apply(Action::NextPane);
    assert_eq!(state.active_pane, 0);

    let _ = state.apply(Action::CommandSubmit("unsplit".into()));
    assert_eq!(state.panes.len(), 1);

    // Populate GWAS data in the current window and test mouse click/drag hit-testing
    state.gwas_points = vec![vec![GwasPoint {
        pos: 25,
        neg_log10_p: 8.4,
        p_raw: 4e-9,
        snp_id: "rsHit".into(),
    }]];
    let body = Rect::new(0, 3, 80, 40);
    let areas = compute(
        Rect::new(0, 0, 80, 45),
        &LayoutSpec {
            gwas_count: 1,
            ..Default::default()
        },
    );
    let hit = FrameHitMap {
        panes: vec![(body, areas.clone())],
    };
    let dragged = handle_drag(&hit, &state, 10, 40, areas.ruler.y);
    assert!(dragged.is_some());
    let _ = handle_click(&hit, &mut state, 40, areas.gwas[0].y + 1);
    assert!(state.inspector.is_some());

    // Render all new TUI widgets into TestBackend
    let panes = split_panes(body, 2);
    assert_eq!(panes.len(), 2);

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let theme = Theme::dark();
    let region = Region::new("chr1", 1, 100).unwrap();
    let popup = InspectorPopup {
        title: "Test".into(),
        rows: vec![("Key".into(), "Val".into())],
    };
    terminal
        .draw(|f| {
            let a = f.area();
            f.render_widget(
                SashimiWidget {
                    display_name: "rna",
                    junctions: &[SpliceJunction {
                        start: 10,
                        end: 60,
                        count: 12,
                        strand: Strand::Forward,
                    }],
                    region: &region,
                    theme: &theme,
                },
                Rect::new(0, 0, 80, 5),
            );
            f.render_widget(
                SegWidget {
                    display_name: "cnv",
                    samples: &["S1".into(), "S2".into()],
                    segments: &[SegInterval {
                        sample: "S1".into(),
                        chrom: "chr1".into(),
                        start: 10,
                        end: 80,
                        num_marks: Some(4),
                        value: 1.2,
                    }],
                    region: &region,
                    theme: &theme,
                },
                Rect::new(0, 5, 80, 4),
            );
            f.render_widget(
                GwasWidget {
                    display_name: "gwas",
                    points: &[GwasPoint {
                        pos: 40,
                        neg_log10_p: 8.0,
                        p_raw: 1e-8,
                        snp_id: "rs1".into(),
                    }],
                    region: &region,
                    theme: &theme,
                },
                Rect::new(0, 9, 80, 6),
            );
            f.render_widget(
                InspectorWidget {
                    popup: &popup,
                    theme: &theme,
                },
                a,
            );
        })
        .unwrap();
}
