//! Captura de audio de UM processo via WASAPI Process Loopback, usando a crate
//! `wasapi` (que embrulha toda a parte de COM / PROPVARIANT / completion handler).
//!
//! Requer Windows 10 build 20348+ / Windows 11.
//!
//! Se a API da `wasapi` estiver diferente da versao que o cargo baixou, os erros
//! sao localizados e faceis de ajustar — rode `cargo doc -p wasapi --open`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use tokio::sync::broadcast;
use wasapi::{AudioClient, Direction, SampleType, ShareMode, WaveFormat};

/// Sobe a captura numa thread dedicada. Ela roda ate `stop` virar true.
pub fn spawn(
    pid: u32,
    exclude: bool,
    tx: broadcast::Sender<Vec<u8>>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("wasapi-process-loopback".into())
        .spawn(move || {
            if let Err(e) = run(pid, exclude, &tx, &stop) {
                log::error!("captura de audio falhou: {e}");
                stop.store(true, Ordering::Relaxed);
            }
        })
        .expect("nao consegui criar a thread de captura")
}

fn run(
    pid: u32,
    exclude: bool,
    tx: &broadcast::Sender<Vec<u8>>,
    stop: &AtomicBool,
) -> Result<(), Box<dyn std::error::Error>> {
    // COM na thread (MTA). Se a assinatura reclamar, pode ser
    // `wasapi::initialize_mta().ok();` ou precisar segurar um guard.
    let _ = wasapi::initialize_mta();

    // `include_tree = true` -> captura o processo escolhido e os filhos dele.
    // Modo "tudo menos X" ainda nao: o helper da `wasapi` so faz "incluir".
    // (a exclusao real precisa da API crua; fica pro proximo passo)
    if exclude {
        log::warn!("modo 'excluir' ainda nao suportado pela captura; capturando so o processo {pid}");
    }
    let mut client = AudioClient::new_application_loopback_client(pid, true)?;

    // f32 intercalado, 48 kHz, estereo — o `convert = true` deixa o WASAPI
    // reamostrar pra esse formato.
    let format = WaveFormat::new(32, 32, &SampleType::Float, 48_000, 2, None);
    client.initialize_client(
        &format,
        0,
        &Direction::Capture,
        &ShareMode::Shared,
        true,
    )?;

    let event = client.set_get_eventhandle()?;
    let capture = client.get_audiocaptureclient()?;
    client.start_stream()?;
    log::info!("captura de audio iniciada (pid={pid})");

    let mut deque: VecDeque<u8> = VecDeque::new();
    while !stop.load(Ordering::Relaxed) {
        // acorda a cada 200 ms no maximo pra reavaliar o `stop`
        if event.wait_for_event(200).is_err() {
            continue;
        }
        capture.read_from_device_to_deque(&mut deque)?;
        if !deque.is_empty() {
            let chunk: Vec<u8> = deque.drain(..).collect();
            let _ = tx.send(chunk); // erro de send = ninguem ouvindo; nao e fatal
        }
    }

    let _ = client.stop_stream();
    log::info!("captura de audio encerrada");
    Ok(())
}
