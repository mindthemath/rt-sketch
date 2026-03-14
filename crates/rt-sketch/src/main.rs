mod config;
mod engine;
mod frame_source;
mod output;
mod stream_output;
mod tcp_output;
mod web;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use clap::Parser;
use tokio::sync::{broadcast, mpsc};

use config::{Args, Config};

/// Deferred stream configuration — FFmpeg is spawned on first "start".
enum StreamConfig {
    Url(String),
    File(String),
}
use engine::canvas::Canvas;
use engine::ProposalEngine;
use frame_source::FrameSource;
use output::{CommandSink, HttpSink, NoopSink};
use web::{AppState, ControlCommand, UpdateMessage};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rt_sketch=info".parse().unwrap()),
        )
        .init();

    let args = Args::parse();

    if let Some(threads) = args.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()
            .expect("failed to configure rayon thread pool");
        tracing::info!("rayon thread pool: {} threads", threads);
    }

    let mut config = Config::from_args(&args);

    // Probe source to fit canvas to its aspect ratio
    if let Some((sw, sh)) = frame_source::probe_source_dimensions(&args.source) {
        tracing::info!("source dimensions: {}x{} px", sw, sh);
        config.fit_to_source(sw, sh);
    } else {
        tracing::warn!("could not probe source dimensions, using canvas aspect ratio as-is");
    }

    tracing::info!(
        "canvas: {:.1}x{:.1} cm, processing: {}x{} px, preview: {}x{} px ({}ppi)",
        config.canvas_width_cm,
        config.canvas_height_cm,
        config.processing_width(),
        config.resolution,
        config.preview_width(),
        config.preview_height(),
        config.ppi,
    );

    let (update_tx, _) = broadcast::channel::<UpdateMessage>(64);
    let (control_tx, control_rx) = mpsc::channel::<ControlCommand>(64);

    let canvas = engine::canvas::Canvas::new(config.canvas_width_cm, config.canvas_height_cm);

    let state = Arc::new(AppState {
        config: Mutex::new(config.clone()),
        canvas: Mutex::new(canvas),
        target_frame: Mutex::new(None),
        iteration: Mutex::new(0),
        current_score: Mutex::new(1.0),
        running: Mutex::new(false),
        update_tx: update_tx.clone(),
        control_tx,
    });

    // Build command sink
    let sink: Box<dyn CommandSink> = match &args.robot_server {
        Some(url) => {
            tracing::info!("robot server: {}", url);
            Box::new(HttpSink::new(url))
        }
        None => {
            tracing::info!("no robot server configured, preview-only mode");
            Box::new(NoopSink)
        }
    };

    // Start web server
    let web_state = state.clone();
    let explicit_port = args.web_port;
    let default_port = 8080u16;
    let listener = {
        let port = explicit_port.unwrap_or(default_port);
        match tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await {
            Ok(l) => l,
            Err(e) if explicit_port.is_some() => {
                eprintln!("error: cannot bind to port {}: {}", port, e);
                std::process::exit(1);
            }
            Err(_) => {
                // Auto-select: try ports above the default
                let mut found = None;
                for p in (default_port + 1)..=(default_port + 100) {
                    if let Ok(l) = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", p)).await {
                        found = Some(l);
                        break;
                    }
                }
                found.unwrap_or_else(|| {
                    eprintln!(
                        "error: could not find an available port in range {}-{}",
                        default_port,
                        default_port + 100
                    );
                    std::process::exit(1);
                })
            }
        }
    };
    let actual_port = listener.local_addr().unwrap().port();
    tracing::info!("web UI: http://localhost:{}", actual_port);
    tokio::spawn(async move {
        let app = web::build_router(web_state);
        axum::serve(listener, app).await.unwrap();
    });

    // Validate stream flags
    let stream_config = match (&args.stream_url, &args.stream_output) {
        (Some(_), Some(_)) => {
            eprintln!("error: cannot use both --stream-url and --stream-output");
            std::process::exit(1);
        }
        (Some(url), None) => Some(StreamConfig::Url(url.clone())),
        (None, Some(path)) => Some(StreamConfig::File(path.clone())),
        (None, None) => None,
    };

    // Set up TCP viewer output if requested
    let tcp_config = args.stream_tcp.clone().map(|addr| {
        let name = args
            .stream_name
            .clone()
            .unwrap_or_else(|| "rt-sketch".to_string());
        (addr, name)
    });

    // Shutdown flag for graceful Ctrl+C handling
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_signal = shutdown.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Ctrl+C received, shutting down...");
        shutdown_signal.store(true, Ordering::SeqCst);
    });

    // Validate --wait-for-viewer requires --stream-tcp
    let wait_for_viewer = args.wait_for_viewer;
    if wait_for_viewer && args.stream_tcp.is_none() {
        eprintln!("error: --wait-for-viewer requires --stream-tcp");
        std::process::exit(1);
    }

    // Run engine loop in a blocking thread
    let engine_state = state.clone();
    let source_str = args.source.clone();
    let auto_start = args.auto_start;
    let stamp_library_path = args.stamp_library.clone();
    let stamp_crop_str = args.stamp_crop.clone();
    let stamp_rotate = !args.no_stamp_rotate;
    tokio::task::spawn_blocking(move || {
        engine_loop(
            engine_state,
            &source_str,
            config,
            sink,
            control_rx,
            stream_config,
            shutdown,
            tcp_config,
            auto_start,
            wait_for_viewer,
            stamp_library_path,
            stamp_crop_str,
            stamp_rotate,
        );
    })
    .await
    .unwrap();
}

/// Build a 256-entry combined LUT: exposure → gamma → contrast, all in one pass.
fn build_correction_lut(gamma: f64, exposure: f64, contrast: f64) -> [u8; 256] {
    let mut lut = [0u8; 256];
    let inv_gamma = 1.0 / gamma;
    let exp_scale = 2.0_f64.powf(exposure);
    for i in 0..256 {
        let mut v = i as f64 / 255.0;
        // Exposure (EV stops)
        v *= exp_scale;
        // Gamma
        v = v.clamp(0.0, 1.0).powf(inv_gamma);
        // Contrast (linear around midpoint)
        v = (v - 0.5) * contrast + 0.5;
        lut[i] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    lut
}

/// Apply a correction LUT to a grayscale buffer, returning a new buffer.
fn apply_correction(buf: &[u8], lut: &[u8; 256]) -> Vec<u8> {
    buf.iter().map(|&p| lut[p as usize]).collect()
}

fn engine_loop(
    state: Arc<AppState>,
    source_str: &str,
    config: Config,
    sink: Box<dyn CommandSink>,
    mut control_rx: mpsc::Receiver<ControlCommand>,
    stream_config: Option<StreamConfig>,
    shutdown: Arc<AtomicBool>,
    tcp_config: Option<(String, String)>,
    auto_start: bool,
    wait_for_viewer: bool,
    stamp_library_path: Option<String>,
    stamp_crop_str: String,
    stamp_rotate: bool,
) {
    // Stream output is spawned lazily on first "start"
    let mut stream: Option<stream_output::StreamOutput> = None;

    // Shared running flag for TCP output — reported in HELLO on reconnect
    let tcp_running = Arc::new(AtomicBool::new(auto_start));

    // TCP viewer output — connects immediately if configured
    let mut tcp_output: Option<tcp_output::TcpOutput> = tcp_config.map(|(addr, name)| {
        tracing::info!("TCP viewer: {} as \"{}\"", addr, name);
        tcp_output::TcpOutput::new(
            &addr,
            &name,
            config.canvas_width_cm,
            config.canvas_height_cm,
            config.stroke_width_cm,
            tcp_running.clone(),
            shutdown.clone(),
        )
    });

    // Block until viewer is reachable if requested
    if wait_for_viewer {
        if let Some(ref mut tcp) = tcp_output {
            if !tcp.is_connected() {
                tracing::info!("waiting for viewer connection...");
                if !tcp.wait_for_connection() {
                    tracing::info!("shutdown during viewer wait");
                    return;
                }
            }
        }
    }

    let pw = config.processing_width();
    let ph = config.resolution;

    let mut frame_source = FrameSource::new(source_str, pw, ph, config.fps);

    // Wait for the first frame — retries indefinitely (ffmpeg auto-respawns)
    let first_frame = {
        let mut attempt = 0u64;
        loop {
            if shutdown.load(Ordering::SeqCst) {
                tracing::info!("shutdown during frame source init");
                return;
            }
            if let Some(f) = frame_source.next_frame() {
                break f;
            }
            attempt += 1;
            if attempt % 10 == 0 {
                tracing::info!("waiting for first frame ({}s)...", attempt / 5);
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    };

    {
        let mut tf = state.target_frame.lock().unwrap();
        *tf = Some(first_frame.clone());
    }

    // Send initial target to UI
    let target_b64 = web::gray_to_base64_png(&first_frame, pw, ph);
    let _ = state.update_tx.send(UpdateMessage {
        msg_type: "target".to_string(),
        target_png: Some(target_b64),
        ..Default::default()
    });

    let mut engine = ProposalEngine::new(&config);

    // Load stamp library if configured
    if let Some(ref stamp_csv) = stamp_library_path {
        let stamp_crop: engine::stamp::StampCrop = match stamp_crop_str.parse() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error: {}", e);
                return;
            }
        };
        match engine::stamp::StampLibrary::load(stamp_csv, config.stroke_width_cm) {
            Ok(library) => {
                tracing::info!("stamp crop: {}, rotate: {}", stamp_crop, stamp_rotate);
                engine.set_stamp_library(library, stamp_crop, stamp_rotate);
            }
            Err(e) => {
                eprintln!("error: failed to load stamp library: {}", e);
                return;
            }
        }
    }

    let mut current_k = config.k;
    let target_frame_duration = Duration::from_secs_f64(1.0 / config.fps);

    // Combined correction LUT (exposure → gamma → contrast) — precomputed for O(1) per-pixel cost
    let mut current_gamma = config.gamma;
    let mut current_exposure = config.exposure;
    let mut current_contrast = config.contrast;
    let mut correction_lut =
        build_correction_lut(current_gamma, current_exposure, current_contrast);

    if auto_start {
        *state.running.lock().unwrap() = true;
        tracing::info!("engine ready, auto-starting");
    } else {
        tracing::info!("engine ready, waiting for start command...");
    }

    // Main loop
    loop {
        // Check for graceful shutdown (Ctrl+C)
        if shutdown.load(Ordering::SeqCst) {
            tracing::info!("engine shutting down, finalizing outputs...");
            drop(stream);
            drop(tcp_output);
            tracing::info!("shutdown complete");
            return;
        }

        // Poll for viewer commands (play/pause/reset from rt-viewer)
        let viewer_cmds: Vec<_> = tcp_output
            .as_mut()
            .map(|tcp| tcp.poll_commands())
            .unwrap_or_default();
        for cmd in viewer_cmds {
            match cmd {
                tcp_output::ViewerCommand::Play => {
                    tracing::info!("viewer: play");
                    *state.running.lock().unwrap() = true;
                    let _ = state.update_tx.send(UpdateMessage {
                        msg_type: "state".to_string(),
                        running: Some(true),
                        ..Default::default()
                    });
                }
                tcp_output::ViewerCommand::Pause => {
                    tracing::info!("viewer: pause");
                    *state.running.lock().unwrap() = false;
                    let _ = state.update_tx.send(UpdateMessage {
                        msg_type: "state".to_string(),
                        running: Some(false),
                        ..Default::default()
                    });
                }
                tcp_output::ViewerCommand::Reset => {
                    tracing::info!("viewer: reset");
                    if let Some(ref mut tcp) = tcp_output {
                        tcp.send_reset();
                    }
                    let was_running = *state.running.lock().unwrap();
                    engine.reset();
                    *state.iteration.lock().unwrap() = 0;
                    *state.current_score.lock().unwrap() = 1.0;
                    *state.canvas.lock().unwrap() = engine.canvas.clone();
                    let _ = state.update_tx.send(UpdateMessage {
                        msg_type: "reset".to_string(),
                        iteration: Some(0),
                        score: Some(1.0),
                        line_count: Some(0),
                        running: Some(was_running),
                        total_length: Some(0.0),
                        ..Default::default()
                    });
                }
            }
        }

        // Process control commands (non-blocking)
        while let Ok(cmd) = control_rx.try_recv() {
            match cmd.command.as_str() {
                "start" | "resume" => {
                    // Spawn stream FFmpeg on first start
                    if stream.is_none() {
                        if let Some(ref sc) = stream_config {
                            let (url, path) = match sc {
                                StreamConfig::Url(u) => (Some(u.as_str()), None),
                                StreamConfig::File(p) => (None, Some(p.as_str())),
                            };
                            tracing::info!("starting stream output");
                            stream = Some(stream_output::StreamOutput::new(
                                config.preview_width(),
                                config.preview_height(),
                                config.fps,
                                url,
                                path,
                            ));
                        }
                    }
                    *state.running.lock().unwrap() = true;
                    if let Some(ref mut tcp) = tcp_output {
                        tcp.send_state(true);
                    }
                    let _ = state.update_tx.send(UpdateMessage {
                        msg_type: "state".to_string(),
                        running: Some(true),
                        ..Default::default()
                    });
                    tracing::info!("engine started/resumed");
                }
                "pause" => {
                    *state.running.lock().unwrap() = false;
                    if let Some(ref mut tcp) = tcp_output {
                        tcp.send_state(false);
                    }
                    // Send final preview so the UI shows the latest canvas state
                    let canvas_raster = Canvas::pixmap_to_gray(engine.cached_pixmap());
                    let canvas_b64 = web::gray_to_base64_png(&canvas_raster, pw, ph);
                    let preview_png = engine.preview_png();
                    let preview_b64 =
                        base64::engine::general_purpose::STANDARD.encode(&preview_png);
                    let _ = state.update_tx.send(UpdateMessage {
                        msg_type: "state".to_string(),
                        canvas_png: Some(canvas_b64),
                        preview_png: Some(preview_b64),
                        running: Some(false),
                        ..Default::default()
                    });
                    tracing::info!("engine paused");
                }
                "reset" => {
                    // Finalize stream on reset (closes the file cleanly)
                    if let Some(s) = stream.take() {
                        tracing::info!("finalizing stream output on reset");
                        drop(s);
                    }
                    if let Some(ref mut tcp) = tcp_output {
                        tcp.send_reset();
                    }
                    engine.reset();
                    *state.iteration.lock().unwrap() = 0;
                    *state.current_score.lock().unwrap() = 1.0;
                    *state.running.lock().unwrap() = false;
                    *state.canvas.lock().unwrap() = engine.canvas.clone();
                    let _ = state.update_tx.send(UpdateMessage {
                        msg_type: "reset".to_string(),
                        iteration: Some(0),
                        score: Some(1.0),
                        line_count: Some(0),
                        running: Some(false),
                        total_length: Some(0.0),
                        ..Default::default()
                    });
                    tracing::info!("engine reset");
                }
                "set_k" => {
                    if let Some(v) = cmd.value.and_then(|v| v.as_u64()) {
                        current_k = v as usize;
                        tracing::info!("K set to {}", current_k);
                    }
                }
                "set_x_sampler" => {
                    if let Some(v) = cmd.value.and_then(|v| v.as_str().map(|s| s.to_string())) {
                        match engine.set_x_sampler(&v) {
                            Ok(()) => tracing::info!("x sampler set to {}", v),
                            Err(e) => tracing::warn!("invalid x sampler '{}': {}", v, e),
                        }
                    }
                }
                "set_y_sampler" => {
                    if let Some(v) = cmd.value.and_then(|v| v.as_str().map(|s| s.to_string())) {
                        match engine.set_y_sampler(&v) {
                            Ok(()) => tracing::info!("y sampler set to {}", v),
                            Err(e) => tracing::warn!("invalid y sampler '{}': {}", v, e),
                        }
                    }
                }
                "set_length_sampler" => {
                    if let Some(v) = cmd.value.and_then(|v| v.as_str().map(|s| s.to_string())) {
                        match engine.set_length_sampler(&v) {
                            Ok(()) => tracing::info!("length sampler set to {}", v),
                            Err(e) => tracing::warn!("invalid length sampler '{}': {}", v, e),
                        }
                    }
                }
                "set_min_len" => {
                    if let Some(v) = cmd.value.and_then(|v| v.as_f64()) {
                        engine.min_line_len = v;
                        tracing::info!("min line length set to {} cm", v);
                    }
                }
                "set_max_len" => {
                    if let Some(v) = cmd.value.and_then(|v| v.as_f64()) {
                        engine.max_line_len = v;
                        tracing::info!("max line length set to {} cm", v);
                    }
                }
                "set_alpha" => {
                    if let Some(v) = cmd.value.and_then(|v| v.as_f64()) {
                        engine.alpha = v;
                        tracing::info!("alpha set to {}", v);
                    }
                }
                "set_gamma" => {
                    if let Some(v) = cmd.value.and_then(|v| v.as_f64()) {
                        current_gamma = v;
                        correction_lut =
                            build_correction_lut(current_gamma, current_exposure, current_contrast);
                        tracing::info!("gamma set to {}", v);
                    }
                }
                "set_exposure" => {
                    if let Some(v) = cmd.value.and_then(|v| v.as_f64()) {
                        current_exposure = v;
                        correction_lut =
                            build_correction_lut(current_gamma, current_exposure, current_contrast);
                        tracing::info!("exposure set to {} EV", v);
                    }
                }
                "set_contrast" => {
                    if let Some(v) = cmd.value.and_then(|v| v.as_f64()) {
                        current_contrast = v;
                        correction_lut =
                            build_correction_lut(current_gamma, current_exposure, current_contrast);
                        tracing::info!("contrast set to {}", v);
                    }
                }
                _ => {}
            }
        }

        // Always drain frames to prevent ffmpeg pipe buffer from filling up
        let got_new_frame = if let Some(new_frame) = frame_source.next_frame() {
            let mut tf = state.target_frame.lock().unwrap();
            *tf = Some(new_frame);
            true
        } else {
            false
        };

        let running = *state.running.lock().unwrap();
        tcp_running.store(running, Ordering::Relaxed);
        if !running {
            // Send target updates even while paused so camera feed stays live
            if got_new_frame {
                let raw_target = state.target_frame.lock().unwrap().clone().unwrap();
                let target = apply_correction(&raw_target, &correction_lut);
                let target_b64 = web::gray_to_base64_png(&target, pw, ph);
                let _ = state.update_tx.send(UpdateMessage {
                    msg_type: "target".to_string(),
                    target_png: Some(target_b64),
                    ..Default::default()
                });
            }
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }

        let step_start = Instant::now();

        let raw_target = state.target_frame.lock().unwrap().clone().unwrap();
        let target = apply_correction(&raw_target, &correction_lut);

        // Run one proposal step
        let result = engine.step(&target, current_k);

        // Send winning line(s) to robot and TCP viewer
        for line in &result.winning_lines {
            if let Err(e) = sink.send_line(line) {
                tracing::warn!("failed to send to robot: {}", e);
            }
            if let Some(ref mut tcp) = tcp_output {
                tcp.send_line(line);
            }
        }

        // Update shared state
        let iteration = {
            let mut it = state.iteration.lock().unwrap();
            *it += 1;
            *it
        };
        *state.current_score.lock().unwrap() = result.score;
        *state.canvas.lock().unwrap() = engine.canvas.clone();

        // Send update to web UI — use cached pixmap (O(1) instead of O(n lines))
        let canvas_raster = Canvas::pixmap_to_gray(engine.cached_pixmap());
        let canvas_b64 = web::gray_to_base64_png(&canvas_raster, pw, ph);

        // Preview from cached pixmap — O(1), no re-rasterization needed
        // Write raw RGBA to stream before PNG encoding (cheaper than decoding PNG)
        if let Some(ref mut s) = stream {
            s.write_frame(engine.preview_pixmap_data());
        }
        let preview_png = engine.preview_png();
        let preview_b64 = Some(base64::engine::general_purpose::STANDARD.encode(&preview_png));

        // Only re-encode target if we got a new frame this iteration
        let target_b64 = if got_new_frame {
            Some(web::gray_to_base64_png(&target, pw, ph))
        } else {
            None
        };

        let total_length: f64 = engine.canvas.lines.iter().map(|l| l.length()).sum();
        let last_bbox = if result.winning_lines.is_empty() {
            None
        } else {
            let mut min_x = f64::INFINITY;
            let mut min_y = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            let mut max_y = f64::NEG_INFINITY;
            for line in &result.winning_lines {
                min_x = min_x.min(line.x1).min(line.x2);
                min_y = min_y.min(line.y1).min(line.y2);
                max_x = max_x.max(line.x1).max(line.x2);
                max_y = max_y.max(line.y1).max(line.y2);
            }
            Some([min_x, min_y, max_x, max_y])
        };
        let _ = state.update_tx.send(UpdateMessage {
            msg_type: "update".to_string(),
            canvas_png: Some(canvas_b64),
            target_png: target_b64,
            preview_png: preview_b64,
            iteration: Some(iteration),
            score: Some(result.score),
            fps: None,
            k: Some(current_k),
            line_count: Some(engine.canvas.lines.len()),
            running: Some(true),
            last_line_len: result.last_metric,
            total_length: Some(total_length),
            stamp_count: if stamp_library_path.is_some() {
                Some(engine.stamp_count)
            } else {
                None
            },
            last_bbox,
            canvas_width_cm: None,
            canvas_height_cm: None,
        });

        // Frame pacing
        let elapsed = step_start.elapsed();
        if elapsed < target_frame_duration {
            std::thread::sleep(target_frame_duration - elapsed);
        }
    }
}
