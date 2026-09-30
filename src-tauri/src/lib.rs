//! Pérola Negra — casca Tauri (app desktop de compartilhamento de tela).
//!
//! O que o lado nativo faz (e o navegador nao consegue):
//!   1. Listar os processos que estao tocando audio       -> `list_audio_processes`
//!   2. Capturar o audio de UM processo (ou "tudo menos X") -> `start_game_audio`
//!   3. Servir esse audio num WebSocket local              -> a webview conecta e
//!      transforma em uma faixa de audio pra publicar no WebRTC (ver index.html)
//!
//! Todo o resto (PeerJS, mesh, tela, chat) vive no index.html, sem alteracao.

mod bridge;
// A captura real do Windows so entra fora do modo mock (ela ainda precisa de
// ajuste na API da crate `windows`).
#[cfg(all(windows, not(feature = "mock-audio")))]
mod audio_capture;
#[cfg(all(windows, not(feature = "mock-audio")))]
mod processes;

use parking_lot::Mutex;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

/// Estado compartilhado: no maximo uma captura ativa por vez.
#[derive(Default)]
pub struct AppState {
    session: Mutex<Option<bridge::CaptureSession>>,
}

#[derive(Serialize, Clone)]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
    /// caminho do executavel (pra UI mostrar/deduplicar)
    pub exe: String,
}

#[derive(Serialize, Clone)]
pub struct GameAudioStarted {
    /// porta do WebSocket local (ws://127.0.0.1:PORTA)
    pub port: u16,
    pub sample_rate: u32,
    pub channels: u16,
}

// ---------------------------------------------------------------------------
// Comandos expostos pra webview (window.__TAURI__.core.invoke)
// ---------------------------------------------------------------------------

#[tauri::command]
fn list_audio_processes() -> Result<Vec<ProcInfo>, String> {
    #[cfg(all(windows, not(feature = "mock-audio")))]
    {
        processes::list_audio_processes()
    }
    #[cfg(any(not(windows), feature = "mock-audio"))]
    {
        // modo mock / fora do Windows: lista ficticia so pra exercitar a UI
        Ok(vec![
            ProcInfo { pid: 1234, name: "Jogo (mock)".into(), exe: String::new() },
            ProcInfo { pid: 5678, name: "Discord (mock)".into(), exe: String::new() },
        ])
    }
}

/// Comeca a capturar. `exclude = true` => captura TUDO menos o processo `pid`
/// (util pra "tudo menos o Discord"). `exclude = false` => so a arvore do `pid`.
#[tauri::command]
async fn start_game_audio(
    state: State<'_, Arc<AppState>>,
    pid: u32,
    exclude: bool,
) -> Result<GameAudioStarted, String> {
    // derruba uma sessao anterior, se houver (guard sai de escopo antes do await)
    let prev = state.session.lock().take();
    if let Some(s) = prev {
        s.stop().await;
    }

    let session = bridge::CaptureSession::start(pid, exclude)
        .await
        .map_err(|e| e.to_string())?;

    let info = GameAudioStarted {
        port: session.port,
        sample_rate: bridge::SAMPLE_RATE,
        channels: bridge::CHANNELS,
    };
    *state.session.lock() = Some(session);
    Ok(info)
}

#[tauri::command]
async fn stop_game_audio(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let prev = state.session.lock().take();
    if let Some(s) = prev {
        s.stop().await;
    }
    Ok(())
}

// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Arc::new(AppState::default()))
        .invoke_handler(tauri::generate_handler![
            list_audio_processes,
            start_game_audio,
            stop_game_audio
        ])
        .setup(|_app| {
            // Nota: na 1a vez o WebView2 pergunta "Permitir microfone?" / tela.
            // Clique em Permitir uma vez — ele lembra (o user-data-folder do
            // WebView2 persiste). Se quiser suprimir o prompt de vez, da pra
            // adicionar um handler de `PermissionRequested` via
            // `window.with_webview(...)` + a crate `webview2-com` (versao casando
            // com a que o Tauri usa: `cargo tree -i webview2-com`).
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erro ao iniciar a Perola Negra");
}
