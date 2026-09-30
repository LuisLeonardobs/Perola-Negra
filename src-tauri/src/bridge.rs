//! Ponte: PCM capturado  ->  WebSocket local  ->  webview.
//!
//! Formato na rede: f32 intercalado (L R L R ...), little-endian, 48 kHz, 2 canais.
//! Cada mensagem binaria do WebSocket e um bloco de amostras (tamanho variavel).
//! O index.html reconstroi isso num AudioWorklet e vira uma faixa de audio WebRTC.

use futures_util::SinkExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: u16 = 2;

/// Uma captura em andamento. Fica no AppState; `stop()` desmonta tudo.
pub struct CaptureSession {
    pub port: u16,
    stop: Arc<AtomicBool>,
    ws_task: JoinHandle<()>,
    /// thread da captura (WASAPI) — Option pra dar `join` no stop
    capture: Option<std::thread::JoinHandle<()>>,
    #[cfg(feature = "mock-audio")]
    mock_task: Option<JoinHandle<()>>,
}

impl CaptureSession {
    pub async fn start(pid: u32, exclude: bool) -> std::io::Result<Self> {
        let _ = (pid, exclude); // usados so no ramo Windows/nao-mock

        // canal 1 -> N: a fonte publica, cada cliente WS consome.
        let (tx, _rx) = broadcast::channel::<Vec<u8>>(512);
        let stop = Arc::new(AtomicBool::new(false));

        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();

        // ---- servidor WebSocket ----
        // `stop()` faz `ws_task.abort()`; as tasks-filhas saem quando todos os
        // Sender do broadcast forem dropados (o que acontece no stop).
        let ws_tx = tx.clone();
        let ws_task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let mut rx = ws_tx.subscribe();
                tokio::spawn(async move {
                    let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else { return };
                    loop {
                        match rx.recv().await {
                            Ok(buf) => {
                                if ws.send(Message::Binary(buf.into())).await.is_err() {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => continue, // cliente lento: pula
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                    let _ = ws.close(None).await;
                });
            }
        });

        // ---- fonte do audio ----
        #[cfg(feature = "mock-audio")]
        let (capture, mock_task): (Option<std::thread::JoinHandle<()>>, Option<JoinHandle<()>>) =
            (None, Some(spawn_mock(tx.clone(), stop.clone())));

        #[cfg(all(windows, not(feature = "mock-audio")))]
        let capture: Option<std::thread::JoinHandle<()>> =
            Some(crate::audio_capture::spawn(pid, exclude, tx.clone(), stop.clone()));

        #[cfg(all(not(windows), not(feature = "mock-audio")))]
        let capture: Option<std::thread::JoinHandle<()>> = None;

        Ok(Self {
            port,
            stop,
            ws_task,
            capture,
            #[cfg(feature = "mock-audio")]
            mock_task,
        })
    }

    pub async fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.ws_task.abort();
        #[cfg(feature = "mock-audio")]
        if let Some(t) = self.mock_task.take() {
            t.abort();
        }
        if let Some(handle) = self.capture.take() {
            // a thread de captura sai sozinha ao ver a flag; espera no pool blocking
            let _ = tokio::task::spawn_blocking(move || {
                let _ = handle.join();
            })
            .await;
        }
    }
}

/// Tom de teste (senoide 220 Hz) — fecha a ponte sem depender do WASAPI.
#[cfg(feature = "mock-audio")]
fn spawn_mock(tx: broadcast::Sender<Vec<u8>>, stop: Arc<AtomicBool>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let frames_per_chunk = 480usize; // 10 ms @ 48k
        let mut phase = 0f32;
        let step = 2.0 * std::f32::consts::PI * 220.0 / SAMPLE_RATE as f32;
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(10));
        while !stop.load(Ordering::Relaxed) {
            tick.tick().await;
            let mut buf = Vec::with_capacity(frames_per_chunk * 2 * 4);
            for _ in 0..frames_per_chunk {
                let s = phase.sin() * 0.15;
                phase += step;
                if phase > std::f32::consts::TAU {
                    phase -= std::f32::consts::TAU;
                }
                buf.extend_from_slice(&s.to_le_bytes());
                buf.extend_from_slice(&s.to_le_bytes());
            }
            let _ = tx.send(buf);
        }
    })
}
