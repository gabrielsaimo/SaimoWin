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
mod fontes_desativadas;
mod capas;
mod destaques;
mod generos;
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
    let mut janela = egui::ViewportBuilder::default()
        .with_title("Saimo TV")
        .with_inner_size([1280.0, 720.0])
        .with_min_inner_size([800.0, 480.0]);
    if std::env::var("SAIMO_FOTO").is_ok() {
        // Foto de teste: abre sem tomar o foco de quem está usando a máquina.
        janela = janela.with_active(false);
    }
    let opcoes = eframe::NativeOptions {
        viewport: janela,
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
    Colecao(Aba, Vec<(vod::Serie, Vec<vod::Episodio>)>),
    Destaques(Vec<destaques::Fila>),
    Generos(generos::Generos),
    Episodios(String, Vec<vod::Episodio>),
    Acervo(Vec<vod::Achado>),
    /// A lista de servidores desligados mudou: a lista de canais muda junto.
    FontesDesativadas,
    /// Uma capa achada no TMDB: só serve para redesenhar a lista.
    Capa,
}

/// O que ocupa a área principal: o vídeo, ou uma das seções do acervo —
/// como no Mac, onde o acervo abre por cima do player.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Aba {
    Canais,
    /// A porta de entrada do acervo: fileiras de capa, no lugar da grade.
    Inicio,
    Filmes,
    Series,
    Animes,
    Doramas,
    Favoritos,
    Extras,
}

impl Aba {
    /// As seções que mostram filmes (e não séries).
    pub fn de_filmes(self) -> bool {
        matches!(self, Aba::Filmes | Aba::Favoritos | Aba::Extras)
    }
}

/// Um cartão da grade: o nome, o que vai escrito embaixo dele, e a letra onde
/// mora o endereço — que só é baixada quando alguém abre o título.
#[derive(Clone)]
pub struct ItemNaTela {
    pub titulo: String,
    pub detalhe: String,
    pub serie: bool,
    pub letra: String,
}

/// O que está tocando quando não é canal: filme ou episódio.
pub struct TocandoVod {
    pub titulo: String,
    pub urls: Vec<String>,
    pub fonte: usize,
    pub desde: Instant,
    pub confirmado: bool,
}

pub struct Tocando {
    pub canal: usize,
    pub fonte: usize,
    pub desde: Instant,
    pub confirmado: bool,
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
    /// A lista publicada como ela chegou, sem nada peneirado: é dela que a
    /// lista visível é remontada quando um servidor é religado.
    publicados: Vec<Canal>,
    gavetas: Vec<vod::Gaveta>,
    letra: String,
    filmes: Vec<vod::Filme>,
    series: Vec<vod::Serie>,
    serie_aberta: Option<vod::Serie>,
    episodios: Vec<vod::Episodio>,
    episodios_colecao: HashMap<String, Vec<vod::Episodio>>,
    /// As fileiras publicadas, baixadas uma vez por abertura do programa.
    filas: Vec<destaques::Fila>,
    /// Os gêneros publicados, e o escolhido na régua (vazio = todos).
    generos: generos::Generos,
    genero: String,
    /// Em qual fileira o teclado está. A coluna é o `foco_vod` de sempre.
    foco_fila: usize,
    carregando_vod: bool,
    /// Todo o acervo só com nome, tipo e letra: é dele que a grade se serve.
    acervo: Vec<vod::Achado>,
    /// O título que espera a letra dele terminar de chegar para abrir.
    abrir_ao_chegar: Option<(String, bool)>,
    favoritos_vod: Vec<String>,
    /// Busca do acervo, separada da busca de canais da barra lateral.
    busca_vod: String,
    quadro: u32,
    ultimo_pulo: Option<Instant>,
    /// Colunas da grade na última pintura, para as setas andarem por linha.
    pub colunas_vod: usize,
    ultimo_progresso: Instant,
    foco_vod: usize,
    tocando_vod: Option<TocandoVod>,
    /// Até quando a barra de controles fica à vista; mexer o mouse renova.
    controles_ate: Instant,
    ajuda_aberta: bool,
    velocidade: f32,
    preencher: bool,
    ultimo_mouse: Option<egui::Pos2>,
}

impl App {
    fn novo(cc: &eframe::CreationContext<'_>) -> Self {
        let (emissor, recados) = channel();
        tela::aplicar_estilo(&cc.egui_ctx);

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
            // Os servidores desligados no painel, de dois em dois minutos: é o
            // tempo que alguém aguenta um canal quebrado.
            let emissor = emissor.clone();
            std::thread::spawn(move || loop {
                if fontes_desativadas::atualizar() {
                    let _ = emissor.send(Recado::FontesDesativadas);
                }
                std::thread::sleep(std::time::Duration::from_secs(120));
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
                // Os gêneros vêm do mesmo lugar, e só servem para a régua do
                // acervo: podem chegar depois de tudo.
                let lidos = generos::baixar();
                if !lidos.vazio() {
                    let _ = emissor.send(Recado::Generos(lidos));
                }
            });
        }

        let mut app = App {
            mpv,
            erro_do_mpv,
            canais: canais.clone(),
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
            publicados: canais.clone(),
            gavetas: Vec::new(),
            letra: "A".into(),
            filmes: Vec::new(),
            series: Vec::new(),
            serie_aberta: None,
            episodios: Vec::new(),
            episodios_colecao: HashMap::new(),
            filas: Vec::new(),
            generos: generos::Generos::default(),
            genero: String::new(),
            foco_fila: 0,
            carregando_vod: false,
            acervo: Vec::new(),
            abrir_ao_chegar: None,
            favoritos_vod: ler_lista("favoritos-vod.txt"),
            busca_vod: String::new(),
            quadro: 0,
            ultimo_pulo: None,
            colunas_vod: 5,
            ultimo_progresso: Instant::now(),
            foco_vod: 0,
            tocando_vod: None,
            controles_ate: Instant::now() + Duration::from_secs(6),
            ajuda_aberta: false,
            velocidade: 1.0,
            preencher: false,
            ultimo_mouse: None,
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
        // Sempre da lista publicada: peneirar a que já está na tela faria o
        // servidor religado não voltar nunca, porque suas fontes já teriam
        // sido jogadas fora.
        let mut lista = fontes_desativadas::peneirar_canais(self.publicados.clone());
        if self.liberado {
            let ja: Vec<String> = lista.iter().map(|c| c.nome.clone()).collect();
            lista.extend(
                fontes_desativadas::peneirar_canais(self.restritos.clone())
                    .into_iter()
                    .filter(|r| !ja.contains(&r.nome)),
            );
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
            // A lista de canais visíveis é montada a cada reordenação; esta
            // é a publicada pura, e guardá-la evita duplicar restrito a cada
            // recarga.
            self.publicados = base;
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
        if let Some(lista) = self.episodios_colecao.get(&serie.titulo).cloned() {
            self.episodios = lista;
            self.carregando_vod = false;
            return;
        }
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
        self.aba = Aba::Canais;
        self.mostrar_controles();
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
    /// A capa de um título.
    ///
    /// O endereço vem da ficha publicada, resolvida pelo id do TMDB — não mais
    /// de uma busca por nome feita aqui, que era lenta e trocava filmes de nome
    /// igual. Quem não tem ficha fica sem capa, e a tela põe uma marca no
    /// lugar.
    fn capa(&mut self, titulo: &str, serie: bool) -> Option<egui::TextureHandle> {
        let endereco = self.generos.capa(titulo, serie)?.to_string();
        self.logo(&endereco)
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
        self.contar_video();
        self.guardar_progresso(false);
    }

    /// O monitor só conta o tempo com o vídeo andando de verdade.
    fn contar_video(&mut self) {
        let rodando = self.tocando.as_ref().map(|t| t.confirmado).unwrap_or(false)
            || self.tocando_vod.as_ref().map(|t| t.confirmado).unwrap_or(false);
        let (carregando, qualidade) = match (rodando, self.mpv.as_ref()) {
            (true, Some(mpv)) => {
                let pulando = mpv.ler("seeking").as_deref() == Some("yes");
                if pulando {
                    self.ultimo_pulo = Some(Instant::now());
                }
                let carregando = pulando || mpv.ler("paused-for-cache").as_deref() == Some("yes");
                // A altura só muda quando troca a variante: não precisa ler todo quadro.
                let qualidade = if self.quadro % 120 == 0 {
                    mpv.ler("height").and_then(|h| h.parse::<u32>().ok()).filter(|h| *h > 0).map(|h| format!("{h}p"))
                } else {
                    None
                };
                (carregando, qualidade)
            }
            _ => (false, None),
        };
        self.quadro = self.quadro.wrapping_add(1);
        // Carregar logo depois de pular para outro ponto do filme é normal, não travamento.
        let pulou = self.ultimo_pulo.map(|t| t.elapsed() < Duration::from_secs(3)).unwrap_or(false);
        telemetria::video(rodando, self.pausado, carregando, pulou, qualidade);
        if !self.busca.trim().is_empty() {
            telemetria::busca(0, &self.busca, !self.visiveis().is_empty());
        }
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
                Recado::FontesDesativadas => {
                    self.reordenar();
                    self.pedir_guia();
                }
                Recado::Capa => {}
                Recado::Filmes(letra, lista) => {
                    if letra == self.letra {
                        self.filmes = lista;
                        self.carregando_vod = false;
                        self.abrir_o_que_esperava();
                    }
                }
                Recado::Series(letra, lista) => {
                    if letra == self.letra {
                        self.series = lista;
                        self.carregando_vod = false;
                        self.abrir_o_que_esperava();
                    }
                }
                Recado::Generos(lidos) => {
                    self.generos = lidos;
                }
                Recado::Destaques(lista) => {
                    for fila in &lista {
                        for item in &fila.itens {
                            capas::anotar(&item.titulo, item.serie(), &item.capa);
                        }
                    }
                    self.filas = lista;
                    self.carregando_vod = false;
                }
                Recado::Colecao(aba, lista) => {
                    if self.aba == aba {
                        self.episodios_colecao = lista.iter()
                            .map(|(serie, episodios)| (serie.titulo.clone(), episodios.clone()))
                            .collect();
                        self.series = lista.into_iter().map(|(serie, _)| serie).collect();
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
        let (eventos, texto_digitado, mouse) = ctx.input(|i| {
            (
                i.events.clone(),
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Text(t) => Some(t.clone()),
                        _ => None,
                    })
                    .collect::<String>(),
                i.pointer.hover_pos(),
            )
        });
        // Mexer o mouse traz os controles de volta, como no Mac.
        if mouse.is_some() && mouse != self.ultimo_mouse {
            self.ultimo_mouse = mouse;
            self.mostrar_controles();
        }
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
            if texto_digitado.contains('?') {
                self.ajuda_aberta = !self.ajuda_aberta;
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
                if !self.liberado && self.aba == Aba::Extras {
                    self.abrir_secao(Aba::Filmes);
                }
            } else if let Ok(numero) = digitado.parse::<usize>() {
                if numero >= 1 && numero <= self.canais.len() {
                    self.tocar(numero - 1, 0, true);
                    self.foco = numero - 1;
                }
            }
        }

        for evento in eventos {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = evento else { continue };
            // Com a busca aberta as letras são texto; só as teclas de sair e de
            // descer para a lista passam.
            if escrevendo && !matches!(key, egui::Key::Escape | egui::Key::Enter | egui::Key::ArrowDown) {
                continue;
            }
            if escrevendo && matches!(key, egui::Key::Escape | egui::Key::Enter | egui::Key::ArrowDown) {
                ctx.memory_mut(|m| m.request_focus(egui::Id::NULL));
                if key != egui::Key::Enter {
                    continue;
                }
            }
            match key {
                egui::Key::Space => self.alternar_pausa(),
                egui::Key::F if modifiers.ctrl => {
                    ctx.memory_mut(|m| m.request_focus(egui::Id::new("busca-canais")));
                    self.lista_aberta = true;
                }
                egui::Key::F | egui::Key::F11 => self.alternar_tela_cheia(ctx),
                egui::Key::F1 | egui::Key::H => self.ajuda_aberta = !self.ajuda_aberta,
                egui::Key::G => {
                    self.guia_aberto = !self.guia_aberto;
                    self.abrir_secao(Aba::Canais);
                }
                egui::Key::L => self.lista_aberta = !self.lista_aberta,
                egui::Key::M => self.alternar_mudo(),
                egui::Key::Plus | egui::Key::Equals => self.mudar_volume(5),
                egui::Key::Minus => self.mudar_volume(-5),
                egui::Key::PageUp => self.pular_canal(-1),
                egui::Key::PageDown => self.pular_canal(1),
                egui::Key::Tab => {
                    let proxima = match self.aba {
                        Aba::Canais => Aba::Inicio,
                        Aba::Inicio => Aba::Filmes,
                        Aba::Filmes => Aba::Series,
                        Aba::Series => Aba::Animes,
                        Aba::Animes => Aba::Doramas,
                        _ => Aba::Canais,
                    };
                    self.abrir_secao(proxima);
                }
                egui::Key::Escape => {
                    if self.ajuda_aberta {
                        self.ajuda_aberta = false;
                    } else if self.serie_aberta.is_some() {
                        self.serie_aberta = None;
                    } else if self.aba != Aba::Canais {
                        self.abrir_secao(Aba::Canais);
                    } else if self.guia_aberto {
                        self.guia_aberto = false;
                    } else if ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                    } else {
                        self.lista_aberta = !self.lista_aberta;
                    }
                }
                _ if self.aba != Aba::Canais => self.tecla_no_acervo(key),
                egui::Key::ArrowDown | egui::Key::ArrowUp => {
                    let visiveis = self.visiveis();
                    let atual = visiveis.iter().position(|i| *i == self.foco).unwrap_or(0);
                    let novo = if key == egui::Key::ArrowDown {
                        (atual + 1).min(visiveis.len().saturating_sub(1))
                    } else {
                        atual.saturating_sub(1)
                    };
                    if let Some(i) = visiveis.get(novo) {
                        self.foco = *i;
                        self.lista_aberta = true;
                    }
                }
                egui::Key::Enter => {
                    let foco = self.foco;
                    if self.visiveis().contains(&foco) {
                        self.tocar(foco, 0, true);
                    }
                }
                egui::Key::ArrowRight => {
                    if self.tocando_vod.is_some() {
                        self.avancar(10.0);
                    } else {
                        self.escolher_fonte_seguinte();
                    }
                }
                egui::Key::ArrowLeft => {
                    if self.tocando_vod.is_some() {
                        self.avancar(-10.0);
                    }
                }
                egui::Key::S => {
                    if let Some(canal) = self.canais.get(self.foco).map(|c| c.nome.clone()) {
                        self.favoritar(canal);
                    }
                }
                _ => {}
            }
        }
    }

    /// Setas, Enter e estrela dentro das fileiras da tela inicial.
    ///
    /// Esquerda e direita andam dentro da fileira; cima e baixo trocam de
    /// fileira mantendo a coluna, que é como toda TV se comporta e como a
    /// pessoa espera depois de dois segundos mexendo.
    fn tecla_nas_fileiras(&mut self, key: egui::Key) {
        let tamanhos = tela::tamanho_das_filas(self);
        if tamanhos.is_empty() {
            return;
        }
        self.foco_fila = self.foco_fila.min(tamanhos.len() - 1);
        let nesta = tamanhos[self.foco_fila];
        match key {
            egui::Key::ArrowRight => {
                self.foco_vod = (self.foco_vod + 1).min(nesta.saturating_sub(1));
            }
            egui::Key::ArrowLeft => self.foco_vod = self.foco_vod.saturating_sub(1),
            egui::Key::ArrowDown => {
                if self.foco_fila + 1 < tamanhos.len() {
                    self.foco_fila += 1;
                    self.foco_vod = self.foco_vod.min(tamanhos[self.foco_fila].saturating_sub(1));
                }
            }
            egui::Key::ArrowUp => {
                if self.foco_fila > 0 {
                    self.foco_fila -= 1;
                    self.foco_vod = self.foco_vod.min(tamanhos[self.foco_fila].saturating_sub(1));
                }
            }
            egui::Key::Enter => tela::abrir_em_foco_na_fileira(self),
            egui::Key::S => {
                if let Some(titulo) = self.titulo_em_foco() {
                    self.favoritar_vod(titulo);
                }
            }
            _ => {}
        }
    }

    /// Setas, Enter e estrela dentro da grade do acervo.
    fn tecla_no_acervo(&mut self, key: egui::Key) {
        if self.aba == Aba::Inicio {
            self.tecla_nas_fileiras(key);
            return;
        }
        let colunas = self.colunas_vod.max(1);
        let total = self.itens_do_acervo();
        match key {
            egui::Key::ArrowRight => self.foco_vod = (self.foco_vod + 1).min(total.saturating_sub(1)),
            egui::Key::ArrowLeft => self.foco_vod = self.foco_vod.saturating_sub(1),
            egui::Key::ArrowDown => {
                let passo = if self.serie_aberta.is_some() { 1 } else { colunas };
                self.foco_vod = (self.foco_vod + passo).min(total.saturating_sub(1));
            }
            egui::Key::ArrowUp => {
                let passo = if self.serie_aberta.is_some() { 1 } else { colunas };
                self.foco_vod = self.foco_vod.saturating_sub(passo);
            }
            egui::Key::Enter => self.abrir_do_acervo(),
            egui::Key::S => {
                if let Some(titulo) = self.titulo_em_foco() {
                    self.favoritar_vod(titulo);
                }
            }
            _ => {}
        }
    }

    /// O que a grade mostra: um título, o bastante para desenhar o cartão, e a
    /// letra onde mora o endereço dele.
    ///
    /// O acervo é publicado por letra porque são 30 MB, mas o índice de busca
    /// tem os trinta mil nomes em 800 KB — então a grade lista tudo a partir
    /// dele e só baixa a letra quando alguém abre um título. O 18+ fica de
    /// fora: ele não entra no índice, e continua vindo uma letra por vez.
    pub fn itens_na_tela(&self) -> Vec<ItemNaTela> {
        let busca = catalogo::chave_de_ordem(self.busca_vod.trim());
        let serie = matches!(self.aba, Aba::Series | Aba::Animes | Aba::Doramas);

        // Coleções e 18+ já estão inteiros na memória; o resto vem do índice.
        if matches!(self.aba, Aba::Animes | Aba::Doramas) {
            return self
                .series_na_tela()
                .into_iter()
                .map(|s| {
                    let ano = if s.ano.is_empty() { String::new() } else { format!("{} · ", s.ano) };
                    ItemNaTela {
                        detalhe: format!("{ano}{} episódios", s.episodios),
                        titulo: s.titulo,
                        serie: true,
                        letra: self.letra.clone(),
                    }
                })
                .collect();
        }
        if self.aba == Aba::Extras {
            return self
                .filmes_na_tela()
                .into_iter()
                .map(|f| ItemNaTela {
                    detalhe: f.versoes.iter().map(|(v, _)| v.to_uppercase()).collect::<Vec<_>>().join(" · "),
                    titulo: f.titulo,
                    serie: false,
                    letra: self.letra.clone(),
                })
                .collect();
        }

        self.acervo
            .iter()
            .filter(|a| a.serie == serie || self.aba == Aba::Favoritos)
            .filter(|a| self.aba != Aba::Favoritos || self.favoritos_vod.iter().any(|t| *t == a.titulo))
            .filter(|a| busca.is_empty() || catalogo::chave_de_ordem(&a.titulo).contains(&busca))
            .filter(|a| self.generos.tem(&a.titulo, a.serie, &self.genero))
            .map(|a| ItemNaTela {
                titulo: a.titulo.clone(),
                detalhe: a.ano.clone(),
                serie: a.serie,
                letra: a.letra.clone(),
            })
            .collect()
    }

    /// Abre um título da grade: a letra dele pode ainda não ter chegado, e aí
    /// a abertura fica esperando o recado com a lista.
    pub fn abrir_item(&mut self, item: ItemNaTela) {
        if self.letra != item.letra && self.aba != Aba::Extras && !matches!(self.aba, Aba::Animes | Aba::Doramas) {
            self.abrir_letra(item.letra.clone());
        }
        if item.serie {
            if let Some(s) = self.series.iter().find(|s| s.titulo == item.titulo).cloned() {
                self.foco_vod = 0;
                self.abrir_serie(s);
                return;
            }
        } else if let Some(f) = self.filmes.iter().find(|f| f.titulo == item.titulo).cloned() {
            if let Some((_, urls)) = f.versoes.first().cloned() {
                self.tocar_vod(f.titulo, urls, 0, true);
            }
            return;
        }
        self.abrir_ao_chegar = Some((item.titulo, item.serie));
        self.carregando_vod = true;
    }

    /// Chamado quando a lista de uma letra chega: se alguém estava esperando
    /// um título dela, abre agora.
    fn abrir_o_que_esperava(&mut self) {
        let Some((titulo, serie)) = self.abrir_ao_chegar.clone() else { return };
        if serie {
            if let Some(s) = self.series.iter().find(|s| s.titulo == titulo).cloned() {
                self.abrir_ao_chegar = None;
                self.foco_vod = 0;
                self.abrir_serie(s);
            }
        } else if let Some(f) = self.filmes.iter().find(|f| f.titulo == titulo).cloned() {
            self.abrir_ao_chegar = None;
            if let Some((_, urls)) = f.versoes.first().cloned() {
                self.tocar_vod(f.titulo, urls, 0, true);
            }
        }
    }

    /// Filmes da seção aberta, já filtrados pela busca e pela seção.
    pub fn filmes_na_tela(&self) -> Vec<vod::Filme> {
        // A lista de gêneros guarda o título como o acervo o escreve, sem o
        // ano que às vezes vem colado no nome.

        let busca = catalogo::chave_de_ordem(self.busca_vod.trim());
        self.filmes
            .iter()
            .filter(|f| match self.aba {
                Aba::Favoritos => self.favoritos_vod.iter().any(|t| *t == f.titulo),
                Aba::Extras => f.reservado,
                _ => !f.reservado || self.liberado,
            })
            .filter(|f| busca.is_empty() || catalogo::chave_de_ordem(&f.titulo).contains(&busca))
            .filter(|f| self.generos.tem(&f.titulo, false, &self.genero))
            .cloned()
            .collect()
    }

    pub fn series_na_tela(&self) -> Vec<vod::Serie> {
        let busca = catalogo::chave_de_ordem(self.busca_vod.trim());
        self.series
            .iter()
            .filter(|s| self.aba != Aba::Favoritos || self.favoritos_vod.iter().any(|t| *t == s.titulo))
            .filter(|s| busca.is_empty() || catalogo::chave_de_ordem(&s.titulo).contains(&busca))
            .filter(|s| self.generos.tem(&s.titulo, true, &self.genero))
            .cloned()
            .collect()
    }

    fn itens_do_acervo(&self) -> usize {
        if self.aba == Aba::Inicio {
            return tela::tamanho_das_filas(self).get(self.foco_fila).copied().unwrap_or(0);
        }
        if self.serie_aberta.is_some() {
            self.episodios.len()
        } else {
            self.itens_na_tela().len()
        }
    }

    fn titulo_em_foco(&self) -> Option<String> {
        self.itens_na_tela().get(self.foco_vod).map(|i| i.titulo.clone())
    }

    /// Enter no acervo: toca o filme ou o episódio, ou abre a série.
    pub fn abrir_do_acervo(&mut self) {
        if let Some(serie) = self.serie_aberta.clone() {
            let Some(episodio) = self.episodios.get(self.foco_vod).cloned() else { return };
            let titulo = format!("{} · T{} E{}", serie.titulo, episodio.temporada, episodio.numero);
            self.tocar_vod(titulo, episodio.urls, 0, true);
            return;
        }
        if let Some(item) = self.itens_na_tela().get(self.foco_vod).cloned() {
            self.abrir_item(item);
        }
    }

    // MARK: - Comandos da barra

    pub fn mostrar_controles(&mut self) {
        self.controles_ate = Instant::now() + Duration::from_secs(3);
    }

    pub fn controles_visiveis(&self) -> bool {
        Instant::now() < self.controles_ate || self.pausado
    }

    pub fn alternar_pausa(&mut self) {
        self.pausado = !self.pausado;
        if let Some(mpv) = self.mpv.as_ref() {
            mpv.pausa(self.pausado);
        }
        self.mostrar_controles();
    }

    pub fn alternar_mudo(&mut self) {
        self.mudo = !self.mudo;
        if let Some(mpv) = self.mpv.as_ref() {
            mpv.mudo(self.mudo);
        }
        self.mostrar_controles();
    }

    pub fn definir_volume(&mut self, valor: i32) {
        self.volume = valor.clamp(0, 130);
        if let Some(mpv) = self.mpv.as_ref() {
            mpv.volume(self.volume);
        }
    }

    /// Canal vizinho na lista que está na tela; -1 é o de cima.
    pub fn pular_canal(&mut self, passo: i64) {
        let visiveis = self.visiveis();
        if visiveis.is_empty() {
            return;
        }
        let atual = self
            .tocando
            .as_ref()
            .and_then(|t| visiveis.iter().position(|i| *i == t.canal))
            .unwrap_or(0) as i64;
        let novo = (atual + passo).rem_euclid(visiveis.len() as i64) as usize;
        let canal = visiveis[novo];
        self.foco = canal;
        self.tocar(canal, 0, true);
    }

    pub fn alternar_tela_cheia(&self, ctx: &egui::Context) {
        let cheia = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!cheia));
    }

    /// Avança ou volta no filme (segundos negativos voltam).
    pub fn avancar(&mut self, segundos: f64) {
        let Some(mpv) = self.mpv.as_ref() else { return };
        if let Some((posicao, duracao)) = mpv.posicao() {
            mpv.ir_para((posicao + segundos).clamp(0.0, duracao - 1.0));
        }
        self.mostrar_controles();
    }

    pub fn ir_para(&mut self, segundos: f64) {
        if let Some(mpv) = self.mpv.as_ref() {
            mpv.ir_para(segundos);
        }
    }

    pub fn definir_velocidade(&mut self, velocidade: f32) {
        self.velocidade = velocidade;
        if let Some(mpv) = self.mpv.as_ref() {
            mpv.definir("speed", &format!("{velocidade}"));
        }
        self.aviso = Some((format!("velocidade {velocidade}×"), Instant::now()));
    }

    /// Preencher a tela corta as bordas em vez de deixar faixas pretas.
    pub fn alternar_preencher(&mut self) {
        self.preencher = !self.preencher;
        if let Some(mpv) = self.mpv.as_ref() {
            mpv.definir("panscan", if self.preencher { "1.0" } else { "0.0" });
        }
    }

    pub fn escolher_fonte(&mut self, fonte: usize) {
        if let Some(canal) = self.tocando.as_ref().map(|t| t.canal) {
            self.tocar(canal, fonte, false);
        } else if let Some(filme) = self.tocando_vod.as_ref() {
            let (titulo, urls) = (filme.titulo.clone(), filme.urls.clone());
            self.tocar_vod(titulo, urls, fonte, false);
        }
    }

    pub fn abrir_secao(&mut self, aba: Aba) {
        if self.aba == aba {
            return;
        }
        self.aba = aba;
        self.busca_vod.clear();
        self.foco_vod = 0;
        self.serie_aberta = None;
        self.foco_fila = 0;
        if aba == Aba::Inicio {
            if self.filas.is_empty() {
                self.carregando_vod = true;
                let emissor = self.emissor.clone();
                std::thread::spawn(move || {
                    let _ = emissor.send(Recado::Destaques(destaques::filas()));
                });
            }
            return;
        }
        if matches!(aba, Aba::Animes | Aba::Doramas) {
            self.series.clear();
            self.episodios_colecao.clear();
            self.carregando_vod = true;
            let emissor = self.emissor.clone();
            std::thread::spawn(move || {
                let tipo = if aba == Aba::Animes { "animes" } else { "doramas" };
                let _ = emissor.send(Recado::Colecao(aba, vod::colecao(tipo)));
            });
            return;
        }
        if aba != Aba::Canais && self.filmes.is_empty() && self.series.is_empty() {
            let letra = self.letra.clone();
            self.letra.clear();
            self.abrir_letra(letra);
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

/// Se o programa foi aberto de dentro do ZIP, sem descompactar.
///
/// Clicando no executável dentro da janela do compactador, o Windows copia só
/// ele para uma pasta temporária e deixa o resto para trás — a libmpv-2.dll
/// fica no ZIP, e o vídeo não tem como abrir. O caminho denuncia: ele passa
/// por uma pasta que termina em ".zip", dentro do Temp do usuário.
///
///   C:\Users\...\AppData\Local\Temp\<código>_SaimoTV-Windows.zip\SaimoTV\
///
/// Vale a pena reconhecer isso em vez de só dizer que falta um arquivo: quem
/// abriu assim não fez nada de errado, e a saída é uma frase, não uma caçada.
pub fn aberto_de_dentro_do_zip() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let Some(pasta) = exe.parent() else { return false };
    let caminho = pasta.to_string_lossy().to_ascii_lowercase();
    let dentro_de_zip = pasta
        .ancestors()
        .filter_map(|p| p.file_name())
        .any(|nome| nome.to_string_lossy().to_ascii_lowercase().ends_with(".zip"));
    // O Temp sozinho não basta: há quem descompacte ali de propósito.
    dentro_de_zip || caminho.contains("\\temp\\") && caminho.contains(".zip")
}

/// A pasta de onde o programa está rodando, para mostrar no aviso.
pub fn pasta_do_programa() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.display().to_string()))
        .unwrap_or_default()
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

/// Conferência visual sem depender de quem está na frente da tela:
/// `SAIMO_FOTO=arquivo.png` salva a própria janela depois de alguns segundos e
/// fecha. `SAIMO_FOTO_TELA` escolhe o que mostrar (filmes, series, guia,
/// atalhos). Não faz nada sem a variável.
fn foto_de_teste(app: &mut App, ctx: &egui::Context) {
    let Ok(destino) = std::env::var("SAIMO_FOTO") else { return };
    static INICIO: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    static PEDIDA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let inicio = *INICIO.get_or_init(Instant::now);
    ctx.request_repaint_after(Duration::from_millis(200));

    let espera = std::env::var("SAIMO_FOTO_ESPERA").ok().and_then(|v| v.parse().ok()).unwrap_or(18);
    if inicio.elapsed() > Duration::from_secs(3) && inicio.elapsed() < Duration::from_secs(4) {
        match std::env::var("SAIMO_FOTO_TELA").unwrap_or_default().as_str() {
            "inicio" => app.abrir_secao(Aba::Inicio),
            "filmes" => app.abrir_secao(Aba::Filmes),
            "series" => app.abrir_secao(Aba::Series),
            "guia" => app.guia_aberto = true,
            "atalhos" => app.ajuda_aberta = true,
            _ => {}
        }
    }
    if inicio.elapsed() > Duration::from_secs(espera) {
        app.mostrar_controles();
        if !PEDIDA.swap(true, std::sync::atomic::Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
        }
    }
    let imagem = ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        })
    });
    if let Some(imagem) = imagem {
        let [w, h] = imagem.size;
        let bytes: Vec<u8> = imagem.pixels.iter().flat_map(|c| c.to_array()).collect();
        if let Some(buffer) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
            let _ = buffer.save(&destino);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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

        foto_de_teste(self, ctx);
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
