use std::io;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context};
use clap::Parser;
use crossterm::event::{Event, EventStream, KeyEventKind};

use igv_tui::cli;
use igv_tui::logging;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use tracing::{error, info};

use igv_core::region::Region;
use igv_core::render::Thresholds;
use igv_core::source::bam::{FetchOpts, NoodlesBamSource};
use igv_core::source::fasta::NoodlesFastaSource;
use igv_core::source::vcf::NoodlesVcfSource;
use igv_core::source::{open_signal, SignalFormat};

use igv_tui::app::action::Action;
use igv_tui::app::loader::{LoadResult, Loader};
use igv_tui::app::state::{
    AppState, BamTrack, SignalTrack, StatusKind,
    ALIGNMENT_DEFAULT_HEIGHT, COVERAGE_DEFAULT_HEIGHT, SIGNAL_DEFAULT_HEIGHT,
};
use igv_tui::command::CommandPalette;
use igv_tui::input::InputState;
use igv_tui::ui::layout::{compute, LayoutSpec};
use igv_tui::ui::theme;
use igv_tui::ui::widgets;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = cli::Cli::parse();
    let _log_guard = logging::setup(&args.log_level)?;
    info!(?args, "igv-rs starting");

    let theme = theme::load_theme(Some(args.light_mode), args.config.as_deref());

    // Explicit `dyn` types are required because `Vec<Arc<T>>` and
    // `Option<Arc<T>>` are invariant — the unsized coercion to `Arc<dyn ...>`
    // only happens for plain `Arc<T>` at function-call boundaries, not when
    // pushed into a Vec or wrapped in Option.
    let fasta: Arc<dyn igv_core::source::FastaSource> =
        Arc::new(NoodlesFastaSource::open(&args.fasta).await?);
    let references = fasta.references().await?;
    let vcf: Option<Arc<dyn igv_core::source::VcfSource>> = match args.vcf.as_deref() {
        Some(p) => Some(Arc::new(NoodlesVcfSource::open(p).await?)),
        None => None,
    };
    let mut bams: Vec<BamTrack> = Vec::new();
    let mut bam_sources: Vec<Arc<dyn igv_core::source::BamSource>> = Vec::new();
    for path in &args.bam {
        let source: Arc<dyn igv_core::source::BamSource> =
            Arc::new(NoodlesBamSource::open(path, args.tag.as_deref()).await?);
        bams.push(BamTrack {
            path: path.clone(),
            display: path.file_name().and_then(|n| n.to_str()).unwrap_or("bam").into(),
            source: Arc::clone(&source),
            fetch_opts: FetchOpts::default(),
        });
        bam_sources.push(source);
    }

    let mut annotations: Vec<igv_tui::app::state::AnnotationTrack> = Vec::new();
    let mut annotation_sources: Vec<std::sync::Arc<dyn igv_core::source::AnnotationSource>> =
        Vec::new();
    let format_override = args
        .annotation_format
        .as_deref()
        .and_then(igv_core::source::AnnotationFormat::parse);
    for path in &args.annotations {
        let src = igv_core::source::open_annotation(path, format_override).await?;
        annotations.push(igv_tui::app::state::AnnotationTrack {
            path: path.clone(),
            display: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("annotation")
                .to_string(),
            source: std::sync::Arc::clone(&src),
        });
        annotation_sources.push(src);
    }

    let mut signals: Vec<SignalTrack> = Vec::new();
    let mut signal_sources: Vec<std::sync::Arc<dyn igv_core::source::SignalSource>> = Vec::new();
    let signal_format_override = args
        .signal_format
        .as_deref()
        .and_then(SignalFormat::parse);
    for path in &args.signals {
        let src = open_signal(path, signal_format_override).await?;
        signals.push(SignalTrack {
            path: path.clone(),
            display: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("signal")
                .to_string(),
            source: std::sync::Arc::clone(&src),
        });
        signal_sources.push(src);
    }

    let initial = match args.region.as_deref() {
        Some(s) => Region::parse(s)
            .with_context(|| format!("invalid -r region: {s}"))?,
        None => {
            let chr = references
                .first()
                .ok_or_else(|| anyhow!("FASTA contains no references"))?
                .name
                .clone();
            Region::new(chr, 1, igv_core::region::DEFAULT_REGION_WIDTH)?
        }
    };

    let bam_count = bams.len();
    let mut state = AppState {
        fasta: fasta.clone(),
        vcf: vcf.clone(),
        bams,
        references,
        region: initial,
        reference_seq: Vec::new(),
        variants: Vec::new(),
        bam_rows: vec![Vec::new(); bam_count],
        bam_lanes: vec![Vec::new(); bam_count],
        bam_total_lanes: vec![0u16; bam_count],
        bam_scroll: 0,
        annotations,
        annotation_rows: vec![Vec::new(); annotation_sources.len()],
        signals,
        signal_bins: vec![Vec::new(); signal_sources.len()],
        signal_shared_scale: false,
        signal_track_height: SIGNAL_DEFAULT_HEIGHT,
        alignment_height: ALIGNMENT_DEFAULT_HEIGHT,
        coverage_height: COVERAGE_DEFAULT_HEIGHT,
        theme: theme.clone(),
        light_mode: args.light_mode,
        thresholds: Thresholds::default(),
        bookmarks: std::collections::HashMap::new(),
        status: None,
        command_open: false,
        command_buffer: String::new(),
        generation: 0,
        loading: true,
        should_quit: false,
    };

    let (tx, mut rx) = mpsc::channel::<LoadResult>(64);
    let mut loader = Loader::new(fasta, vcf, bam_sources, annotation_sources, signal_sources, tx);
    if let Some(req) = state.apply(Action::Goto(state.region.clone())) {
        loader.dispatch(req);
    }

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut input_state = InputState::default();
    let mut palette = CommandPalette::default();
    let mut events = EventStream::new();

    let result = run_loop(
        &mut terminal,
        &mut state,
        &mut loader,
        &mut rx,
        &mut events,
        &mut input_state,
        &mut palette,
    )
    .await;

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    if let Err(e) = result {
        error!("fatal: {e}");
        eprintln!("igv-rs exited with error: {e}");
        return Err(e);
    }
    Ok(())
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    loader: &mut Loader,
    rx: &mut mpsc::Receiver<LoadResult>,
    events: &mut EventStream,
    input_state: &mut InputState,
    palette: &mut CommandPalette,
) -> anyhow::Result<()> {
    while !state.should_quit {
        terminal.draw(|f| draw(f, state))?;

        tokio::select! {
            maybe_evt = events.next() => {
                if let Some(Ok(evt)) = maybe_evt {
                    let action = if state.command_open {
                        let act = palette.handle(&evt);
                        state.command_buffer = palette.input.value().to_string();
                        act
                    } else if matches!(&evt, Event::Key(k) if k.kind != KeyEventKind::Press) {
                        Action::None
                    } else {
                        let act = input_state.map(&evt, false);
                        if matches!(act, Action::OpenCommand) {
                            palette.open();
                        }
                        act
                    };
                    if let Some(req) = state.apply(action) {
                        loader.dispatch(req);
                    }
                }
            }
            maybe_result = rx.recv() => {
                if let Some(result) = maybe_result {
                    apply_load_result(state, result);
                    if state.bam_rows.iter().all(|r| !r.is_empty() || state.bams.is_empty())
                        && !state.reference_seq.is_empty()
                    {
                        state.loading = false;
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(150)) => {
                let stale = state
                    .status
                    .as_ref()
                    .is_some_and(|s| s.set_at.elapsed() > Duration::from_secs(3));
                if stale {
                    state.status = None;
                }
            }
        }
    }
    Ok(())
}

fn apply_load_result(state: &mut AppState, result: LoadResult) {
    match result {
        LoadResult::Reference { generation, region, bytes } => {
            if generation == state.generation && region.chrom == state.region.chrom {
                state.reference_seq = bytes;
            }
        }
        LoadResult::Variants { generation, records } => {
            if generation == state.generation {
                state.variants = records;
            }
        }
        LoadResult::Bam { generation, bam_index, rows } => {
            if generation == state.generation {
                let lanes = igv_core::alignment::assign_lanes(&rows);
                let total = lanes.iter().copied().max().map(|m| m + 1).unwrap_or(0);
                let total_u16 = total.min(u16::MAX as u32) as u16;
                if let Some(slot) = state.bam_rows.get_mut(bam_index) {
                    *slot = rows;
                }
                if let Some(slot) = state.bam_lanes.get_mut(bam_index) {
                    *slot = lanes;
                }
                if let Some(slot) = state.bam_total_lanes.get_mut(bam_index) {
                    *slot = total_u16;
                }
            }
        }
        LoadResult::Annotation { generation, track_index, transcripts } => {
            if generation == state.generation {
                if let Some(slot) = state.annotation_rows.get_mut(track_index) {
                    *slot = transcripts;
                }
            }
        }
        LoadResult::Signal { generation, track_index, bins } => {
            if generation == state.generation {
                if let Some(slot) = state.signal_bins.get_mut(track_index) {
                    *slot = bins;
                }
            }
        }
        LoadResult::Error { generation, message } => {
            if generation == state.generation {
                state.set_status(StatusKind::Error, message);
            }
        }
    }
}

fn draw(f: &mut ratatui::Frame<'_>, state: &AppState) {
    let spec = LayoutSpec {
        has_vcf: state.vcf.is_some(),
        bam_count: state.bams.len(),
        annotation_tracks: state.annotations.len(),
        coverage_height: state.coverage_height,
        alignments_min_per_track: state.alignment_height,
        signal_count: state.signals.len(),
        signal_height_per_track: state.signal_track_height,
        ..Default::default()
    };
    let areas = compute(f.area(), &spec);
    f.render_widget(widgets::header::HeaderWidget { state, theme: &state.theme }, areas.header);
    f.render_widget(widgets::overview::OverviewWidget { state, theme: &state.theme }, areas.overview);
    f.render_widget(widgets::ruler::RulerWidget { state, theme: &state.theme }, areas.ruler);
    f.render_widget(widgets::sequence::SequenceWidget { state, theme: &state.theme }, areas.sequence);
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
    if let Some(va) = areas.variants {
        f.render_widget(widgets::variants::VariantsWidget { state, theme: &state.theme }, va);
    }
    if let Some(ca) = areas.coverage {
        f.render_widget(widgets::coverage::CoverageWidget { state, theme: &state.theme }, ca);
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
        let bins: &[igv_core::source::SignalBin] =
            state.signal_bins.get(i).map(|v| v.as_slice()).unwrap_or(&[]);
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
    for (i, area) in areas.alignments.iter().enumerate() {
        f.render_widget(
            widgets::alignments::AlignmentsWidget { state, theme: &state.theme, track_index: i },
            *area,
        );
    }
    f.render_widget(widgets::footer::FooterWidget { state, theme: &state.theme }, areas.footer);
}
