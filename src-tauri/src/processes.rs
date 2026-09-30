//! Lista os processos abertos (nome + pid) pra pessoa escolher "o jogo".
//! Usa o snapshot do Toolhelp — a API de enumeracao de processos mais simples
//! do Windows, sem COM.

use crate::ProcInfo;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

// processos de sistema / de fundo que nunca vao ser "o jogo" — tira da lista
const OCULTOS: &[&str] = &[
    "svchost", "runtimebroker", "dwm", "csrss", "wininit", "winlogon", "services",
    "lsass", "smss", "fontdrvhost", "sihost", "taskhostw", "explorer", "searchhost",
    "textinputhost", "shellexperiencehost", "startmenuexperiencehost", "ctfmon",
    "conhost", "registry", "memory compression", "system", "audiodg", "spoolsv",
    "perolanegra", "msedgewebview2",
];

pub fn list_audio_processes() -> Result<Vec<ProcInfo>, String> {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|e| e.to_string())?;

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        let me = std::process::id();
        let mut out: Vec<ProcInfo> = Vec::new();

        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let end = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let exe = String::from_utf16_lossy(&entry.szExeFile[..end]);
                let pid = entry.th32ProcessID;
                let base = exe.trim_end_matches(".exe").trim_end_matches(".EXE").to_string();
                let low = base.to_lowercase();

                if pid != 0
                    && pid != me
                    && !low.is_empty()
                    && !OCULTOS.contains(&low.as_str())
                {
                    out.push(ProcInfo { pid, name: base, exe });
                }

                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snap);

        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        out.dedup_by(|a, b| a.name.to_lowercase() == b.name.to_lowercase());
        Ok(out)
    }
}
