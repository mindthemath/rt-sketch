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
use engine::sampler::LineLengthMode;
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
    let config = Config::from_args(&args);

    tracing::info!(
        "canvas: {}x{} cm, processing: {}x{} px, preview: {}x{} px ({}ppi)",
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
    let preview_w = config.preview_width();
    let preview_h = config.preview_height();

    let mut frame_source = FrameSource::new(source_str, pw, ph, config.fps);

    // Read the first frame
    let first_frame = frame_source
        .next_frame()
        .expect("failed to read first frame from source");

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
                    tracing::info!("engine started/resumed");
                }
                "pause" => {
                    *state.running.lock().unwrap() = false;
                    tracing::info!("engine paused");
                }
                "reset" => {
                    engine.reset();
                    *state.iteration.lock().unwrap() = 0;
                    *state.current_score.lock().unwrap() = 1.0;
                    *state.running.lock().unwrap() = false;
                    // Update shared canvas
                    *state.canvas.lock().unwrap() = engine.canvas.clone();
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
                "set_line_mode" => {
                    if let Some(v) = cmd.value.and_then(|v| v.as_str().map(|s| s.to_string())) {
                        engine.line_length_mode = match v.as_str() {
                            "fixed" => LineLengthMode::Fixed,
                            _ => LineLengthMode::Random,
                        };
                        tracing::info!("line length mode set to {:?}", engine.line_length_mode);
                    }
                }
                "set_line_len" => {
                    if let Some(v) = cmd.value.and_then(|v| v.as_f64()) {
                        engine.fixed_line_len = v;
                        tracing::info!("fixed line length set to {} cm", v);
                    }
                }
                _ => {}
            }
        }

        let running = *state.running.lock().unwrap();
        if !running {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }

        let step_start = Instant::now();

        // Try to get a new frame (for video/webcam sources)
        if let Some(new_frame) = frame_source.next_frame() {
            let mut tf = state.target_frame.lock().unwrap();
            *tf = Some(new_frame);
        }

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

        // Send update to web UI
        let canvas_raster = engine.canvas.rasterize(pw, ph);
        let canvas_b64 = web::gray_to_base64_png(&canvas_raster, pw, ph);

        // Generate preview at full PPI scale
        let preview_png = engine.canvas.rasterize_png(preview_w, preview_h);
        let preview_b64 = base64::engine::general_purpose::STANDARD.encode(&preview_png);

        // Also send target if it changed
        let target_b64 = web::gray_to_base64_png(&target, pw, ph);

        let _ = state.update_tx.send(UpdateMessage {
            msg_type: "update".to_string(),
            canvas_png: Some(canvas_b64),
            target_png: Some(target_b64),
            preview_png: Some(preview_b64),
            iteration: Some(iteration),
            score: Some(result.score),
            fps: None,
            k: Some(current_k),
            line_count: Some(engine.canvas.lines.len()),
            running: Some(true),
        });

        // Frame pacing
        let elapsed = step_start.elapsed();
        if elapsed < target_frame_duration {
            std::thread::sleep(target_frame_duration - elapsed);
        }
    }
}
