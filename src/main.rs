//! Saimo TV para Windows.
//!
//! Aplicativo nativo: a janela, a lista e o letreiro são desenhados pela GPU
//! (egui sobre OpenGL) e o vídeo é do mpv, que desenha no mesmo framebuffer.
//! Não há navegador nem página HTML em lugar nenhum.
//!
//! A lista é a mesma dos outros aplicativos, baixada do repositório publicado.

// Sem console preto atrás da janela quando alguém abre pelo Explorer.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod atualizacao;
mod catalogo;
mod mpv;
mod rede;
mod telemetria;

use catalogo::Canal;
use eframe::egui;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

pub const VERSAO: &str = env!("CARGO_PKG_VERSION");
pub const AGENTE: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";
/// Código que revela os canais restritos, o mesmo dos outros aplicativos.
const CODIGO: &str = "1010";
/// Uma fonte que não abre neste tempo está fora: vai para a próxima.
const ESPERA_DA_FONTE: Duration = Duration::from_secs(14);
const SUMIR_LETREIRO: Duration = Duration::from_secs(6);

fn main() -> eframe::Result<()> {
    let opcoes = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Saimo TV")
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([800.0, 480.0]),
        vsync: true,
        ..Default::default()
    };
    eframe::run_native("Saimo TV", opcoes, Box::new(|cc| Ok(Box::new(App::novo(cc)))))
}

/// Recados que chegam das threads de fundo.
enum Recado {
    Catalogo(Vec<Canal>),
    Restritos(Vec<Canal>),
    Logo(String, egui::ColorImage),
    Atualizacao(atualizacao::Versao),
}

struct Tocando {
    canal: usize,
    fonte: usize,
    desde: Instant,
    confirmado: bool,
}

struct App {
    mpv: Option<std::sync::Arc<mpv::Mpv>>,
    erro_do_mpv: Option<String>,
    canais: Vec<Canal>,
    restritos: Vec<Canal>,
    liberado: bool,
    favoritos: Vec<String>,
    busca: String,
    foco: usize,
    tocando: Option<Tocando>,
    lista_aberta: bool,
    letreiro_ate: Instant,
    digitado: String,
    digitado_em: Instant,
    volume: i32,
    mudo: bool,
    pausado: bool,
    logos: HashMap<String, Option<egui::TextureHandle>>,
    pedidos_de_logo: Vec<String>,
    recados: Receiver<Recado>,
    emissor: Sender<Recado>,
    nova_versao: Option<atualizacao::Versao>,
    ultima_checagem: Instant,
    aviso: Option<(String, Instant)>,
}

impl App {
    fn novo(cc: &eframe::CreationContext<'_>) -> Self {
        let (emissor, recados) = channel();
        let mut estilo = (*cc.egui_ctx.style()).clone();
        estilo.visuals = egui::Visuals::dark();
        estilo.visuals.panel_fill = egui::Color32::from_rgb(10, 10, 12);
        cc.egui_ctx.set_style(estilo);

        let (mpv, erro_do_mpv) = match mpv::Mpv::novo(&caminho_do_mpv()) {
            Ok(m) => (Some(std::sync::Arc::new(m)), None),
            Err(e) => (None, Some(e)),
        };

        telemetria::iniciar();

        let canais = catalogo::em_cache("catalogo.txt");
        let restritos = catalogo::em_cache("restritos.txt");
        let favoritos = ler_favoritos();

        // Lista publicada e versão nova, sem segurar a abertura.
        for (arquivo, monta) in [
            ("catalogo.txt", Recado::Catalogo as fn(Vec<Canal>) -> Recado),
            ("restritos.txt", Recado::Restritos as fn(Vec<Canal>) -> Recado),
        ] {
            let emissor = emissor.clone();
            std::thread::spawn(move || {
                if let Some(lista) = catalogo::baixar(arquivo) {
                    let _ = emissor.send(monta(lista));
                }
            });
        }
        {
            let emissor = emissor.clone();
            std::thread::spawn(move || {
                if let Some(v) = atualizacao::procurar() {
                    let _ = emissor.send(Recado::Atualizacao(v));
                }
            });
        }

        let mut app = App {
            mpv,
            erro_do_mpv,
            canais,
            restritos,
            liberado: false,
            favoritos,
            busca: String::new(),
            foco: 0,
            tocando: None,
            lista_aberta: true,
            letreiro_ate: Instant::now(),
            digitado: String::new(),
            digitado_em: Instant::now(),
            volume: 100,
            mudo: false,
            pausado: false,
            logos: HashMap::new(),
            pedidos_de_logo: Vec::new(),
            recados,
            emissor,
            nova_versao: None,
            ultima_checagem: Instant::now(),
            aviso: None,
        };
        app.reordenar();
        app
    }

    /// A lista que está na tela: catálogo (mais os restritos quando liberados),
    /// na ordem das seções, filtrada pela busca.
    fn visiveis(&self) -> Vec<usize> {
        let busca = catalogo::chave_de_ordem(self.busca.trim());
        self.canais
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                busca.is_empty()
                    || catalogo::chave_de_ordem(&c.nome).contains(&busca)
                    || catalogo::chave_de_ordem(c.secao()).contains(&busca)
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn reordenar(&mut self) {
        let atual = self.tocando.as_ref().and_then(|t| self.canais.get(t.canal)).map(|c| c.nome.clone());
        let mut lista = self.canais.clone();
        if self.liberado {
            let ja: Vec<String> = lista.iter().map(|c| c.nome.clone()).collect();
            lista.extend(self.restritos.iter().filter(|r| !ja.contains(&r.nome)).cloned());
        } else {
            lista.retain(|c| c.secao() != "Adulto");
        }
        catalogo::ordenar(&mut lista, &self.favoritos);
        self.canais = lista;
        if let Some(nome) = atual {
            if let Some(novo) = self.canais.iter().position(|c| c.nome == nome) {
                if let Some(t) = self.tocando.as_mut() {
                    t.canal = novo;
                }
            }
        }
        self.foco = self.foco.min(self.canais.len().saturating_sub(1));
    }

    fn trocar_lista(&mut self, base: Vec<Canal>, restrita: bool) {
        if restrita {
            self.restritos = base;
        } else {
            // A lista de canais visíveis é a de trabalho; guardar a publicada
            // pura evita duplicar restrito a cada recarga.
            self.canais = base;
        }
        self.reordenar();
    }

    fn tocar(&mut self, canal: usize, fonte: usize, nova: bool) {
        let Some(mpv) = self.mpv.as_ref() else { return };
        let Some(c) = self.canais.get(canal) else { return };
        let Some(f) = c.fontes.get(fonte) else {
            telemetria::caiu(&c.nome, c.fontes.len());
            self.aviso = Some((format!("{}: nenhuma fonte abriu", c.nome), Instant::now()));
            self.tocando = None;
            return;
        };
        mpv.tocar(&f.url, f.referer.as_deref(), f.agente.as_deref());
        mpv.pausa(false);
        self.pausado = false;
        telemetria::comecou(&c.nome, &f.url, fonte + 1, nova);
        self.tocando = Some(Tocando { canal, fonte, desde: Instant::now(), confirmado: false });
        self.letreiro_ate = Instant::now() + SUMIR_LETREIRO;
    }

    fn proxima_fonte(&mut self, motivo: &str) {
        let Some(t) = self.tocando.as_ref() else { return };
        let (canal, fonte) = (t.canal, t.fonte);
        let (nome, url, total) = {
            let Some(c) = self.canais.get(canal) else { return };
            (c.nome.clone(), c.fontes.get(fonte).map(|f| f.url.clone()).unwrap_or_default(), c.fontes.len())
        };
        telemetria::falhou(&nome, &url, fonte + 1, motivo);
        if fonte + 1 >= total {
            telemetria::caiu(&nome, total);
            telemetria::parou();
            self.aviso = Some((format!("{nome}: nenhuma das {total} fontes abriu"), Instant::now()));
            self.tocando = None;
            if let Some(mpv) = self.mpv.as_ref() {
                mpv.parar();
            }
            return;
        }
        self.aviso = Some((format!("{nome}: trocando para a fonte {}", fonte + 2), Instant::now()));
        self.tocar(canal, fonte + 1, false);
    }

    fn escolher_fonte_seguinte(&mut self) {
        let Some(t) = self.tocando.as_ref() else { return };
        let (canal, fonte) = (t.canal, t.fonte);
        let total = self.canais.get(canal).map(|c| c.fontes.len()).unwrap_or(0);
        if total <= 1 {
            return;
        }
        self.tocar(canal, (fonte + 1) % total, false);
    }

    fn cuidar_do_mpv(&mut self, ctx: &egui::Context) {
        let avisos: Vec<mpv::Aviso> = self.mpv.as_ref().map(|m| m.avisos().collect()).unwrap_or_default();
        for aviso in avisos {
            match aviso {
                mpv::Aviso::Tocando => {
                    if let Some(t) = self.tocando.as_mut() {
                        if !t.confirmado {
                            t.confirmado = true;
                            let ms = t.desde.elapsed().as_millis();
                            let (canal, fonte) = (t.canal, t.fonte);
                            if let Some(c) = self.canais.get(canal) {
                                let url = c.fontes.get(fonte).map(|f| f.url.clone()).unwrap_or_default();
                                telemetria::tocou(&c.nome, &url, fonte + 1, ms);
                            }
                        }
                    }
                }
                mpv::Aviso::Falhou => self.proxima_fonte("mpv: erro na fonte"),
                mpv::Aviso::Fim => self.proxima_fonte("a transmissão terminou"),
            }
        }
        // Fonte que nunca abre não manda erro nenhum: o relógio é quem decide.
        let estourou = self
            .tocando
            .as_ref()
            .map(|t| !t.confirmado && t.desde.elapsed() > ESPERA_DA_FONTE)
            .unwrap_or(false);
        if estourou {
            self.proxima_fonte("a fonte não abriu em 14 s");
        }
        if self.tocando.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn cuidar_dos_recados(&mut self, ctx: &egui::Context) {
        while let Ok(recado) = self.recados.try_recv() {
            match recado {
                Recado::Catalogo(lista) => self.trocar_lista(lista, false),
                Recado::Restritos(lista) => self.trocar_lista(lista, true),
                Recado::Logo(url, imagem) => {
                    let textura = ctx.load_texture(&url, imagem, egui::TextureOptions::LINEAR);
                    self.logos.insert(url, Some(textura));
                }
                Recado::Atualizacao(v) => self.nova_versao = Some(v),
            }
        }
        if self.ultima_checagem.elapsed() > Duration::from_secs(3600) {
            self.ultima_checagem = Instant::now();
            let emissor = self.emissor.clone();
            std::thread::spawn(move || {
                if let Some(v) = atualizacao::procurar() {
                    let _ = emissor.send(Recado::Atualizacao(v));
                }
            });
        }
        telemetria::no_relogio();
    }

    fn logo(&mut self, url: &str) -> Option<egui::TextureHandle> {
        if let Some(guardado) = self.logos.get(url) {
            return guardado.clone();
        }
        self.logos.insert(url.to_string(), None);
        self.pedidos_de_logo.push(url.to_string());
        None
    }

    fn baixar_logos_pendentes(&mut self) {
        for url in std::mem::take(&mut self.pedidos_de_logo) {
            let emissor = self.emissor.clone();
            std::thread::spawn(move || {
                let Some(bytes) = rede::bytes(&url, 4 * 1024 * 1024) else { return };
                let Ok(imagem) = image::load_from_memory(&bytes) else { return };
                let imagem = imagem.thumbnail(96, 96).to_rgba8();
                let tamanho = [imagem.width() as usize, imagem.height() as usize];
                let cores = egui::ColorImage::from_rgba_unmultiplied(tamanho, imagem.as_raw());
                let _ = emissor.send(Recado::Logo(url, cores));
            });
        }
    }

    fn teclado(&mut self, ctx: &egui::Context) {
        let (teclas, texto_digitado) = ctx.input(|i| {
            (i.events.clone(), i.events.iter().filter_map(|e| match e {
                egui::Event::Text(t) => Some(t.clone()),
                _ => None,
            }).collect::<String>())
        });
        let escrevendo = ctx.memory(|m| m.focused().is_some());

        // Número de canal, e o código que revela a lista restrita.
        if !escrevendo {
            for c in texto_digitado.chars().filter(|c| c.is_ascii_digit()) {
                if self.digitado_em.elapsed() > Duration::from_millis(1500) {
                    self.digitado.clear();
                }
                self.digitado.push(c);
                self.digitado_em = Instant::now();
            }
        }
        if !self.digitado.is_empty() && self.digitado_em.elapsed() > Duration::from_millis(1200) {
            let digitado = std::mem::take(&mut self.digitado);
            if digitado == CODIGO {
                self.liberado = !self.liberado;
                let aviso = if self.liberado { "lista completa liberada" } else { "lista restrita escondida" };
                self.aviso = Some((aviso.into(), Instant::now()));
                self.reordenar();
            } else if let Ok(numero) = digitado.parse::<usize>() {
                if numero >= 1 && numero <= self.canais.len() {
                    self.tocar(numero - 1, 0, true);
                    self.foco = numero - 1;
                    self.lista_aberta = false;
                }
            }
        }

        for evento in teclas {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = evento else { continue };
            if escrevendo && !matches!(key, egui::Key::Escape | egui::Key::Enter | egui::Key::ArrowDown) {
                continue;
            }
            let visiveis = self.visiveis();
            match key {
                egui::Key::ArrowDown | egui::Key::ArrowUp => {
                    if !self.lista_aberta {
                        self.lista_aberta = true;
                        continue;
                    }
                    let atual = visiveis.iter().position(|i| *i == self.foco).unwrap_or(0);
                    let novo = if key == egui::Key::ArrowDown {
                        (atual + 1).min(visiveis.len().saturating_sub(1))
                    } else {
                        atual.saturating_sub(1)
                    };
                    if let Some(i) = visiveis.get(novo) {
                        self.foco = *i;
                    }
                }
                egui::Key::Enter => {
                    if escrevendo {
                        ctx.memory_mut(|m| m.request_focus(egui::Id::NULL));
                    }
                    if let Some(i) = visiveis.iter().find(|i| **i == self.foco).copied() {
                        self.tocar(i, 0, true);
                        self.lista_aberta = false;
                    }
                }
                egui::Key::Escape => {
                    if escrevendo {
                        ctx.memory_mut(|m| m.request_focus(egui::Id::NULL));
                    } else if ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                    } else {
                        self.lista_aberta = !self.lista_aberta;
                    }
                }
                egui::Key::F if !modifiers.ctrl => {
                    let cheia = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!cheia));
                }
                egui::Key::F11 => {
                    let cheia = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!cheia));
                }
                egui::Key::Space => {
                    self.pausado = !self.pausado;
                    if let Some(mpv) = self.mpv.as_ref() {
                        mpv.pausa(self.pausado);
                    }
                }
                egui::Key::M => {
                    self.mudo = !self.mudo;
                    if let Some(mpv) = self.mpv.as_ref() {
                        mpv.mudo(self.mudo);
                    }
                }
                egui::Key::ArrowRight => self.escolher_fonte_seguinte(),
                egui::Key::Plus | egui::Key::Equals => self.mudar_volume(5),
                egui::Key::Minus => self.mudar_volume(-5),
                egui::Key::L => {
                    if let Some(canal) = self.canais.get(self.foco).map(|c| c.nome.clone()) {
                        self.favoritar(canal);
                    }
                }
                _ => {}
            }
        }
    }

    fn mudar_volume(&mut self, passo: i32) {
        self.volume = (self.volume + passo).clamp(0, 130);
        if let Some(mpv) = self.mpv.as_ref() {
            mpv.volume(self.volume);
        }
        self.aviso = Some((format!("volume {}%", self.volume), Instant::now()));
    }

    fn favoritar(&mut self, canal: String) {
        if let Some(pos) = self.favoritos.iter().position(|f| *f == canal) {
            self.favoritos.remove(pos);
        } else {
            self.favoritos.push(canal);
        }
        gravar_favoritos(&self.favoritos);
        self.reordenar();
    }
}

/// No Windows a biblioteca vai no pacote, ao lado do executável. No Mac (só
/// para testar a interface antes de publicar) vale a do sistema.
fn caminho_do_mpv() -> std::path::PathBuf {
    if let Ok(escolhido) = std::env::var("SAIMO_MPV") {
        return std::path::PathBuf::from(escolhido);
    }
    if cfg!(windows) {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.join("libmpv-2.dll")))
            .unwrap_or_else(|| std::path::PathBuf::from("libmpv-2.dll"))
    } else {
        [
            "/opt/homebrew/lib/libmpv.dylib",
            "/opt/homebrew/opt/mpv/lib/libmpv.dylib",
            "/usr/local/lib/libmpv.dylib",
        ]
            .iter()
            .map(std::path::PathBuf::from)
            .find(|p| p.exists())
            .unwrap_or_else(|| std::path::PathBuf::from("libmpv.dylib"))
    }
}

fn ler_favoritos() -> Vec<String> {
    std::fs::read_to_string(catalogo::pasta().join("favoritos.txt"))
        .map(|t| t.lines().map(str::to_string).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

fn gravar_favoritos(favoritos: &[String]) {
    let _ = std::fs::write(catalogo::pasta().join("favoritos.txt"), favoritos.join("\n"));
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.cuidar_dos_recados(ctx);
        self.cuidar_do_mpv(ctx);
        self.teclado(ctx);
        self.baixar_logos_pendentes();

        tela::desenhar(self, ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        telemetria::fechando();
    }
}

mod tela;
