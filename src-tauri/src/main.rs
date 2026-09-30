// Esconde o console preto no Windows quando roda em release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Interruptor de tela preta: se existir "sem-gpu.txt" ao lado do exe,
    // desliga a composicao por GPU do WebView2. Conserta captura preta em
    // alguns PCs (placa dupla, drivers antigos), ao custo de um pouco de
    // fluidez. Sem o arquivo, roda normal.
    #[cfg(windows)]
    {
        let sem_gpu = std::env::current_exe()
            .ok()
            .map(|p| p.with_file_name("sem-gpu.txt").exists())
            .unwrap_or(false);
        if sem_gpu {
            std::env::set_var(
                "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
                "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-gpu-compositing --disable-direct-composition",
            );
        }
    }

    sharescreen_lib::run()
}
