# Pérola Negra — app desktop (Tauri)

*(pasta `sharescreen-desktop`, binario interno `sharescreen` — o nome do app pro usuario e "Pérola Negra", vem do `tauri.conf.json`)*

Empacota o `index.html` que já existe num app de Windows leve (~5–10 MB) e
**adiciona a captura de áudio por processo** — mandar o som do jogo sem pegar as
vozes do Discord.

- Quem **transmite** com áudio de processo usa este app.
- Quem só **assiste** continua no navegador (Edge/Firefox/qualquer um), mesma
  sala. Nada muda pra eles.
- O `src/index.html` é o mesmo do navegador + um bloco extra que só liga quando
  roda dentro do app. Dá pra continuar publicando a versão web a partir dele.

---

## Como funciona

```
┌─ Janela Tauri ──────────────────────────────────────────────┐
│  WebView2 (motor do Edge, já instalado no Windows)          │
│    src/index.html  — PeerJS / WebRTC / tela / chat / mesh   │
│         ▲  ws://127.0.0.1:<porta>  (PCM f32 48k estéreo)    │
│         │  AudioWorklet → MediaStreamTrack → publish()      │
│  ┌──────┴───────────────── backend Rust ─────────────────┐  │
│  │ list_audio_processes → apps tocando som              │  │
│  │ start_game_audio(pid, exclude) → WASAPI Process       │  │
│  │   Loopback → broadcast → WebSocket local              │  │
│  │ stop_game_audio                                       │  │
│  └──────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────┘
```

O áudio capturado entra na sala como **mais uma faixa de microfone**, rotulada
`SeuNome (jogo)`. `exclude = true` captura **tudo menos** o processo escolhido
(ex: menos o Discord); `exclude = false` captura **só** a árvore daquele processo.

---

## Pré-requisitos (máquina de quem compila)

| | |
|---|---|
| **Rust** | https://rustup.rs — instala `rustc` + `cargo` |
| **MSVC Build Tools** | "Visual Studio Build Tools" com **Desktop development with C++** (o linker do Windows). https://visualstudio.microsoft.com/visual-cpp-build-tools/ |
| **Tauri CLI** | `cargo install tauri-cli --version "^2"`  (ou use `npm i` e `npx tauri`) |
| **WebView2** | já vem no Windows 11. No 10, o instalador gerado baixa sozinho. |

Windows **10 build 20348+ ou Windows 11** — a API de captura por processo não
existe antes disso.

---

## Rodar em desenvolvimento

```bash
cd sharescreen-desktop
cargo tauri dev
```

Primeira vez: o WebView2 vai perguntar "Permitir microfone?" e pedir permissão de
tela — **clique em Permitir** (ele lembra nas próximas).

### Sem encarar o WASAPI ainda: modo mock

O módulo de captura (`src-tauri/src/audio_capture.rs`) é a parte que mais
provavelmente precisa de ajuste no primeiro build (a crate `windows` muda de
assinatura entre versões). Pra testar **toda a ponte** (backend → WebSocket →
AudioWorklet → WebRTC) sem isso, tem um gerador de tom de teste:

```bash
cargo tauri dev --features mock-audio
```

Clique em "🎮 Áudio do jogo" → escolha qualquer processo → "Capturar". Os outros
na sala devem ouvir uma senoide de 220 Hz. Se ouvirem, a ponte está fechada e só
falta o WASAPI.

---

## Gerar o instalador

```bash
# 1) ícones de verdade (uma vez) — gera todos os tamanhos + .ico a partir do PNG
cargo tauri icon app-icon.png

# 2) build
cargo tauri build
```

Sai em `src-tauri/target/release/bundle/nsis/Pérola Negra_0.2.0_x64-setup.exe`.

**Sem assinatura de código**, o Windows mostra *"Windows protegeu seu PC"* →
**Mais informações → Executar assim mesmo**. Uma vez por pessoa. Pra remover isso
precisa de um certificado (Azure Trusted Signing é o mais barato hoje).

---

## Estrutura

```
sharescreen-desktop/
├── app-icon.png              fonte do ícone (troque por um seu, 1024×1024)
├── src/
│   └── index.html            o app web + bloco "áudio do jogo" (gate em window.__TAURI__)
└── src-tauri/
    ├── tauri.conf.json       janela, bundle, aponta frontendDist p/ ../src
    ├── capabilities/default.json
    ├── Cargo.toml
    └── src/
        ├── main.rs           entrypoint
        ├── lib.rs            builder Tauri + comandos invoke
        ├── bridge.rs         broadcast PCM → WebSocket local (cross-platform)
        ├── processes.rs      lista processos com sessão de áudio (WASAPI)
        └── audio_capture.rs  ★ WASAPI Process Loopback — a parte delicada
```

---

## O que ajustar no primeiro `cargo build`

Tudo isolado em `audio_capture.rs`, marcado com `// AJUSTE:`. Os suspeitos:

1. **`PROPVARIANT`** — em `windows` 0.58 fica em `Win32::System::Variant`. Se os
   campos `Anonymous.Anonymous.vt` / `.blob` não baterem, veja
   `PROPVARIANT` na doc da versão que o `cargo` baixou (`cargo doc -p windows --open`).
2. **`IActivateAudioInterfaceCompletionHandler_Impl::ActivateCompleted`** — o
   argumento é `windows_core::Ref<'_, T>` nas versões novas, `Option<&T>` nas
   antigas.
3. **`AUDIOCLIENT_ACTIVATION_PARAMS_0`** — nome do campo da union
   (`ProcessLoopbackParams`).
4. **`bool` vs `BOOL`** — se `CreateEventW` / `OpenProcess` /
   `QueryFullProcessImageNameW` reclamarem, troque `false` por `false.into()` ou
   `windows::core::BOOL(0)`.
5. **`IMMDevice::Activate`** (em `processes.rs`) — nas versões novas é
   `Activate::<T>(CLSCTX_ALL, None)`; nas antigas tem out-param.
6. Se travar de vez, rode com `--features mock-audio` enquanto resolve — o resto
   do app funciona.

Referência canônica (C++): sample **ApplicationLoopback** da Microsoft —
<https://github.com/microsoft/Windows-classic-samples/tree/main/Samples/ApplicationLoopback>

---

## Notas

- **Latência** do áudio do jogo: ~50–150 ms. Ok pra assistir; não serve pra
  música sincronizada.
- O `src/index.html` continua funcionando **aberto direto no navegador** — o
  bloco de áudio por processo só aparece dentro do app. Dá pra manter a versão
  hospedada a partir desse mesmo arquivo.
- A sala, o broker PeerJS, STUN/TURN: **iguais à versão web**. O app não muda
  nada de rede.
- macOS/Linux: o app compila mas `list_audio_processes` / captura retornam erro
  (a API é do Windows). A parte de tela/voz/chat funciona.
