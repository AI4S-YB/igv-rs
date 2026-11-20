use std::io::{self, Write as _};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context};
use clap::Parser;
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::Terminal;
use tokio::sync::mpsc;
use tracing::{error, info};

use igv_core::alignment::{AlignmentColorMode, AlignmentGroupMode, AlignmentSortMode};
use igv_core::pedigree::Pedigree;
use igv_core::region::Region;
use igv_core::render::Thresholds;
use igv_core::render_inputs::RenderInputs;
use igv_core::source::{
    AnnotationFormat, FetchOpts, FetchSignalOpts, FastaSource, LinkFormat, SignalFormat,
    SignalSummary,
};
use igv_core::{CollectOpts, Sources};
use igv_render::{render_terminal_escape, SvgOptions, TerminalGraphicsProtocol};
use igv_serve::BrowserCommand;

use igv_rs::app::action::Action;
use igv_rs::app::commands::{execute_command, run_blat_query};
use igv_rs::app::loader::{LoadResult, Loader};
use igv_rs::app::open::{open_genome_spec, open_track_spec, spec_to_pathbuf, OpenedTrack};
use igv_rs::app::serve::ServeController;
use igv_rs::app::state::{
    signal_bins_for_width, AnnotationTrack, AppState, AsyncCommand, BamTrack, GwasTrack, LinkTrack,
    SegTrack, SignalTrack, StatusKind, VcfTrack,
};
use igv_rs::batch::run_batch_script;
use igv_rs::cli;
use igv_rs::command::CommandPalette;
use igv_rs::input::InputState;
use igv_rs::logging;
use igv_rs::session::SessionFile;
use igv_rs::snapshot::batch::{run_sources, BatchOpts};
use igv_rs::snapshot::writer::{inputs_from_state, write_circular_snapshot, write_snapshot};
use igv_rs::socket::spawn_socket_server;
use igv_rs::ui::hit::{handle_click, handle_drag, FrameHitMap};
use igv_rs::ui::layout::{compute_pane_body, split_panes, LayoutAreas, LayoutSpec};
use igv_rs::ui::theme;
use igv_rs::ui::widgets;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = cli::Cli::parse();
    let _log_guard = logging::setup(&args.log_level)?;
    info!(?args, "igv-rs starting");

    if let Some(batch_script) = args.batch.as_deref() {
        return run_batch_script(
            batch_script,
            args.auth_token.as_deref(),
            args.snapshot_width,
        )
        .await;
    }

    if args.snapshot_bed.is_some() && args.snapshot_genes.is_some() {
        return Err(anyhow!(
            "--snapshot-bed and --snapshot-genes are mutually exclusive"
        ));
    }
    if (args.snapshot_bed.is_some() || args.snapshot_genes.is_some()) && args.snapshot_out.is_none()
    {
        return Err(anyhow!("--snapshot-out is required with batch flags"));
    }

    let session = match args.session.as_deref() {
        Some(p) => Some(SessionFile::load(p)?),
        None => None,
    };

    let auth = args.auth_token.as_deref();
    let mut loaded_hubs = Vec::new();
    for hspec in &args.hub {
        loaded_hubs.push(igv_core::hub::parse_hub(hspec, auth).await?);
    }

    let genome_spec = args
        .genome
        .clone()
        .or_else(|| args.fasta.clone())
        .or_else(|| session.as_ref().and_then(|s| s.genome.clone().or(s.fasta.clone())))
        .or_else(|| loaded_hubs.iter().find_map(|h| h.two_bit_url.clone()))
        .ok_or_else(|| {
            anyhow!(
                "missing reference: provide positional <FASTA/.2bit>, --genome <ID>, --hub, --session, or --batch"
            )
        })?;

    let loaded_genome = open_genome_spec(&genome_spec, auth).await?;
    let fasta: Arc<dyn FastaSource> = loaded_genome.fasta;
    let references = loaded_genome.references;
    let cytobands = loaded_genome.cytobands;
    let chrom_aliases = loaded_genome.aliases;
    let genome_id = Some(loaded_genome.genome_id);

    let (theme_preset, theme) = theme::load_theme(Some(args.light_mode), args.config.as_deref());

    let ped_spec = args
        .ped
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .or_else(|| session.as_ref().and_then(|s| s.pedigree.clone()));
    let pedigree = match ped_spec {
        Some(spec) => Some(Arc::new(Pedigree::open(&spec, auth).await?)),
        None => None,
    };

    let mut state = AppState {
        fasta: Arc::clone(&fasta),
        genome_id,
        cytobands,
        chrom_aliases,
        pedigree: pedigree.clone(),
        auth_token: args.auth_token.clone(),
        references: references.clone(),
        link_min_score: args.link_min_score,
        theme: theme.clone(),
        theme_preset,
        thresholds: Thresholds::default(),
        ..AppState::default()
    };

    state.alignment_opts.color_mode = args
        .color_by
        .as_deref()
        .or_else(|| session.as_ref().and_then(|s| s.color_by.as_deref()))
        .and_then(AlignmentColorMode::parse)
        .unwrap_or(if args.tag.is_some() {
            AlignmentColorMode::Tag
        } else {
            AlignmentColorMode::Strand
        });
    state.bam_tag = args
        .tag
        .clone()
        .or_else(|| session.as_ref().and_then(|s| s.tag.clone()));
    state.alignment_opts.sort_mode = args
        .sort_by
        .as_deref()
        .or_else(|| session.as_ref().and_then(|s| s.sort_by.as_deref()))
        .and_then(AlignmentSortMode::parse)
        .unwrap_or(AlignmentSortMode::None);
    state.alignment_opts.group_mode = args
        .group_by
        .as_deref()
        .or_else(|| session.as_ref().and_then(|s| s.group_by.as_deref()))
        .and_then(AlignmentGroupMode::parse)
        .unwrap_or(AlignmentGroupMode::None);
    state.alignment_opts.view_as_pairs = args.view_as_pairs
        || session
            .as_ref()
            .and_then(|s| s.view_as_pairs)
            .unwrap_or(false);
    state.alignment_opts.show_soft_clips = args.show_soft_clips
        || session
            .as_ref()
            .and_then(|s| s.show_soft_clips)
            .unwrap_or(false);
    let max_depth = session
        .as_ref()
        .and_then(|s| s.max_depth)
        .unwrap_or(args.max_depth);
    state.alignment_opts.max_depth = if max_depth == 0 {
        None
    } else {
        Some(max_depth)
    };
    state.show_sashimi =
        args.sashimi || session.as_ref().and_then(|s| s.show_sashimi).unwrap_or(false);
    state.signal_summary = SignalSummary::parse(&args.signal_summary)
        .or_else(|| {
            session
                .as_ref()
                .and_then(|s| s.signal_summary.as_deref())
                .and_then(SignalSummary::parse)
        })
        .unwrap_or(SignalSummary::Max);
    state.sequence_opts.motif_pattern = session.as_ref().and_then(|s| s.motif.clone());
    state.graphics_protocol = TerminalGraphicsProtocol::parse(&args.graphics)
        .unwrap_or(TerminalGraphicsProtocol::Auto);

    let ann_fmt = args
        .annotation_format
        .as_deref()
        .and_then(AnnotationFormat::parse);
    let sig_fmt = args.signal_format.as_deref().and_then(SignalFormat::parse);
    let link_fmt = args.link_format.as_deref().and_then(LinkFormat::parse);

    // Gather all track specs (CLI + session + hubs)
    let mut all_specs: Vec<(String, Option<&'static str>)> = Vec::new();
    for s in &args.vcf {
        all_specs.push((s.clone(), Some("vcf")));
    }
    for s in &args.bam {
        all_specs.push((s.clone(), Some("bam")));
    }
    for s in &args.annotations {
        all_specs.push((s.clone(), Some("ann")));
    }
    for s in &args.signals {
        all_specs.push((s.clone(), Some("sig")));
    }
    for s in &args.links {
        all_specs.push((s.clone(), Some("link")));
    }
    for s in &args.seg {
        all_specs.push((s.clone(), Some("seg")));
    }
    for s in &args.gwas {
        all_specs.push((s.clone(), Some("gwas")));
    }
    if let Some(sess) = &session {
        for s in &sess.vcfs {
            all_specs.push((s.clone(), Some("vcf")));
        }
        for s in &sess.bams {
            all_specs.push((s.clone(), Some("bam")));
        }
        for s in &sess.annotations {
            all_specs.push((s.clone(), Some("ann")));
        }
        for s in &sess.signals {
            all_specs.push((s.clone(), Some("sig")));
        }
        for s in &sess.links {
            all_specs.push((s.clone(), Some("link")));
        }
        for s in &sess.segs {
            all_specs.push((s.clone(), Some("seg")));
        }
        for s in &sess.gwas {
            all_specs.push((s.clone(), Some("gwas")));
        }
    }
    for hub in &loaded_hubs {
        for ht in &hub.tracks {
            all_specs.push((ht.data_url.clone(), None));
        }
    }

    for (spec, hint) in all_specs {
        let opened = match hint {
            Some("vcf") => OpenedTrack::Variant(
                igv_core::io::Location::parse(&spec).display_name(),
                igv_core::source::open_variant(&spec, state.pedigree.clone(), auth).await?,
            ),
            Some("bam") => OpenedTrack::Alignment(
                igv_core::io::Location::parse(&spec).display_name(),
                igv_core::source::open_alignment(
                    &spec,
                    Arc::clone(&state.fasta),
                    state.bam_tag.as_deref(),
                    auth,
                )
                .await?,
            ),
            Some("ann") => {
                let loc = igv_core::io::Location::parse(&spec);
                OpenedTrack::Annotation(
                    loc.display_name(),
                    igv_core::source::open_annotation_location(&loc, ann_fmt, auth).await?,
                )
            }
            Some("sig") => {
                let loc = igv_core::io::Location::parse(&spec);
                OpenedTrack::Signal(
                    loc.display_name(),
                    igv_core::source::open_signal_location(&loc, sig_fmt, auth).await?,
                )
            }
            Some("link") => {
                let loc = igv_core::io::Location::parse(&spec);
                OpenedTrack::Link(
                    loc.display_name(),
                    igv_core::source::open_link_location(&loc, link_fmt, auth).await?,
                )
            }
            _ => {
                open_track_spec(
                    &spec,
                    &state.fasta,
                    state.bam_tag.as_deref(),
                    state.pedigree.clone(),
                    ann_fmt,
                    sig_fmt,
                    link_fmt,
                    auth,
                )
                .await?
            }
        };
        attach_opened_track(&mut state, &spec, opened);
    }
    state.ensure_track_vecs();

    // Headless batch flags
    if args.snapshot_genes.is_some() || args.snapshot_bed.is_some() {
        let regions = if let Some(genes_path) = args.snapshot_genes.as_deref() {
            if state.annotations.is_empty() {
                return Err(anyhow!(
                    "--snapshot-genes requires at least one -g/--annotation"
                ));
            }
            let names = igv_rs::snapshot::genes::read_names(genes_path)?;
            let srcs: Vec<_> = state
                .annotations
                .iter()
                .map(|t| Arc::clone(&t.source))
                .collect();
            let resolved = igv_rs::snapshot::genes::resolve(&names, &srcs);
            if resolved.is_empty() {
                return Err(anyhow!("--snapshot-genes: no genes resolved"));
            }
            resolved
        } else {
            igv_rs::snapshot::regions::parse_bed(args.snapshot_bed.as_deref().unwrap())?
        };
        let format = igv_rs::snapshot::batch::parse_format(&args.snapshot_format)?;
        let stheme = igv_rs::snapshot::batch::parse_theme(&args.snapshot_theme)?;
        let batch = BatchOpts {
            out_dir: args.snapshot_out.clone().unwrap(),
            format,
            width_px: args.snapshot_width,
            flank: args.snapshot_flank,
            theme: stheme,
        };
        let sources = state_to_sources(&state);
        let copts = CollectOpts {
            signal_opts: FetchSignalOpts {
                max_bins: args.snapshot_width,
                summary: state.signal_summary,
            },
            alignment_opts: state.alignment_opts.clone(),
            sequence_opts: state.sequence_opts.clone(),
            ..CollectOpts::default()
        };
        return run_sources(sources, copts, regions, batch, state.show_sashimi).await;
    }

    // Resolve initial regions / multi-locus split panes
    let mut region_specs = args.region.clone();
    if region_specs.is_empty() {
        if let Some(sess) = &session {
            region_specs = sess.regions.clone();
        }
    }
    let mut panes = Vec::new();
    for s in region_specs.into_iter().take(4) {
        let mut r = Region::parse(&s).with_context(|| format!("invalid -r region: {s}"))?;
        let avail: Vec<String> = state.references.iter().map(|m| m.name.clone()).collect();
        if let Some(canon) = state.chrom_aliases.resolve(&r.chrom, &avail) {
            r.chrom = canon.to_string();
        }
        panes.push(r);
    }
    if panes.is_empty() {
        let chr = state
            .references
            .first()
            .ok_or_else(|| anyhow!("Reference contains no chromosomes"))?
            .name
            .clone();
        let len = state.references.first().map(|r| r.length).unwrap_or(1000);
        panes.push(Region::new(
            chr,
            1,
            len.clamp(1, igv_core::region::DEFAULT_REGION_WIDTH),
        )?);
    }
    state.region = panes[0].clone();
    state.panes = panes;
    state.active_pane = 0;
    state.ensure_track_vecs();

    // Headless circular export if requested without interactive TUI
    if let Some(circ_path) = args.circular_out.as_deref() {
        // Pre-fetch first region segments so CNV inner ring is populated if segs exist
        for (i, st) in state.segs.iter().enumerate() {
            if let Ok(d) = st.source.fetch(&state.region).await {
                if let Some(slot) = state.seg_data.get_mut(i) {
                    *slot = d;
                }
            }
        }
        write_circular_snapshot(&state, circ_path)?;
        eprintln!("circular view → {}", circ_path.display());
        if args.port.is_none() {
            return Ok(());
        }
    }

    if let Ok((cols, _)) = crossterm::terminal::size() {
        state.terminal_width = cols;
    }

    let (tx, mut rx) = mpsc::channel::<LoadResult>(64);
    let mut loader = build_loader_from_state(&state, tx.clone());
    if let Some(req) = state.apply(Action::Goto(state.region.clone())) {
        loader.dispatch(req);
    }

    // Optional live TCP socket server (--port)
    let mut socket_rx = if let Some(port) = args.port {
        let (addr, srx, _jh) = spawn_socket_server(port).await?;
        info!(%addr, "igv-rs socket listener started");
        state.set_status(StatusKind::Info, format!("socket listening on {addr}"));
        Some(srx)
    } else {
        None
    };

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut input_state = InputState::default();
    let mut palette = CommandPalette::default();
    let mut events = EventStream::new();

    let serve_cfg = igv_rs::ui::theme::load_serve_config(args.config.as_deref());
    let auto_open = !args.no_browser && serve_cfg.auto_open;
    let serve_port = if args.serve_port != 0 {
        args.serve_port
    } else {
        serve_cfg.port
    };
    let fasta_path = spec_to_pathbuf(&genome_spec);
    let vcf_path = state.vcf_tracks.first().map(|t| t.path.clone());
    let mut serve_controller = ServeController::new(auto_open, serve_port, fasta_path, vcf_path);

    let result = run_loop(
        &mut terminal,
        &mut state,
        &mut loader,
        &tx,
        &mut rx,
        &mut events,
        &mut input_state,
        &mut palette,
        &mut serve_controller,
        &mut socket_rx,
        &genome_spec,
    )
    .await;

    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .ok();
    terminal.show_cursor().ok();

    serve_controller.shutdown().await;

    if let Err(e) = result {
        error!("fatal: {e}");
        eprintln!("igv-rs exited with error: {e}");
        return Err(e);
    }
    Ok(())
}

fn attach_opened_track(state: &mut AppState, spec: &str, opened: OpenedTrack) {
    let path = spec_to_pathbuf(spec);
    match opened {
        OpenedTrack::Alignment(display, source) => {
            state.bams.push(BamTrack {
                path,
                display,
                source,
                fetch_opts: FetchOpts::default(),
            });
        }
        OpenedTrack::Variant(display, source) => {
            if state.vcf.is_none() {
                state.vcf = Some(Arc::clone(&source));
            }
            state.vcf_tracks.push(VcfTrack {
                path,
                display,
                source,
            });
        }
        OpenedTrack::Annotation(display, source) => {
            state.annotations.push(AnnotationTrack {
                path,
                display,
                source,
            });
        }
        OpenedTrack::Signal(display, source) => {
            state.signals.push(SignalTrack {
                path,
                display,
                source,
            });
        }
        OpenedTrack::Link(display, source) => {
            state.links.push(LinkTrack {
                path,
                display,
                source,
            });
        }
        OpenedTrack::Seg(display, source) => {
            state.segs.push(SegTrack {
                path,
                display,
                source,
            });
        }
        OpenedTrack::Gwas(display, source) => {
            state.gwas.push(GwasTrack {
                path,
                display,
                source,
            });
        }
        OpenedTrack::Pedigree(p) => state.pedigree = Some(p),
        OpenedTrack::Hub(_) => {}
    }
}

fn build_loader_from_state(state: &AppState, tx: mpsc::Sender<LoadResult>) -> Loader {
    let bams = state.bams.iter().map(|t| Arc::clone(&t.source)).collect();
    let vcfs = state
        .vcf_tracks
        .iter()
        .map(|t| Arc::clone(&t.source))
        .collect();
    let anns = state
        .annotations
        .iter()
        .map(|t| Arc::clone(&t.source))
        .collect();
    let sigs = state.signals.iter().map(|t| Arc::clone(&t.source)).collect();
    let links = state.links.iter().map(|t| Arc::clone(&t.source)).collect();
    let segs = state.segs.iter().map(|t| Arc::clone(&t.source)).collect();
    let gwas = state.gwas.iter().map(|t| Arc::clone(&t.source)).collect();
    let mut loader = Loader::new(
        Arc::clone(&state.fasta),
        state.vcf.clone(),
        bams,
        anns,
        sigs,
        links,
        tx,
    );
    loader.set_extra_tracks(vcfs, segs, gwas, state.signal_summary);
    loader
}

fn state_to_sources(state: &AppState) -> Sources {
    Sources {
        fasta: Arc::clone(&state.fasta),
        vcf: state.vcf.clone(),
        vcfs: state
            .vcf_tracks
            .iter()
            .map(|t| (t.display.clone(), Arc::clone(&t.source)))
            .collect(),
        bams: state
            .bams
            .iter()
            .map(|t| (t.display.clone(), Arc::clone(&t.source)))
            .collect(),
        annotations: state
            .annotations
            .iter()
            .map(|t| (t.display.clone(), Arc::clone(&t.source)))
            .collect(),
        signals: state
            .signals
            .iter()
            .map(|t| (t.display.clone(), Arc::clone(&t.source)))
            .collect(),
        links: state
            .links
            .iter()
            .map(|t| (t.display.clone(), Arc::clone(&t.source)))
            .collect(),
        segs: state
            .segs
            .iter()
            .map(|t| (t.display.clone(), Arc::clone(&t.source)))
            .collect(),
        gwas: state
            .gwas
            .iter()
            .map(|t| (t.display.clone(), Arc::clone(&t.source)))
            .collect(),
        references: state.references.clone(),
        cytobands: state.cytobands.clone(),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    loader: &mut Loader,
    tx: &mpsc::Sender<LoadResult>,
    rx: &mut mpsc::Receiver<LoadResult>,
    events: &mut EventStream,
    input_state: &mut InputState,
    palette: &mut CommandPalette,
    serve_controller: &mut ServeController,
    socket_rx: &mut Option<mpsc::UnboundedReceiver<igv_rs::socket::SocketCommand>>,
    genome_spec: &str,
) -> anyhow::Result<()> {
    let mut current_genome_spec = genome_spec.to_string();

    while !state.should_quit {
        let mut frame_hit = FrameHitMap::default();
        terminal.draw(|f| frame_hit = draw(f, state))?;
        let last_hit = frame_hit;

        if state.graphics_overlay && !state.loading {
            if let Ok(Some(esc)) = render_terminal_escape(
                &inputs_from_state(state),
                &SvgOptions::default(),
                state.graphics_protocol,
                state.terminal_width.max(80),
                24,
            ) {
                let mut out = io::stdout();
                let _ = out.write_all(esc.as_bytes());
                let _ = out.flush();
            }
            state.graphics_overlay = false;
        }

        // Poll incoming browser locus updates if serve is running
        while let Some(BrowserCommand::Goto(locus)) = serve_controller.try_recv_command() {
            if let Ok(r) = Region::parse(&locus) {
                if let Some(req) = state.apply(Action::Goto(r)) {
                    loader.dispatch(req);
                }
            }
        }

        let sock_fut = async {
            match socket_rx.as_mut() {
                Some(rx) => rx.recv().await,
                None => std::future::pending().await,
            }
        };

        tokio::select! {
            maybe_evt = events.next() => {
                if let Some(Ok(evt)) = maybe_evt {
                    if let Event::Resize(w, _) = evt {
                        let prev = signal_bins_for_width(state.terminal_width);
                        let next = signal_bins_for_width(w);
                        state.terminal_width = w;
                        if !state.signals.is_empty() && next != prev {
                            if let Some(req) = state.apply(Action::Goto(state.region.clone())) {
                                loader.dispatch(req);
                            }
                        }
                        continue;
                    }
                    let action = if state.command_open {
                        let act = palette.handle(&evt);
                        state.command_buffer = palette.input.value().to_string();
                        act
                    } else if matches!(&evt, Event::Key(k) if k.kind != KeyEventKind::Press) {
                        Action::None
                    } else {
                        let act = input_state.map_full(
                            &evt,
                            false,
                            state.help_open,
                            state.inspector.is_some(),
                        );
                        if matches!(act, Action::OpenCommand) {
                            palette.open();
                        }
                        act
                    };

                    match action {
                        Action::OpenBrowser => match serve_controller.open(state).await {
                            Ok(url) => state.set_status(StatusKind::Info, format!("browser → {url}")),
                            Err(e) => state.set_status(StatusKind::Error, format!("serve failed: {e}")),
                        },
                        Action::MouseClick { x, y } => {
                            if let Some(r) = handle_click(&last_hit, state, x, y) {
                                if let Some(req) = state.apply(Action::Goto(r)) {
                                    loader.dispatch(req);
                                }
                            }
                        }
                        Action::MouseDrag { start_x, end_x, y } => {
                            if let Some(r) = handle_drag(&last_hit, state, start_x, end_x, y) {
                                if let Some(req) = state.apply(Action::Goto(r)) {
                                    loader.dispatch(req);
                                }
                            }
                        }
                        other => {
                            if let Some(req) = state.apply(other) {
                                loader.dispatch(req);
                            }
                        }
                    }
                }
            }
            maybe_result = rx.recv() => {
                if let Some(result) = maybe_result {
                    let result_gen = result.generation();
                    apply_load_result(state, result);
                    if result_gen == state.generation {
                        state.loaded_count = state.loaded_count.saturating_add(1);
                        if state.loaded_count >= state.expected_loads() {
                            state.loading = false;
                            serve_controller.notify_view(state);
                        }
                    }
                }
            }
            maybe_sock = sock_fut => {
                if let Some(scmd) = maybe_sock {
                    let reply = handle_socket_line(state, loader, &scmd.line);
                    let _ = scmd.reply.send(reply);
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(120)) => {
                let stale = state
                    .status
                    .as_ref()
                    .is_some_and(|s| s.set_at.elapsed() > Duration::from_secs(3));
                if stale {
                    state.status = None;
                }
            }
        }

        // Execute queued async commands (:load, :genome, :save-session, :load-session, :blat)
        if let Some(acmd) = state.pending_async.take() {
            handle_async_command(state, loader, tx, &mut current_genome_spec, acmd).await;
        }
        drain_snapshot(state);
        drain_circular(state);
    }
    Ok(())
}

fn handle_socket_line(state: &mut AppState, loader: &mut Loader, line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.eq_ignore_ascii_case("getLocus") {
        return state.region.to_string();
    }
    let normalized = trimmed
        .strip_prefix("goto ")
        .unwrap_or(trimmed);
    if let Some(req) = execute_command(state, normalized) {
        loader.dispatch(req);
    }
    "OK".to_string()
}

async fn handle_async_command(
    state: &mut AppState,
    loader: &mut Loader,
    tx: &mpsc::Sender<LoadResult>,
    genome_spec: &mut String,
    acmd: AsyncCommand,
) {
    let auth = state.auth_token.clone();
    let auth_ref = auth.as_deref();
    match acmd {
        AsyncCommand::LoadTrack(spec) => {
            match open_track_spec(
                &spec,
                &state.fasta,
                state.bam_tag.as_deref(),
                state.pedigree.clone(),
                None,
                None,
                None,
                auth_ref,
            )
            .await
            {
                Ok(opened) => {
                    attach_opened_track(state, &spec, opened);
                    state.ensure_track_vecs();
                    *loader = build_loader_from_state(state, tx.clone());
                    let r = state.region.clone();
                    if let Some(req) = state.set_region_pending(r) {
                        loader.dispatch(req);
                    }
                    state.set_status(StatusKind::Info, format!("loaded {spec}"));
                }
                Err(e) => state.set_status(StatusKind::Error, format!("load failed: {e}")),
            }
        }
        AsyncCommand::LoadGenome(spec) => match open_genome_spec(&spec, auth_ref).await {
            Ok(loaded) => {
                *genome_spec = spec.clone();
                state.fasta = loaded.fasta;
                state.references = loaded.references;
                state.cytobands = loaded.cytobands;
                state.chrom_aliases = loaded.aliases;
                state.genome_id = Some(loaded.genome_id);
                *loader = build_loader_from_state(state, tx.clone());
                if let Some(first) = state.references.first() {
                    if let Ok(r) = Region::new(first.name.clone(), 1, first.length.clamp(1, 1000)) {
                        if let Some(req) = state.set_region_pending(r) {
                            loader.dispatch(req);
                        }
                    }
                }
                state.set_status(StatusKind::Info, format!("genome → {spec}"));
            }
            Err(e) => state.set_status(StatusKind::Error, format!("genome failed: {e}")),
        },
        AsyncCommand::SaveSession(path) => {
            let sess = SessionFile::from_state(state, Some(genome_spec.clone()));
            match sess.save_toml(&path) {
                Ok(()) => state.set_status(StatusKind::Info, format!("session → {}", path.display())),
                Err(e) => state.set_status(StatusKind::Error, format!("save-session: {e}")),
            }
        }
        AsyncCommand::LoadSession(path) => match SessionFile::load(&path) {
            Ok(sess) => {
                state.set_status(StatusKind::Info, format!("session loaded: {}", path.display()));
                if let Some(r_s) = sess.regions.first() {
                    if let Ok(r) = Region::parse(r_s) {
                        if let Some(req) = state.set_region_pending(r) {
                            loader.dispatch(req);
                        }
                    }
                }
            }
            Err(e) => state.set_status(StatusKind::Error, format!("load-session: {e}")),
        },
        AsyncCommand::BlatQuery(seq) => {
            let db = state.genome_id.as_deref().unwrap_or("hg38");
            match run_blat_query(db, &seq).await {
                Ok((top, popup)) => {
                    state.inspector = Some(popup);
                    if let Some(r) = top {
                        if let Some(req) = state.set_region_pending(r) {
                            loader.dispatch(req);
                        }
                    }
                }
                Err(e) => state.set_status(StatusKind::Error, format!("BLAT failed: {e}")),
            }
        }
    }
}

fn drain_snapshot(state: &mut AppState) {
    let Some(job) = state.pending_snapshot.take() else {
        return;
    };
    let path = job
        .path
        .clone()
        .unwrap_or_else(|| igv_rs::snapshot::naming::auto_name(&state.region, job.format));
    match write_snapshot(state, &path, job.format) {
        Ok(()) => state.set_status(StatusKind::Info, format!("snapshot → {}", path.display())),
        Err(e) => state.set_status(StatusKind::Error, format!("snapshot failed: {}", e)),
    }
}

fn drain_circular(state: &mut AppState) {
    let Some(path) = state.pending_circular.take() else {
        return;
    };
    match write_circular_snapshot(state, &path) {
        Ok(()) => state.set_status(StatusKind::Info, format!("circular → {}", path.display())),
        Err(e) => state.set_status(StatusKind::Error, format!("circular failed: {}", e)),
    }
}

fn apply_load_result(state: &mut AppState, result: LoadResult) {
    match result {
        LoadResult::Reference {
            generation,
            region,
            bytes,
        } => {
            if generation == state.generation && region.chrom == state.region.chrom {
                state.reference_seq = bytes;
                state.recompute_motif_hits();
                if state.alignment_opts.sort_mode == AlignmentSortMode::Base {
                    state.recompute_all_bams();
                }
            }
        }
        LoadResult::Variants {
            generation,
            records,
        } => {
            if generation == state.generation {
                state.variants = records;
            }
        }
        LoadResult::VcfTrack {
            generation,
            track_index,
            records,
        } => {
            if generation == state.generation {
                if let Some(slot) = state.vcf_track_rows.get_mut(track_index) {
                    *slot = records;
                }
            }
        }
        LoadResult::Bam {
            generation,
            bam_index,
            rows,
        } => {
            if generation == state.generation {
                if let Some(slot) = state.bam_raw_rows.get_mut(bam_index) {
                    *slot = rows;
                }
                state.recompute_bam_layout(bam_index);
            }
        }
        LoadResult::Annotation {
            generation,
            track_index,
            transcripts,
        } => {
            if generation == state.generation {
                if let Some(slot) = state.annotation_rows.get_mut(track_index) {
                    *slot = transcripts;
                }
            }
        }
        LoadResult::Signal {
            generation,
            track_index,
            bins,
        } => {
            if generation == state.generation {
                if let Some(slot) = state.signal_bins.get_mut(track_index) {
                    *slot = bins;
                }
            }
        }
        LoadResult::Link {
            generation,
            track_index,
            visible,
            total_record_count: _,
        } => {
            if generation == state.generation {
                if let Some(slot) = state.link_records.get_mut(track_index) {
                    *slot = visible;
                }
            }
        }
        LoadResult::Seg {
            generation,
            track_index,
            data,
        } => {
            if generation == state.generation {
                if let Some(slot) = state.seg_data.get_mut(track_index) {
                    *slot = data;
                }
            }
        }
        LoadResult::Gwas {
            generation,
            track_index,
            points,
        } => {
            if generation == state.generation {
                if let Some(slot) = state.gwas_points.get_mut(track_index) {
                    *slot = points;
                }
            }
        }
        LoadResult::Error {
            generation,
            message,
        } => {
            if generation == state.generation {
                state.set_status(StatusKind::Error, message);
            }
        }
    }
}

fn draw(f: &mut ratatui::Frame<'_>, state: &AppState) -> FrameHitMap {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(f.area());
    let header_area = outer[0];
    let body_area = outer[1];
    let footer_area = outer[2];

    f.render_widget(
        widgets::header::HeaderWidget {
            state,
            theme: &state.theme,
        },
        header_area,
    );

    let seq_h = 1
        + if state.sequence_opts.show_complement { 1 } else { 0 }
        + if state.sequence_opts.show_translation { 3 } else { 0 };
    let vcf_heights: Vec<u16> = state
        .vcf_tracks
        .iter()
        .map(|t| (3 + t.source.sample_names().len().min(6) as u16).max(3))
        .collect();
    let sashimi_count = state
        .bam_junctions
        .iter()
        .filter(|j| !j.is_empty())
        .count();

    let spec = LayoutSpec {
        has_vcf: state.vcf.is_some() && vcf_heights.is_empty(),
        vcf_heights,
        bam_count: state.bams.len(),
        coverage_height: state.coverage_height,
        show_sashimi: state.show_sashimi,
        sashimi_count,
        sashimi_height: 5,
        alignments_min_per_track: state.alignment_height,
        annotation_tracks: state.annotations.len(),
        link_count: state.links.len(),
        link_height_per_track: state.link_track_height,
        signal_count: state.signals.len(),
        signal_height_per_track: state.signal_track_height,
        seg_count: state.segs.len(),
        seg_height_per_track: 6,
        gwas_count: state.gwas.len(),
        gwas_height_per_track: 6,
        sequence_height: seq_h.max(2),
        ..Default::default()
    };

    let pane_rects = split_panes(body_area, state.panes.len());
    let mut hit = FrameHitMap::default();
    for (pane_idx, &pane_rect) in pane_rects.iter().enumerate() {
        let areas = compute_pane_body(pane_rect, &spec);
        if pane_idx == state.active_pane {
            render_active_pane(f, state, &areas);
        } else {
            let cached = state
                .pane_snapshots
                .get(pane_idx)
                .and_then(|o| o.as_ref());
            let r = &state.panes[pane_idx];
            render_inactive_pane(f, state, r, cached, pane_rect);
        }
        hit.panes.push((pane_rect, areas));
    }

    f.render_widget(
        widgets::footer::FooterWidget {
            state,
            theme: &state.theme,
        },
        footer_area,
    );

    if let Some(popup) = &state.inspector {
        f.render_widget(
            widgets::inspector::InspectorWidget {
                popup,
                theme: &state.theme,
            },
            f.area(),
        );
    }
    if state.help_open {
        f.render_widget(
            widgets::help::HelpWidget {
                theme: &state.theme,
            },
            f.area(),
        );
    }

    hit
}

fn render_active_pane(f: &mut ratatui::Frame<'_>, state: &AppState, areas: &LayoutAreas) {
    f.render_widget(
        widgets::overview::OverviewWidget {
            state,
            theme: &state.theme,
        },
        areas.overview,
    );
    f.render_widget(
        widgets::ruler::RulerWidget {
            state,
            theme: &state.theme,
        },
        areas.ruler,
    );
    f.render_widget(
        widgets::sequence::SequenceWidget {
            state,
            theme: &state.theme,
        },
        areas.sequence,
    );
    for (i, area) in areas.annotations.iter().enumerate() {
        f.render_widget(
            widgets::annotations::AnnotationsWidget {
                state,
                theme: &state.theme,
                track_index: i,
            },
            *area,
        );
    }
    for (i, area) in areas.links.iter().enumerate() {
        let track = &state.links[i];
        let visible = state
            .link_records
            .get(i)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        f.render_widget(
            widgets::link::LinkWidget {
                display_name: &track.display,
                region: &state.region,
                theme: &state.theme,
                visible,
                total_record_count: track.source.record_count(),
                height_rows: state.link_track_height,
            },
            *area,
        );
    }
    let mode = state.render_mode();
    for (i, area) in areas.vcf_tracks.iter().enumerate() {
        let vt = &state.vcf_tracks[i];
        let recs = state
            .vcf_track_rows
            .get(i)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        f.render_widget(
            widgets::variants::VcfTrackWidget {
                display_name: &vt.display,
                sample_names: &vt.source.sample_names(),
                records: recs,
                region: &state.region,
                mode,
                theme: &state.theme,
            },
            *area,
        );
    }
    if let Some(va) = areas.variants {
        f.render_widget(
            widgets::variants::VariantsWidget {
                state,
                theme: &state.theme,
            },
            va,
        );
    }
    if let Some(ca) = areas.coverage {
        f.render_widget(
            widgets::coverage::CoverageWidget {
                state,
                theme: &state.theme,
            },
            ca,
        );
    }
    let sashimi_pairs: Vec<_> = state
        .bams
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            let j = state.bam_junctions.get(i)?;
            if j.is_empty() {
                None
            } else {
                Some((&b.display, j.as_slice()))
            }
        })
        .collect();
    for (area, (name, juncs)) in areas.sashimi.iter().zip(sashimi_pairs) {
        f.render_widget(
            widgets::sashimi::SashimiWidget {
                display_name: name,
                junctions: juncs,
                region: &state.region,
                theme: &state.theme,
            },
            *area,
        );
    }
    let global_signal_max = if state.signal_shared_scale {
        state
            .signal_bins
            .iter()
            .flatten()
            .map(|b| b.value)
            .fold(0.0_f32, f32::max)
    } else {
        0.0
    };
    for (i, area) in areas.signals.iter().enumerate() {
        let track = &state.signals[i];
        let bins = state.signal_bins.get(i).map(Vec::as_slice).unwrap_or(&[]);
        f.render_widget(
            widgets::signal::SignalWidget {
                display_name: &track.display,
                bins,
                region: &state.region,
                theme: &state.theme,
                shared_max: if state.signal_shared_scale {
                    Some(global_signal_max)
                } else {
                    None
                },
            },
            *area,
        );
    }
    for (i, area) in areas.segs.iter().enumerate() {
        let st = &state.segs[i];
        let data = state.seg_data.get(i);
        let samples = data
            .map(|d| d.samples.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| st.source.samples());
        let segments = data.map(|d| d.segments.as_slice()).unwrap_or(&[]);
        f.render_widget(
            widgets::seg::SegWidget {
                display_name: &st.display,
                samples: &samples,
                segments,
                region: &state.region,
                theme: &state.theme,
            },
            *area,
        );
    }
    for (i, area) in areas.gwas.iter().enumerate() {
        let gt = &state.gwas[i];
        let pts = state.gwas_points.get(i).map(Vec::as_slice).unwrap_or(&[]);
        f.render_widget(
            widgets::gwas::GwasWidget {
                display_name: &gt.display,
                points: pts,
                region: &state.region,
                theme: &state.theme,
            },
            *area,
        );
    }
    for (i, area) in areas.alignments.iter().enumerate() {
        f.render_widget(
            widgets::alignments::AlignmentsWidget {
                state,
                theme: &state.theme,
                track_index: i,
            },
            *area,
        );
    }
}

fn render_inactive_pane(
    f: &mut ratatui::Frame<'_>,
    state: &AppState,
    region: &Region,
    cached: Option<&RenderInputs>,
    rect: Rect,
) {
    use ratatui::text::Line;
    use ratatui::widgets::{Block, Borders, Paragraph};
    let mut lines = vec![
        Line::from(format!("Locus: {region}")),
        Line::from(format!("Span : {} bp", region.width())),
        Line::from("Click pane or press Tab to focus"),
    ];
    if let Some(snap) = cached {
        let reads: usize = snap.bams.iter().map(|b| b.rows.len()).sum();
        lines.push(Line::from(format!("Cached reads: {reads}")));
        lines.push(Line::from(format!("Cached variants: {}", snap.variants.len())));
    }
    let blk = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {region} "))
        .style(state.theme.get("BORDER"));
    f.render_widget(Paragraph::new(lines).block(blk), rect);
}
