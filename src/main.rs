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
mod capas;
mod catalogo;
mod mpv;
mod progresso;
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
    Guia,
    Gavetas(Vec<vod::Gaveta>),
    Filmes(String, Vec<vod::Filme>),
    Series(String, Vec<vod::Serie>),
    Episodios(String, Vec<vod::Episodio>),
    Acervo(Vec<vod::Achado>),
    /// Uma capa achada no TMDB: só serve para redesenhar a lista.
    Capa,
}

/// As três listas da tela.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Aba {
    Canais,
    Filmes,
    Series,
}

/// O que está tocando quando não é canal: filme ou episódio.
pub struct TocandoVod {
    pub titulo: String,
    pub urls: Vec<String>,
    pub fonte: usize,
    pub desde: Instant,
    pub confirmado: bool,
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
    aba: Aba,
    guia_aberto: bool,
    gavetas: Vec<vod::Gaveta>,
    letra: String,
    filmes: Vec<vod::Filme>,
    series: Vec<vod::Serie>,
    serie_aberta: Option<vod::Serie>,
    episodios: Vec<vod::Episodio>,
    carregando_vod: bool,
    /// Todo o acervo só com nome, tipo e letra, para procurar fora da letra.
    acervo: Vec<vod::Achado>,
    favoritos_vod: Vec<String>,
    so_favoritos: bool,
    ultimo_progresso: Instant,
    foco_vod: usize,
    tocando_vod: Option<TocandoVod>,
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
        {
            // O índice do acervo é pequeno e dá as bases dos endereços; sem ele
            // nenhum filme abre.
            let emissor = emissor.clone();
            std::thread::spawn(move || {
                let gavetas = vod::indice();
                if !gavetas.is_empty() {
                    let _ = emissor.send(Recado::Gavetas(gavetas));
                }
                // O índice de busca são uns 800 KB para trinta mil títulos:
                // procurar no acervo inteiro sem baixar o acervo.
                let acervo = vod::busca();
                if !acervo.is_empty() {
                    let _ = emissor.send(Recado::Acervo(acervo));
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
            aba: Aba::Canais,
            guia_aberto: false,
            gavetas: Vec::new(),
            letra: "A".into(),
            filmes: Vec::new(),
            series: Vec::new(),
            serie_aberta: None,
            episodios: Vec::new(),
            carregando_vod: false,
            acervo: Vec::new(),
            favoritos_vod: ler_lista("favoritos-vod.txt"),
            so_favoritos: false,
            ultimo_progresso: Instant::now(),
            foco_vod: 0,
            tocando_vod: None,
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

    /// Manda montar o guia com a lista que está na tela.
    fn pedir_guia(&self) {
        let canais = self.canais.clone();
        let emissor = self.emissor.clone();
        epg::carregar(canais, move || {
            let _ = emissor.send(Recado::Guia);
        });
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
        self.pedir_guia();
    }

    // MARK: - Filmes e séries

    /// Troca a letra do acervo e busca as duas listas dela.
    fn abrir_letra(&mut self, letra: String) {
        if self.letra == letra && !self.filmes.is_empty() {
            return;
        }
        self.letra = letra.clone();
        self.filmes.clear();
        self.series.clear();
        self.serie_aberta = None;
        self.episodios.clear();
        self.foco_vod = 0;
        self.carregando_vod = true;
        let liberado = self.liberado;
        for serie in [false, true] {
            let emissor = self.emissor.clone();
            let letra = letra.clone();
            std::thread::spawn(move || {
                let recado = if serie {
                    Recado::Series(letra.clone(), vod::series(&letra))
                } else {
                    let mut lista = vod::filmes(&letra);
                    // Os reservados só existem depois do código, como os canais.
                    if liberado {
                        lista.extend(vod::reservados(&letra));
                        lista.sort_by(|a, b| {
                            catalogo::chave_de_ordem(&a.titulo).cmp(&catalogo::chave_de_ordem(&b.titulo))
                        });
                    }
                    Recado::Filmes(letra.clone(), lista)
                };
                let _ = emissor.send(recado);
            });
        }
    }

    fn abrir_serie(&mut self, serie: vod::Serie) {
        self.serie_aberta = Some(serie.clone());
        self.episodios.clear();
        self.carregando_vod = true;
        let emissor = self.emissor.clone();
        let letra = self.letra.clone();
        std::thread::spawn(move || {
            let lista = vod::episodios(&letra, &serie);
            let _ = emissor.send(Recado::Episodios(serie.titulo.clone(), lista));
        });
    }

    fn tocar_vod(&mut self, titulo: String, urls: Vec<String>, fonte: usize, nova: bool) {
        // Trocar de filme fecha a conta do anterior antes de perder a posição.
        self.guardar_progresso(true);
        let Some(mpv) = self.mpv.as_ref() else { return };
        let Some(url) = urls.get(fonte).cloned() else {
            telemetria::caiu("vod", &titulo, urls.len());
            self.aviso = Some((format!("{titulo}: nenhuma fonte abriu"), Instant::now()));
            self.tocando_vod = None;
            return;
        };
        mpv.tocar(&url, None, None);
        mpv.pausa(false);
        self.pausado = false;
        self.tocando = None;
        telemetria::comecou("vod", &titulo, &url, fonte + 1, nova);
        self.tocando_vod = Some(TocandoVod {
            titulo,
            urls,
            fonte,
            desde: Instant::now(),
            confirmado: false,
        });
        self.letreiro_ate = Instant::now() + SUMIR_LETREIRO;
        self.lista_aberta = false;
    }

    /// Grava onde o filme parou. `agora` força, em vez de esperar os 15 s.
    fn guardar_progresso(&mut self, agora: bool) {
        if !agora && self.ultimo_progresso.elapsed() < Duration::from_secs(15) {
            return;
        }
        self.ultimo_progresso = Instant::now();
        let (Some(filme), Some(mpv)) = (self.tocando_vod.as_ref(), self.mpv.as_ref()) else { return };
        if !filme.confirmado {
            return;
        }
        if let Some((posicao, duracao)) = mpv.posicao() {
            progresso::salvar(&filme.titulo, posicao, duracao);
        }
    }

    /// Volta ao ponto em que parou, quando há um.
    fn retomar(&mut self) {
        let (Some(filme), Some(mpv)) = (self.tocando_vod.as_ref(), self.mpv.as_ref()) else { return };
        let Some(marca) = progresso::onde_parou(&filme.titulo) else { return };
        mpv.ir_para(marca.posicao);
        let minutos = (marca.posicao / 60.0).round() as i64;
        self.aviso = Some((format!("continuando de {minutos} min"), Instant::now()));
    }

    fn favoritar_vod(&mut self, titulo: String) {
        if let Some(pos) = self.favoritos_vod.iter().position(|f| *f == titulo) {
            self.favoritos_vod.remove(pos);
        } else {
            self.favoritos_vod.push(titulo);
        }
        gravar_lista("favoritos-vod.txt", &self.favoritos_vod);
    }

    /// A capa do título: devolve o que já se sabe e manda procurar o resto.
    fn capa(&mut self, titulo: &str, serie: bool) -> Option<egui::TextureHandle> {
        match capas::conhecida(titulo, serie) {
            Some(url) if url.is_empty() => None,
            Some(url) => self.logo(&url),
            None => {
                let emissor = self.emissor.clone();
                capas::procurar(titulo.to_string(), serie, move |_| {
                    let _ = emissor.send(Recado::Capa);
                });
                None
            }
        }
    }

    fn proxima_fonte_vod(&mut self, motivo: &str) {
        let Some(t) = self.tocando_vod.as_ref() else { return };
        let (titulo, fonte, urls) = (t.titulo.clone(), t.fonte, t.urls.clone());
        telemetria::falhou("vod", &titulo, urls.get(fonte).map(String::as_str).unwrap_or(""), fonte + 1, motivo);
        if fonte + 1 >= urls.len() {
            telemetria::caiu("vod", &titulo, urls.len());
            telemetria::parou();
            self.aviso = Some((format!("{titulo}: nenhuma das {} fontes abriu", urls.len()), Instant::now()));
            self.tocando_vod = None;
            if let Some(mpv) = self.mpv.as_ref() {
                mpv.parar();
            }
            return;
        }
        self.aviso = Some((format!("{titulo}: trocando para a fonte {}", fonte + 2), Instant::now()));
        self.tocar_vod(titulo, urls, fonte + 1, false);
    }

    fn tocar(&mut self, canal: usize, fonte: usize, nova: bool) {
        let Some(mpv) = self.mpv.as_ref() else { return };
        let Some(c) = self.canais.get(canal) else { return };
        let Some(f) = c.fontes.get(fonte) else {
            telemetria::caiu("live", &c.nome, c.fontes.len());
            self.aviso = Some((format!("{}: nenhuma fonte abriu", c.nome), Instant::now()));
            self.tocando = None;
            return;
        };
        mpv.tocar(&f.url, f.referer.as_deref(), f.agente.as_deref());
        mpv.pausa(false);
        self.pausado = false;
        telemetria::comecou("live", &c.nome, &f.url, fonte + 1, nova);
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
        telemetria::falhou("live", &nome, &url, fonte + 1, motivo);
        if fonte + 1 >= total {
            telemetria::caiu("live", &nome, total);
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
            if self.tocando_vod.is_some() {
                match aviso {
                    mpv::Aviso::Tocando => {
                        if let Some(t) = self.tocando_vod.as_mut() {
                            if !t.confirmado {
                                t.confirmado = true;
                                let (titulo, fonte, ms) =
                                    (t.titulo.clone(), t.fonte, t.desde.elapsed().as_millis());
                                let url = t.urls.get(fonte).cloned().unwrap_or_default();
                                telemetria::tocou("vod", &titulo, &url, fonte + 1, ms);
                                self.retomar();
                            }
                        }
                    }
                    mpv::Aviso::Falhou => self.proxima_fonte_vod("mpv: erro na fonte"),
                    // Filme que termina é fim mesmo; canal que termina é fonte caindo.
                    mpv::Aviso::Fim => {
                        let acabou = self.tocando_vod.as_ref().map(|t| t.confirmado).unwrap_or(false);
                        if acabou {
                            // Chegou ao fim: nada a retomar da próxima vez.
                            if let Some(filme) = self.tocando_vod.as_ref() {
                                progresso::esquecer(&filme.titulo);
                            }
                            telemetria::parou();
                            self.tocando_vod = None;
                            self.lista_aberta = true;
                        } else {
                            self.proxima_fonte_vod("a fonte não abriu");
                        }
                    }
                }
                continue;
            }
            match aviso {
                mpv::Aviso::Tocando => {
                    if let Some(t) = self.tocando.as_mut() {
                        if !t.confirmado {
                            t.confirmado = true;
                            let ms = t.desde.elapsed().as_millis();
                            let (canal, fonte) = (t.canal, t.fonte);
                            if let Some(c) = self.canais.get(canal) {
                                let url = c.fontes.get(fonte).map(|f| f.url.clone()).unwrap_or_default();
                                telemetria::tocou("live", &c.nome, &url, fonte + 1, ms);
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
        let estourou_vod = self
            .tocando_vod
            .as_ref()
            .map(|t| !t.confirmado && t.desde.elapsed() > ESPERA_DA_FONTE)
            .unwrap_or(false);
        if estourou_vod {
            self.proxima_fonte_vod("a fonte não abriu em 14 s");
        }
        if self.tocando.is_some() || self.tocando_vod.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        self.guardar_progresso(false);
    }

    fn cuidar_dos_recados(&mut self, ctx: &egui::Context) {
        let mut chegou_algo = false;
        while let Ok(recado) = self.recados.try_recv() {
            chegou_algo = true;
            match recado {
                Recado::Catalogo(lista) => self.trocar_lista(lista, false),
                Recado::Restritos(lista) => self.trocar_lista(lista, true),
                Recado::Logo(url, imagem) => {
                    let textura = ctx.load_texture(&url, imagem, egui::TextureOptions::LINEAR);
                    self.logos.insert(url, Some(textura));
                }
                Recado::Atualizacao(v) => self.nova_versao = Some(v),
                Recado::Guia => {}
                Recado::Gavetas(lista) => self.gavetas = lista,
                Recado::Acervo(lista) => self.acervo = lista,
                Recado::Capa => {}
                Recado::Filmes(letra, lista) => {
                    if letra == self.letra {
                        self.filmes = lista;
                        self.carregando_vod = false;
                    }
                }
                Recado::Series(letra, lista) => {
                    if letra == self.letra {
                        self.series = lista;
                        self.carregando_vod = false;
                    }
                }
                Recado::Episodios(serie, lista) => {
                    if self.serie_aberta.as_ref().map(|s| s.titulo == serie).unwrap_or(false) {
                        self.episodios = lista;
                        self.carregando_vod = false;
                    }
                }
            }
        }
        // Sem isto, a lista só redesenharia no próximo movimento do mouse — e a
        // capa que acabou de chegar ficaria invisível até lá.
        if chegou_algo || !self.pedidos_de_logo.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(200));
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
        if !escrevendo && self.aba == Aba::Canais {
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
                let letra = self.letra.clone();
                self.letra.clear();
                self.abrir_letra(letra);
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
            // Nas listas do acervo as setas andam por elas, não pelos canais.
            if self.aba != Aba::Canais
                && matches!(key, egui::Key::ArrowDown | egui::Key::ArrowUp | egui::Key::Enter)
            {
                let total = if self.serie_aberta.is_some() {
                    self.episodios.len()
                } else if self.aba == Aba::Filmes {
                    self.filmes.len()
                } else {
                    self.series.len()
                };
                match key {
                    egui::Key::ArrowDown => self.foco_vod = (self.foco_vod + 1).min(total.saturating_sub(1)),
                    egui::Key::ArrowUp => self.foco_vod = self.foco_vod.saturating_sub(1),
                    _ => self.abrir_do_acervo(),
                }
                continue;
            }
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
                egui::Key::G => {
                    self.guia_aberto = !self.guia_aberto;
                }
                egui::Key::Tab => {
                    self.aba = match self.aba {
                        Aba::Canais => Aba::Filmes,
                        Aba::Filmes => Aba::Series,
                        Aba::Series => Aba::Canais,
                    };
                    self.busca.clear();
                    self.foco_vod = 0;
                    if self.aba != Aba::Canais && self.filmes.is_empty() && self.series.is_empty() {
                        let letra = self.letra.clone();
                        self.letra.clear();
                        self.abrir_letra(letra);
                    }
                }
                egui::Key::Plus | egui::Key::Equals => self.mudar_volume(5),
                egui::Key::Minus => self.mudar_volume(-5),
                egui::Key::L => match self.aba {
                    Aba::Canais => {
                        if let Some(canal) = self.canais.get(self.foco).map(|c| c.nome.clone()) {
                            self.favoritar(canal);
                        }
                    }
                    Aba::Filmes => {
                        if let Some(filme) = self.filmes.get(self.foco_vod).map(|f| f.titulo.clone()) {
                            self.favoritar_vod(filme);
                        }
                    }
                    Aba::Series => {
                        if let Some(serie) = self.series.get(self.foco_vod).map(|s| s.titulo.clone()) {
                            self.favoritar_vod(serie);
                        }
                    }
                },
                _ => {}
            }
        }
    }

    /// Enter nas listas do acervo: toca o filme ou o episódio, ou abre a série.
    fn abrir_do_acervo(&mut self) {
        if self.serie_aberta.is_some() {
            let Some(episodio) = self.episodios.get(self.foco_vod).cloned() else { return };
            let titulo = format!(
                "{} · T{} E{}",
                self.serie_aberta.as_ref().map(|s| s.titulo.clone()).unwrap_or_default(),
                episodio.temporada,
                episodio.numero
            );
            self.tocar_vod(titulo, episodio.urls, 0, true);
            return;
        }
        if self.aba == Aba::Filmes {
            let Some(filme) = self.filmes.get(self.foco_vod).cloned() else { return };
            let Some((_, urls)) = filme.versoes.first().cloned() else { return };
            self.tocar_vod(filme.titulo, urls, 0, true);
        } else if let Some(serie) = self.series.get(self.foco_vod).cloned() {
            self.abrir_serie(serie);
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
    ler_lista("favoritos.txt")
}

fn gravar_favoritos(favoritos: &[String]) {
    gravar_lista("favoritos.txt", favoritos);
}

fn ler_lista(nome: &str) -> Vec<String> {
    std::fs::read_to_string(catalogo::pasta().join(nome))
        .map(|t| t.lines().map(str::to_string).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

fn gravar_lista(nome: &str, itens: &[String]) {
    let _ = std::fs::write(catalogo::pasta().join(nome), itens.join("\n"));
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

        // Capa e logo pedidos durante o desenho só chegam no quadro seguinte:
        // sem este pedido de redesenho, a lista ficaria parada até alguém mexer.
        if !self.pedidos_de_logo.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.guardar_progresso(true);
        telemetria::fechando();
    }
}

mod epg;
mod vod;
mod tela;
