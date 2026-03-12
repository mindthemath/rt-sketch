mod config;
mod engine;
mod frame_source;
mod output;
mod web;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use clap::Parser;
use tokio::sync::{broadcast, mpsc};

use config::{Args, Config};
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
    let web_port = args.web_port;
    tokio::spawn(async move {
        let app = web::build_router(web_state);
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", web_port))
            .await
            .expect("failed to bind web server port");
        tracing::info!("web UI: http://localhost:{}", web_port);
        axum::serve(listener, app).await.unwrap();
    });

    // Run engine loop in a blocking thread
    let engine_state = state.clone();
    let source_str = args.source.clone();
    tokio::task::spawn_blocking(move || {
        engine_loop(engine_state, &source_str, config, sink, control_rx);
    })
    .await
    .unwrap();
}

fn engine_loop(
    state: Arc<AppState>,
    source_str: &str,
    config: Config,
    sink: Box<dyn CommandSink>,
    mut control_rx: mpsc::Receiver<ControlCommand>,
) {
    let pw = config.processing_width();
    let ph = config.resolution;

    let mut frame_source = FrameSource::new(source_str, pw, ph, config.fps);

    // Wait for the first frame (webcams may need a moment to initialize)
    let first_frame = {
        let mut frame = None;
        for attempt in 1..=30 {
            if let Some(f) = frame_source.next_frame() {
                frame = Some(f);
                break;
            }
            if attempt % 5 == 0 {
                tracing::info!("waiting for first frame ({}s)...", attempt / 5);
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        frame.expect("failed to read first frame after 6s — check ffmpeg output above")
    };

    {
        let mut tf = state.target_frame.lock().unwrap();
        *tf = Some(first_frame.clone());
    }

    // Send initial target to UI
    let target_b64 = web::gray_to_base64_png(&first_frame, pw, ph);
    let _ = state.update_tx.send(UpdateMessage {
        msg_type: "target".to_string(),
        canvas_png: None,
        target_png: Some(target_b64),
        preview_png: None,
        iteration: None,
        score: None,
        fps: None,
        k: None,
        line_count: None,
        running: None,
        last_line_len: None,
    });

    let mut engine = ProposalEngine::new(&config);
    let mut current_k = config.k;
    let target_frame_duration = Duration::from_secs_f64(1.0 / config.fps);

    tracing::info!("engine ready, waiting for start command...");

    // Main loop
    loop {
        // Process control commands (non-blocking)
        while let Ok(cmd) = control_rx.try_recv() {
            match cmd.command.as_str() {
                "start" | "resume" => {
                    *state.running.lock().unwrap() = true;
                    let _ = state.update_tx.send(UpdateMessage {
                        msg_type: "state".to_string(),
                        canvas_png: None,
                        target_png: None,
                        preview_png: None,
                        iteration: None,
                        score: None,
                        fps: None,
                        k: None,
                        line_count: None,
                        running: Some(true),
                        last_line_len: None,
                    });
                    tracing::info!("engine started/resumed");
                }
                "pause" => {
                    *state.running.lock().unwrap() = false;
                    // Send final preview so the UI shows the latest canvas state
                    let canvas_raster = Canvas::pixmap_to_gray(engine.cached_pixmap());
                    let canvas_b64 = web::gray_to_base64_png(&canvas_raster, pw, ph);
                    let preview_png = engine.preview_png();
                    let preview_b64 = base64::engine::general_purpose::STANDARD.encode(&preview_png);
                    let _ = state.update_tx.send(UpdateMessage {
                        msg_type: "state".to_string(),
                        canvas_png: Some(canvas_b64),
                        target_png: None,
                        preview_png: Some(preview_b64),
                        iteration: None,
                        score: None,
                        fps: None,
                        k: None,
                        line_count: None,
                        running: Some(false),
                        last_line_len: None,
                    });
                    tracing::info!("engine paused");
                }
                "reset" => {
                    engine.reset();
                    *state.iteration.lock().unwrap() = 0;
                    *state.current_score.lock().unwrap() = 1.0;
                    *state.running.lock().unwrap() = false;
                    *state.canvas.lock().unwrap() = engine.canvas.clone();
                    let _ = state.update_tx.send(UpdateMessage {
                        msg_type: "reset".to_string(),
                        canvas_png: None,
                        target_png: None,
                        preview_png: None,
                        iteration: Some(0),
                        score: Some(1.0),
                        fps: None,
                        k: None,
                        line_count: Some(0),
                        running: Some(false),
                        last_line_len: None,
                    });
                    tracing::info!("engine reset");
                }
                "set_k" => {
                    if let Some(v) = cmd.value.and_then(|v| v.as_u64()) {
                        current_k = v as usize;
                        tracing::info!("K set to {}", current_k);
                    }
                }
                "set_sampler" => {
                    if let Some(v) = cmd.value.and_then(|v| v.as_str().map(|s| s.to_string())) {
                        engine.set_sampler(&v);
                        tracing::info!("sampler set to {}", v);
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
        if !running {
            // Send target updates even while paused so camera feed stays live
            if got_new_frame {
                let target = state.target_frame.lock().unwrap().clone().unwrap();
                let target_b64 = web::gray_to_base64_png(&target, pw, ph);
                let _ = state.update_tx.send(UpdateMessage {
                    msg_type: "target".to_string(),
                    canvas_png: None,
                    target_png: Some(target_b64),
                    preview_png: None,
                    iteration: None,
                    score: None,
                    fps: None,
                    k: None,
                    line_count: None,
                    running: None,
                    last_line_len: None,
                });
            }
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }

        let step_start = Instant::now();

        let target = state.target_frame.lock().unwrap().clone().unwrap();

        // Run one proposal step
        let result = engine.step(&target, current_k);

        // Send winning line to robot
        if let Some(ref line) = result.winning_line {
            if let Err(e) = sink.send_line(line) {
                tracing::warn!("failed to send to robot: {}", e);
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
        let preview_png = engine.preview_png();
        let preview_b64 = Some(base64::engine::general_purpose::STANDARD.encode(&preview_png));

        // Only re-encode target if we got a new frame this iteration
        let target_b64 = if got_new_frame {
            Some(web::gray_to_base64_png(&target, pw, ph))
        } else {
            None
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
            last_line_len: result.winning_line.map(|l| l.length()),
        });

        // Frame pacing
        let elapsed = step_start.elapsed();
        if elapsed < target_frame_duration {
            std::thread::sleep(target_frame_duration - elapsed);
        }
    }
}
