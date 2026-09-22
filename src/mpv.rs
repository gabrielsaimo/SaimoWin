//! Player de vídeo: o mpv, carregado da DLL que vai junto no pacote.
//!
//! A imagem não vai para uma janela do mpv: ele desenha dentro do mesmo
//! framebuffer OpenGL da interface (API de render do libmpv), e a lista de
//! canais é desenhada por cima. Sem isso seriam duas janelas disputando a tela.
//!
//! As funções são resolvidas em tempo de execução (`libloading`), então não é
//! preciso biblioteca de importação do mpv para compilar do Mac para o Windows.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

#[allow(non_camel_case_types)]
type mpv_handle = c_void;
#[allow(non_camel_case_types)]
type mpv_render_context = c_void;

#[repr(C)]
struct MpvRenderParam {
    tipo: c_int,
    dados: *mut c_void,
}

#[repr(C)]
struct MpvOpenGLInitParams {
    get_proc_address: extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    get_proc_address_ctx: *mut c_void,
}

#[repr(C)]
struct MpvOpenGLFbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    internal_format: c_int,
}

#[repr(C)]
struct MpvEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct MpvEventEndFile {
    reason: c_int,
    error: c_int,
}

const PARAM_INVALID: c_int = 0;
const PARAM_API_TYPE: c_int = 1;
const PARAM_OPENGL_INIT_PARAMS: c_int = 2;
const PARAM_OPENGL_FBO: c_int = 3;
const PARAM_FLIP_Y: c_int = 4;

const EVENT_END_FILE: c_int = 7;
const EVENT_FILE_LOADED: c_int = 8;
const EVENT_PLAYBACK_RESTART: c_int = 21;
const END_FILE_ERROR: c_int = 4;

/// O que a tela precisa saber do player, sem falar C.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Aviso {
    /// O arquivo abriu e o vídeo começou.
    Tocando,
    /// A fonte morreu: hora de tentar a próxima.
    Falhou,
    /// Acabou sozinho (VOD) ou o servidor cortou.
    Fim,
}

struct Simbolos {
    _lib: libloading::Library,
    create: unsafe extern "C" fn() -> *mut mpv_handle,
    initialize: unsafe extern "C" fn(*mut mpv_handle) -> c_int,
    set_option_string: unsafe extern "C" fn(*mut mpv_handle, *const c_char, *const c_char) -> c_int,
    set_property_string: unsafe extern "C" fn(*mut mpv_handle, *const c_char, *const c_char) -> c_int,
    get_property_string: unsafe extern "C" fn(*mut mpv_handle, *const c_char) -> *mut c_char,
    free: unsafe extern "C" fn(*mut c_void),
    command: unsafe extern "C" fn(*mut mpv_handle, *const *const c_char) -> c_int,
    wait_event: unsafe extern "C" fn(*mut mpv_handle, f64) -> *mut MpvEvent,
    wakeup: unsafe extern "C" fn(*mut mpv_handle),
    terminate_destroy: unsafe extern "C" fn(*mut mpv_handle),
    render_context_create:
        unsafe extern "C" fn(*mut *mut mpv_render_context, *mut mpv_handle, *mut MpvRenderParam) -> c_int,
    render_context_render: unsafe extern "C" fn(*mut mpv_render_context, *mut MpvRenderParam) -> c_int,
    render_context_free: unsafe extern "C" fn(*mut mpv_render_context),
}

/// Ponteiro do mpv atravessando threads: a API do libmpv é feita para isso.
struct Ponteiro(*mut mpv_handle);
unsafe impl Send for Ponteiro {}
unsafe impl Sync for Ponteiro {}

struct Render(*mut mpv_render_context);
unsafe impl Send for Render {}
unsafe impl Sync for Render {}

pub struct Mpv {
    simbolos: Arc<Simbolos>,
    handle: Arc<Ponteiro>,
    render: Mutex<Option<Render>>,
    erro_render: Mutex<Option<String>>,
    avisos: Receiver<Aviso>,
    _emissor: Sender<Aviso>,
}

#[derive(Debug, Clone)]
pub struct Faixa {
    pub id: String,
    pub titulo: String,
    pub selecionada: bool,
}

// Ponteiros só são tocados por estes métodos, todos sincronizados pelo próprio
// libmpv, que é thread-safe.
unsafe impl Send for Mpv {}
unsafe impl Sync for Mpv {}

/// Endereço das funções de OpenGL, que no Windows moram em dois lugares: as do
/// OpenGL 1.1 na opengl32.dll e o resto no driver, via wglGetProcAddress.
extern "C" fn endereco_gl(_ctx: *mut c_void, nome: *const c_char) -> *mut c_void {
    #[cfg(windows)]
    unsafe {
        extern "system" {
            fn wglGetProcAddress(name: *const c_char) -> *mut c_void;
            fn LoadLibraryA(name: *const c_char) -> *mut c_void;
            fn GetProcAddress(modulo: *mut c_void, name: *const c_char) -> *mut c_void;
        }
        // O wglGetProcAddress não devolve só nulo quando não acha: a própria
        // documentação da Microsoft lista 1, 2, 3 e -1 como valores de falha.
        // O -1 passava pela conferência antiga e ia para o mpv como endereço
        // bom — e o mpv o chamava. Toda função do OpenGL 1.1 (glClear,
        // glViewport, glGetString) cai nesse caso em boa parte dos drivers.
        let falhou = |p: *mut c_void| {
            let valor = p as usize;
            p.is_null() || valor <= 3 || valor == usize::MAX
        };
        let achado = wglGetProcAddress(nome);
        if !falhou(achado) {
            return achado;
        }
        // LoadLibraryA e não GetModuleHandleA: se a opengl32 ainda não estiver
        // carregada no processo, o handle viria nulo e ficaria sem nada.
        let opengl32 = LoadLibraryA(b"opengl32.dll\0".as_ptr() as *const c_char);
        if opengl32.is_null() {
            return std::ptr::null_mut();
        }
        GetProcAddress(opengl32, nome)
    }
    // Fora do Windows o endereço sai do próprio processo: o OpenGL já está
    // carregado pela janela, e é assim que dá para rodar este mesmo caminho
    // aqui no Mac para conferir o desenho sem depender de uma máquina Windows.
    #[cfg(not(windows))]
    unsafe {
        extern "C" {
            fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        }
        const RTLD_DEFAULT: *mut c_void = std::ptr::null_mut();
        dlsym(RTLD_DEFAULT, nome)
    }
}

impl Mpv {
    /// Carrega a DLL ao lado do executável e sobe um mpv sem janela própria.
    pub fn novo(caminho_da_dll: &std::path::Path) -> Result<Self, String> {
        let simbolos = unsafe {
            let lib = libloading::Library::new(caminho_da_dll)
                .map_err(|e| format!("não achei o mpv ({}): {e}", caminho_da_dll.display()))?;
            macro_rules! pega {
                ($nome:literal) => {
                    *lib.get(concat!($nome, "\0").as_bytes())
                        .map_err(|e| format!("mpv sem {}: {e}", $nome))?
                };
            }
            Simbolos {
                create: pega!("mpv_create"),
                initialize: pega!("mpv_initialize"),
                set_option_string: pega!("mpv_set_option_string"),
                set_property_string: pega!("mpv_set_property_string"),
                get_property_string: pega!("mpv_get_property_string"),
                free: pega!("mpv_free"),
                command: pega!("mpv_command"),
                wait_event: pega!("mpv_wait_event"),
                wakeup: pega!("mpv_wakeup"),
                terminate_destroy: pega!("mpv_terminate_destroy"),
                render_context_create: pega!("mpv_render_context_create"),
                render_context_render: pega!("mpv_render_context_render"),
                render_context_free: pega!("mpv_render_context_free"),
                _lib: lib,
            }
        };
        let simbolos = Arc::new(simbolos);

        let handle = unsafe { (simbolos.create)() };
        if handle.is_null() {
            return Err("mpv_create falhou".into());
        }

        let opcoes = [
            // Sem janela própria: quem desenha é a API de render.
            ("vo", "libmpv"),
            // A API de render usa OpenGL/WGL. No Windows, decodificação
            // direta por D3D11 exige ANGLE; alguns drivers aceitam a faixa mas
            // entregam somente quadros pretos. O modo copy continua usando a
            // GPU para decodificar e traz o quadro para a memória antes de
            // desenhá-lo, funcionando também nesses aparelhos.
            ("hwdec", "auto-copy-safe"),
            // Ao vivo: perder quadro é melhor que atrasar a imagem.
            ("profile", "low-latency"),
            ("cache", "yes"),
            ("demuxer-max-bytes", "64MiB"),
            ("network-timeout", "20"),
            ("user-agent", crate::AGENTE),
            // Há origem que recusa quem não manda "Accept": o EmbedPlayer, dos
            // doramas e animes novos, responde 200 com "security error" no
            // lugar da playlist.
            ("http-header-fields", "Accept: */*"),
            // O EmbedPlayer serve os pedaços do HLS com extensão ".js", e o
            // FFmpeg os recusa por isso: "not in allowed_segment_extensions".
            // Sem estas duas opções a duração do filme sai vazia — e é dela
            // que depende guardar onde a pessoa parou.
            ("demuxer-lavf-o-add", "allowed_segment_extensions=ALL"),
            ("demuxer-lavf-o-add", "extension_picky=0"),
            ("tls-verify", "no"),
            ("keep-open", "no"),
            ("idle", "yes"),
            ("osc", "no"),
            ("input-default-bindings", "no"),
            ("terminal", "no"),
        ];
        for (nome, valor) in opcoes {
            let n = CString::new(nome).unwrap();
            let v = CString::new(valor).unwrap();
            unsafe { (simbolos.set_option_string)(handle, n.as_ptr(), v.as_ptr()) };
        }
        if unsafe { (simbolos.initialize)(handle) } < 0 {
            return Err("mpv_initialize falhou".into());
        }

        let handle = Arc::new(Ponteiro(handle));
        let (emissor, avisos) = channel();

        // Thread só para os avisos do mpv: sem ela, uma fonte morta não teria
        // como avisar a tela para pular para a próxima.
        {
            let simbolos = simbolos.clone();
            let handle = handle.clone();
            let emissor = emissor.clone();
            std::thread::spawn(move || loop {
                let evento = unsafe { (simbolos.wait_event)(handle.0, 1.0) };
                if evento.is_null() {
                    continue;
                }
                let evento = unsafe { &*evento };
                let aviso = match evento.event_id {
                    EVENT_FILE_LOADED | EVENT_PLAYBACK_RESTART => Some(Aviso::Tocando),
                    EVENT_END_FILE => {
                        let fim = unsafe { &*(evento.data as *const MpvEventEndFile) };
                        Some(if fim.reason == END_FILE_ERROR { Aviso::Falhou } else { Aviso::Fim })
                    }
                    _ => None,
                };
                if let Some(aviso) = aviso {
                    if emissor.send(aviso).is_err() {
                        return;
                    }
                }
            });
        }

        Ok(Mpv {
            simbolos,
            handle,
            render: Mutex::new(None),
            erro_render: Mutex::new(None),
            avisos,
            _emissor: emissor,
        })
    }

    /// Liga o mpv ao contexto OpenGL da janela. Só pode ser chamado na thread
    /// que desenha.
    pub fn ligar_video(&self) -> Result<(), String> {
        let mut render = self.render.lock().unwrap();
        if render.is_some() {
            return Ok(());
        }
        let mut init = MpvOpenGLInitParams {
            get_proc_address: endereco_gl,
            get_proc_address_ctx: std::ptr::null_mut(),
        };
        let api = CString::new("opengl").unwrap();
        let mut params = [
            MpvRenderParam { tipo: PARAM_API_TYPE, dados: api.as_ptr() as *mut c_void },
            MpvRenderParam {
                tipo: PARAM_OPENGL_INIT_PARAMS,
                dados: &mut init as *mut _ as *mut c_void,
            },
            MpvRenderParam { tipo: PARAM_INVALID, dados: std::ptr::null_mut() },
        ];
        let mut contexto: *mut mpv_render_context = std::ptr::null_mut();
        let erro = unsafe {
            (self.simbolos.render_context_create)(&mut contexto, self.handle.0, params.as_mut_ptr())
        };
        if erro < 0 || contexto.is_null() {
            let mensagem = format!("mpv_render_context_create falhou ({erro})");
            *self.erro_render.lock().unwrap() = Some(mensagem.clone());
            return Err(mensagem);
        }
        *self.erro_render.lock().unwrap() = None;
        *render = Some(Render(contexto));
        Ok(())
    }

    pub fn erro_de_video(&self) -> Option<String> {
        self.erro_render.lock().unwrap().clone()
    }

    /// Desenha o quadro atual no framebuffer que já está ligado.
    pub fn desenhar(&self, fbo: i32, largura: i32, altura: i32) {
        let render = self.render.lock().unwrap();
        let Some(contexto) = render.as_ref() else { return };
        let mut alvo = MpvOpenGLFbo { fbo, w: largura, h: altura, internal_format: 0 };
        // A imagem sai de cabeça para baixo sem isto: o OpenGL conta a altura
        // de baixo para cima, e o egui de cima para baixo.
        let mut inverter: c_int = 1;
        let mut params = [
            MpvRenderParam { tipo: PARAM_OPENGL_FBO, dados: &mut alvo as *mut _ as *mut c_void },
            MpvRenderParam { tipo: PARAM_FLIP_Y, dados: &mut inverter as *mut _ as *mut c_void },
            MpvRenderParam { tipo: PARAM_INVALID, dados: std::ptr::null_mut() },
        ];
        unsafe { (self.simbolos.render_context_render)(contexto.0, params.as_mut_ptr()) };
    }

    pub fn tocar(&self, url: &str, referer: Option<&str>, agente: Option<&str>) {
        self.propriedade("referrer", referer.unwrap_or(""));
        self.propriedade("user-agent", agente.unwrap_or(crate::AGENTE));
        // Alguns masters HLS são servidos como text/plain e terminam em .txt.
        // Sem a opção por arquivo o FFmpeg os detecta como terminal ANSI e o
        // MPV nunca chega às variantes, áudios ou legendas.
        if url.split('?').next().unwrap_or(url).to_ascii_lowercase().ends_with(".txt") {
            self.comando(&["loadfile", url, "replace", "-1", "demuxer-lavf-format=hls"]);
        } else {
            self.comando(&["loadfile", url, "replace"]);
        }
    }

    /// Faixas que o MPV encontrou no master (variantes de vídeo, idiomas e
    /// legendas). Os nomes vêm do próprio manifesto quando existem.
    pub fn faixas(&self, tipo: &str) -> Vec<Faixa> {
        let total = self.ler("track-list/count").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        (0..total).filter_map(|i| {
            if self.ler(&format!("track-list/{i}/type")).as_deref() != Some(tipo) { return None; }
            let id = self.ler(&format!("track-list/{i}/id"))?;
            let idioma = self.ler(&format!("track-list/{i}/lang")).unwrap_or_default();
            let nome = self.ler(&format!("track-list/{i}/title")).unwrap_or_default();
            let altura = self.ler(&format!("track-list/{i}/demux-h")).unwrap_or_default();
            let titulo = if tipo == "video" && !altura.is_empty() && altura != "0" {
                format!("{altura}p")
            } else if !nome.is_empty() && !idioma.is_empty() {
                format!("{nome} · {idioma}")
            } else if !nome.is_empty() {
                nome
            } else if !idioma.is_empty() {
                idioma
            } else {
                format!("Faixa {id}")
            };
            let selecionada = self.ler(&format!("track-list/{i}/selected")).as_deref() == Some("yes");
            Some(Faixa { id, titulo, selecionada })
        }).collect()
    }

    pub fn escolher_faixa(&self, tipo: &str, id: Option<&str>) {
        let propriedade = match tipo { "video" => "vid", "audio" => "aid", "sub" => "sid", _ => return };
        self.propriedade(propriedade, id.unwrap_or(if tipo == "sub" { "no" } else { "auto" }));
    }

    /// Pula para um ponto do arquivo (em segundos).
    pub fn ir_para(&self, segundos: f64) {
        self.comando(&["seek", &format!("{segundos:.0}"), "absolute"]);
    }

    /// Posição e duração do que está tocando, quando o mpv já sabe.
    /// Onde o vídeo está e quanto dura. Duração 0 quando a fonte não a diz —
    /// canal ao vivo, e também o HLS de origem que não declara o fim.
    pub fn posicao(&self) -> Option<(f64, f64)> {
        let posicao = self.ler("time-pos")?.parse::<f64>().ok()?;
        let duracao = self
            .ler("duration")
            .and_then(|d| d.parse::<f64>().ok())
            .filter(|d| *d > 0.0)
            .unwrap_or(0.0);
        Some((posicao, duracao))
    }

    pub fn parar(&self) {
        self.comando(&["stop"]);
    }

    pub fn pausa(&self, pausado: bool) {
        self.propriedade("pause", if pausado { "yes" } else { "no" });
    }

    pub fn volume(&self, valor: i32) {
        self.propriedade("volume", &valor.to_string());
    }

    pub fn mudo(&self, calado: bool) {
        self.propriedade("mute", if calado { "yes" } else { "no" });
    }

    /// Texto de uma propriedade do mpv, ou nada quando ela não existe agora.
    pub fn ler(&self, nome: &str) -> Option<String> {
        let n = CString::new(nome).ok()?;
        unsafe {
            let bruto = (self.simbolos.get_property_string)(self.handle.0, n.as_ptr());
            if bruto.is_null() {
                return None;
            }
            let texto = CStr::from_ptr(bruto).to_string_lossy().into_owned();
            (self.simbolos.free)(bruto as *mut c_void);
            Some(texto)
        }
    }

    pub fn avisos(&self) -> impl Iterator<Item = Aviso> + '_ {
        self.avisos.try_iter()
    }

    /// Qualquer propriedade do mpv ("speed", "panscan"…).
    pub fn definir(&self, nome: &str, valor: &str) {
        self.propriedade(nome, valor);
    }

    fn propriedade(&self, nome: &str, valor: &str) {
        let (Ok(n), Ok(v)) = (CString::new(nome), CString::new(valor)) else { return };
        unsafe { (self.simbolos.set_property_string)(self.handle.0, n.as_ptr(), v.as_ptr()) };
    }

    fn comando(&self, partes: &[&str]) {
        let textos: Vec<CString> = partes.iter().filter_map(|p| CString::new(*p).ok()).collect();
        let mut ponteiros: Vec<*const c_char> = textos.iter().map(|t| t.as_ptr()).collect();
        ponteiros.push(std::ptr::null());
        unsafe { (self.simbolos.command)(self.handle.0, ponteiros.as_ptr()) };
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        if let Some(render) = self.render.lock().unwrap().take() {
            unsafe { (self.simbolos.render_context_free)(render.0) };
        }
        unsafe {
            (self.simbolos.wakeup)(self.handle.0);
            (self.simbolos.terminate_destroy)(self.handle.0);
        }
    }
}
