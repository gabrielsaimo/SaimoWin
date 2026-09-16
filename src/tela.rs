//! O desenho: vídeo no fundo, lista e letreiro por cima.
//!
//! Tudo numa passada só do egui. O vídeo entra como uma chamada de desenho
//! OpenGL antes de qualquer widget, então a interface fica sempre por cima da
//! imagem sem precisar de uma segunda janela.

use crate::{atualizacao, catalogo, epg, vod, Aba, App};
use eframe::egui;
use eframe::glow::HasContext;
use std::sync::Arc;
use std::time::{Duration, Instant};

const ROXO: egui::Color32 = egui::Color32::from_rgb(139, 92, 246);
const ROSA: egui::Color32 = egui::Color32::from_rgb(236, 72, 153);

pub fn desenhar(app: &mut App, ctx: &egui::Context) {
    video(app, ctx);

    if app.lista_aberta {
        lista(app, ctx);
    }
    if app.guia_aberto {
        guia(app, ctx);
    }
    letreiro(app, ctx);
    avisos(app, ctx);
    atualizacao_na_tela(app, ctx);
}

/// O quadro do mpv, desenhado no framebuffer da janela.
fn video(app: &mut App, ctx: &egui::Context) {
    let Some(mpv) = app.mpv.clone() else {
        sem_mpv(app, ctx);
        return;
    };
    // Camada de fundo, e não um painel central: painel central se desenha por
    // cima dos laterais e escondia a lista e o guia.
    egui::Area::new(egui::Id::new("video"))
        .order(egui::Order::Background)
        .fixed_pos(egui::pos2(0.0, 0.0))
        .show(ctx, |ui| {
            let rect = ctx.screen_rect();
            ui.allocate_rect(rect, egui::Sense::hover());
            let callback = egui::PaintCallback {
                rect,
                callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |info, pintor| {
                    // A primeira chamada acontece já com o contexto OpenGL de
                    // pé, que é o que o mpv precisa para se ligar à janela.
                    if let Err(erro) = mpv.ligar_video() {
                        crate::telemetria::erro(erro);
                        return;
                    }
                    let gl = pintor.gl();
                    let fbo = unsafe { gl.get_parameter_i32(eframe::glow::DRAW_FRAMEBUFFER_BINDING) };
                    let [largura, altura] = info.screen_size_px;
                    mpv.desenhar(fbo, largura as i32, altura as i32);
                })),
            };
            ui.painter().add(callback);
        });
}

fn sem_mpv(app: &App, ctx: &egui::Context) {
    egui::Area::new(egui::Id::new("sem-mpv"))
        .order(egui::Order::Background)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.vertical_centered(|ui| {
                ui.heading("Falta o mpv");
                ui.label("O arquivo libmpv-2.dll precisa estar na mesma pasta do Saimo TV.exe.");
                ui.label(
                    egui::RichText::new(app.erro_do_mpv.clone().unwrap_or_default())
                        .size(11.0)
                        .weak(),
                );
            });
        });
}

fn lista(app: &mut App, ctx: &egui::Context) {
    let largura = (ctx.screen_rect().width() * 0.34).clamp(320.0, 520.0);
    egui::SidePanel::left("canais")
        .exact_width(largura)
        .frame(
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_unmultiplied(12, 12, 16, 235))
                .inner_margin(egui::Margin::symmetric(12.0, 12.0)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Saimo").size(20.0).strong());
                ui.label(egui::RichText::new("TV").size(20.0).strong().color(ROSA));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("{} canais", app.canais.len())).weak());
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                for (aba, nome) in [(Aba::Canais, "Canais"), (Aba::Filmes, "Filmes"), (Aba::Series, "Séries")] {
                    let atual = app.aba == aba;
                    if ui.selectable_label(atual, egui::RichText::new(nome).size(14.0)).clicked() && !atual {
                        app.aba = aba;
                        app.busca.clear();
                        if aba != Aba::Canais && app.filmes.is_empty() && app.series.is_empty() {
                            let letra = app.letra.clone();
                            app.letra.clear();
                            app.abrir_letra(letra);
                        }
                    }
                }
            });
            ui.add_space(6.0);
            let dica = match app.aba {
                Aba::Canais => "Buscar canal…",
                Aba::Filmes => "Buscar filme desta letra…",
                Aba::Series => "Buscar série desta letra…",
            };
            let busca = ui.add(
                egui::TextEdit::singleline(&mut app.busca)
                    .hint_text(dica)
                    .desired_width(f32::INFINITY),
            );
            if busca.changed() {
                app.foco_vod = 0;
                if let Some(primeiro) = app.visiveis().first() {
                    app.foco = *primeiro;
                }
            }
            ui.add_space(8.0);

            if app.aba != Aba::Canais {
                acervo(app, ui);
                return;
            }

            let visiveis = app.visiveis();
            let tocando = app.tocando.as_ref().map(|t| t.canal);
            let foco = app.foco;
            let mut clicado: Option<usize> = None;
            let mut favoritar: Option<String> = None;
            let mut secao_atual = String::new();

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for indice in visiveis {
                    let (nome, secao, logo, fontes) = {
                        let canal = &app.canais[indice];
                        (canal.nome.clone(), canal.secao().to_string(), canal.logo.clone(), canal.fontes.len())
                    };
                    if secao != secao_atual {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(secao.to_uppercase()).size(11.0).color(ROXO).strong());
                        ui.add_space(2.0);
                        secao_atual = secao;
                    }
                    let favorito = app.favoritos.iter().any(|f| *f == nome);
                    let textura = logo.as_ref().and_then(|url| app.logo(url));
                    let no_ar = epg::agora_e_depois(&nome).map(|(atual, _)| atual.titulo);
                    let resposta = linha_do_canal(
                        ui,
                        indice + 1,
                        &nome,
                        fontes,
                        textura,
                        favorito,
                        Some(indice) == tocando,
                        indice == foco,
                        no_ar.as_deref(),
                    );
                    if resposta.clicked() {
                        clicado = Some(indice);
                    }
                    if resposta.secondary_clicked() {
                        favoritar = Some(nome.clone());
                    }
                    if indice == foco && ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { .. }))) {
                        resposta.scroll_to_me(Some(egui::Align::Center));
                    }
                }
            });

            if let Some(canal) = favoritar {
                app.favoritar(canal);
            }
            if let Some(indice) = clicado {
                app.foco = indice;
                app.tocar(indice, 0, true);
                app.lista_aberta = false;
            }

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Enter assiste · → troca a fonte · G guia · Tab muda de lista · L favorita · F tela cheia · Esc esconde a lista",
                )
                .size(11.0)
                .weak(),
            );
        });
}

#[allow(clippy::too_many_arguments)]
fn linha_do_canal(
    ui: &mut egui::Ui,
    numero: usize,
    nome: &str,
    fontes: usize,
    logo: Option<egui::TextureHandle>,
    favorito: bool,
    tocando: bool,
    focado: bool,
    no_ar: Option<&str>,
) -> egui::Response {
    let altura = if no_ar.is_some() { 54.0 } else { 46.0 };
    let (rect, resposta) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), altura), egui::Sense::click());
    let pintor = ui.painter();
    if focado || tocando || resposta.hovered() {
        let cor = if tocando {
            egui::Color32::from_rgba_unmultiplied(139, 92, 246, 60)
        } else if focado {
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 26)
        } else {
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 14)
        };
        pintor.rect_filled(rect, 8.0, cor);
    }
    if focado {
        pintor.rect_stroke(rect, 8.0, egui::Stroke::new(1.5, ROXO));
    }

    let mut x = rect.left() + 10.0;
    pintor.text(
        egui::pos2(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        format!("{numero:03}"),
        egui::FontId::monospace(12.0),
        egui::Color32::from_gray(140),
    );
    x += 40.0;
    if let Some(textura) = logo {
        let quadro = egui::Rect::from_min_size(egui::pos2(x, rect.center().y - 14.0), egui::vec2(34.0, 28.0));
        egui::Image::new(&textura)
            .maintain_aspect_ratio(true)
            .fit_to_exact_size(quadro.size())
            .paint_at(ui, quadro);
    }
    x += 42.0;
    let meio = if no_ar.is_some() { rect.center().y - 8.0 } else { rect.center().y };
    pintor.text(
        egui::pos2(x, meio),
        egui::Align2::LEFT_CENTER,
        nome,
        egui::FontId::proportional(14.5),
        egui::Color32::from_gray(235),
    );
    if let Some(programa) = no_ar {
        let cabe = ((rect.width() - x + rect.left() - 70.0) / 6.2) as usize;
        let corte: String = programa.chars().take(cabe.max(12)).collect();
        pintor.text(
            egui::pos2(x, rect.center().y + 9.0),
            egui::Align2::LEFT_CENTER,
            corte,
            egui::FontId::proportional(11.5),
            egui::Color32::from_gray(140),
        );
    }
    let direita = rect.right() - 10.0;
    if favorito {
        pintor.text(
            egui::pos2(direita, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            "★",
            egui::FontId::proportional(14.0),
            ROSA,
        );
    } else if fontes > 1 {
        pintor.text(
            egui::pos2(direita, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            format!("{fontes} fontes"),
            egui::FontId::proportional(10.5),
            egui::Color32::from_gray(110),
        );
    }
    resposta
}

/// Nome, número e fonte do que está tocando; some sozinho.
fn letreiro(app: &App, ctx: &egui::Context) {
    if let Some(filme) = app.tocando_vod.as_ref() {
        if Instant::now() > app.letreiro_ate && !app.lista_aberta {
            return;
        }
        egui::Area::new(egui::Id::new("letreiro-vod"))
            .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(24.0, -24.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 170))
                    .rounding(12.0)
                    .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(&filme.titulo).size(20.0).strong());
                        ui.label(
                            egui::RichText::new(format!(
                                "fonte {}/{}",
                                filme.fonte + 1,
                                filme.urls.len()
                            ))
                            .size(12.0)
                            .weak(),
                        );
                    });
            });
        return;
    }
    let Some(tocando) = app.tocando.as_ref() else { return };
    if Instant::now() > app.letreiro_ate && !app.lista_aberta {
        return;
    }
    let Some(canal) = app.canais.get(tocando.canal) else { return };
    let host = canal
        .fontes
        .get(tocando.fonte)
        .and_then(|f| f.url.split("://").nth(1))
        .and_then(|resto| resto.split('/').next())
        .unwrap_or("");
    egui::Area::new(egui::Id::new("letreiro"))
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(24.0, -24.0))
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 170))
                .rounding(12.0)
                .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{:03}", tocando.canal + 1))
                                .size(22.0)
                                .color(ROXO)
                                .strong(),
                        );
                        ui.label(egui::RichText::new(&canal.nome).size(22.0).strong());
                        if app.pausado {
                            ui.label(egui::RichText::new("pausado").size(12.0).weak());
                        }
                    });
                    let qualidade = app
                        .mpv
                        .as_ref()
                        .and_then(|m| m.ler("video-params/h"))
                        .map(|altura| format!(" · {altura}p"))
                        .unwrap_or_default();
                    ui.label(
                        egui::RichText::new(format!(
                            "fonte {}/{} · {host}{qualidade}",
                            tocando.fonte + 1,
                            canal.fontes.len()
                        ))
                        .size(12.0)
                        .weak(),
                    );
                });
        });
}

fn avisos(app: &mut App, ctx: &egui::Context) {
    let Some((texto, quando)) = app.aviso.clone() else { return };
    if quando.elapsed() > Duration::from_secs(4) {
        app.aviso = None;
        return;
    }
    ctx.request_repaint_after(Duration::from_millis(250));
    egui::Area::new(egui::Id::new("aviso"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-24.0, 24.0))
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_unmultiplied(20, 20, 26, 230))
                .rounding(10.0)
                .inner_margin(egui::Margin::symmetric(14.0, 10.0))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(texto).size(13.0));
                });
        });
}

fn atualizacao_na_tela(app: &mut App, ctx: &egui::Context) {
    let Some(versao) = app.nova_versao.clone() else { return };
    let mut aberta = true;
    egui::Window::new("Atualização")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .open(&mut aberta)
        .show(ctx, |ui| {
            ui.set_max_width(460.0);
            ui.label(
                egui::RichText::new(format!("Versão {} disponível", versao.numero))
                    .size(16.0)
                    .strong(),
            );
            ui.label(egui::RichText::new(format!("Você está na {}", crate::VERSAO)).weak());
            if !versao.notas.is_empty() {
                ui.add_space(8.0);
                egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                    ui.label(versao.notas.chars().take(800).collect::<String>());
                });
            }
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(
                    "O download abre no navegador. Depois é só fechar o Saimo TV e descompactar por cima.",
                )
                .size(11.0)
                .weak(),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Baixar").clicked() {
                    atualizacao::abrir_no_navegador(&versao.link);
                    atualizacao::adiar(&versao);
                    app.nova_versao = None;
                }
                if ui.button("Depois").clicked() {
                    atualizacao::adiar(&versao);
                    app.nova_versao = None;
                }
            });
        });
    if !aberta {
        atualizacao::adiar(&versao);
        app.nova_versao = None;
    }
}

/// Usado pela lista: nome sem acento para a busca.
pub fn _chave(texto: &str) -> String {
    catalogo::chave_de_ordem(texto)
}

/// Filmes e séries: as letras em cima, a lista da letra embaixo.
fn acervo(app: &mut App, ui: &mut egui::Ui) {
    let letras: Vec<String> = if app.gavetas.is_empty() {
        Vec::new()
    } else {
        app.gavetas.iter().map(|g| g.letra.clone()).collect()
    };
    let mut escolhida: Option<String> = None;
    ui.horizontal_wrapped(|ui| {
        for letra in &letras {
            let quantos = app
                .gavetas
                .iter()
                .find(|g| g.letra == *letra)
                .map(|g| if app.aba == Aba::Filmes { g.filmes } else { g.series })
                .unwrap_or(0);
            let rotulo = ui.selectable_label(*letra == app.letra, letra);
            if rotulo.on_hover_text(format!("{quantos} títulos")).clicked() {
                escolhida = Some(letra.clone());
            }
        }
    });
    if let Some(letra) = escolhida {
        app.busca.clear();
        app.abrir_letra(letra);
        return;
    }
    ui.add_space(6.0);

    if let Some(serie) = app.serie_aberta.clone() {
        episodios_da_serie(app, ui, &serie);
        return;
    }

    if app.carregando_vod {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(egui::RichText::new("buscando…").weak());
        });
        return;
    }

    let busca = catalogo::chave_de_ordem(app.busca.trim());
    let combina = |titulo: &str| busca.is_empty() || catalogo::chave_de_ordem(titulo).contains(&busca);

    // Com três letras digitadas a procura passa a valer no acervo inteiro, e
    // não só na letra aberta — trinta mil títulos, quase todos fora dela.
    if busca.chars().count() >= 3 {
        let serie = app.aba == Aba::Series;
        let achados: Vec<vod::Achado> = app
            .acervo
            .iter()
            .filter(|a| a.serie == serie && catalogo::chave_de_ordem(&a.titulo).contains(&busca))
            .take(400)
            .cloned()
            .collect();
        let mesma_letra: Vec<&vod::Achado> = achados.iter().filter(|a| a.letra == app.letra).collect();
        if achados.is_empty() && mesma_letra.is_empty() {
            ui.label(egui::RichText::new("nada no acervo").weak());
            return;
        }
        let mut ir_para: Option<vod::Achado> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for achado in &achados {
                let detalhe = if achado.letra == app.letra {
                    achado.ano.clone()
                } else {
                    format!("letra {}", achado.letra)
                };
                if item_do_acervo(ui, &achado.titulo, &detalhe, false).clicked() {
                    ir_para = Some(achado.clone());
                }
            }
        });
        if let Some(achado) = ir_para {
            if achado.letra != app.letra {
                app.abrir_letra(achado.letra.clone());
                app.busca = achado.titulo.clone();
            } else if achado.serie {
                if let Some(serie) = app.series.iter().find(|s| s.titulo == achado.titulo).cloned() {
                    app.abrir_serie(serie);
                }
            } else if let Some(filme) = app.filmes.iter().find(|f| f.titulo == achado.titulo).cloned() {
                if let Some((_, urls)) = filme.versoes.first().cloned() {
                    app.tocar_vod(filme.titulo, urls, 0, true);
                }
            }
        }
        return;
    }

    let mut tocar: Option<(String, Vec<String>)> = None;
    let mut abrir: Option<vod::Serie> = None;

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if app.aba == Aba::Filmes {
            let lista: Vec<&vod::Filme> = app.filmes.iter().filter(|f| combina(&f.titulo)).collect();
            if lista.is_empty() {
                ui.label(egui::RichText::new("nada nesta letra").weak());
            }
            for (posicao, filme) in lista.iter().enumerate() {
                let versoes = filme
                    .versoes
                    .iter()
                    .map(|(v, urls)| format!("{v} ({})", urls.len()))
                    .collect::<Vec<_>>()
                    .join(" · ");
                if item_do_acervo(ui, &filme.titulo, &versoes, posicao == app.foco_vod).clicked() {
                    app.foco_vod = posicao;
                    tocar = Some((filme.titulo.clone(), filme.versoes[0].1.clone()));
                }
            }
        } else {
            let lista: Vec<&vod::Serie> = app.series.iter().filter(|s| combina(&s.titulo)).collect();
            if lista.is_empty() {
                ui.label(egui::RichText::new("nada nesta letra").weak());
            }
            for (posicao, serie) in lista.iter().enumerate() {
                let detalhe = format!(
                    "{}{} episódios",
                    if serie.ano.is_empty() { String::new() } else { format!("{} · ", serie.ano) },
                    serie.episodios
                );
                if item_do_acervo(ui, &serie.titulo, &detalhe, posicao == app.foco_vod).clicked() {
                    app.foco_vod = posicao;
                    abrir = Some((*serie).clone());
                }
            }
        }
    });

    if let Some((titulo, urls)) = tocar {
        app.tocar_vod(titulo, urls, 0, true);
    }
    if let Some(serie) = abrir {
        app.abrir_serie(serie);
    }
}

fn episodios_da_serie(app: &mut App, ui: &mut egui::Ui, serie: &vod::Serie) {
    ui.horizontal(|ui| {
        if ui.button("← voltar").clicked() {
            app.serie_aberta = None;
            app.episodios.clear();
        }
        ui.label(egui::RichText::new(&serie.titulo).strong());
    });
    ui.add_space(4.0);
    if app.carregando_vod {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(egui::RichText::new("buscando episódios…").weak());
        });
        return;
    }

    let mut tocar: Option<(String, Vec<String>)> = None;
    let mut temporada_atual = u32::MAX;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (posicao, episodio) in app.episodios.iter().enumerate() {
            if episodio.temporada != temporada_atual {
                temporada_atual = episodio.temporada;
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!("TEMPORADA {temporada_atual}"))
                        .size(11.0)
                        .color(ROXO)
                        .strong(),
                );
            }
            let nome = format!("T{} E{}", episodio.temporada, episodio.numero);
            if item_do_acervo(ui, &nome, &episodio.versao, posicao == app.foco_vod).clicked() {
                tocar = Some((
                    format!("{} · {nome}", serie.titulo),
                    episodio.urls.clone(),
                ));
            }
        }
    });
    if let Some((titulo, urls)) = tocar {
        app.tocar_vod(titulo, urls, 0, true);
    }
}

fn item_do_acervo(ui: &mut egui::Ui, titulo: &str, detalhe: &str, focado: bool) -> egui::Response {
    let (rect, resposta) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 38.0), egui::Sense::click());
    let pintor = ui.painter();
    if focado || resposta.hovered() {
        let cor = if focado {
            egui::Color32::from_rgba_unmultiplied(139, 92, 246, 45)
        } else {
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 14)
        };
        pintor.rect_filled(rect, 8.0, cor);
    }
    pintor.text(
        egui::pos2(rect.left() + 10.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        titulo,
        egui::FontId::proportional(14.0),
        egui::Color32::from_gray(232),
    );
    if !detalhe.is_empty() {
        pintor.text(
            egui::pos2(rect.right() - 10.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            detalhe,
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(120),
        );
    }
    resposta
}

/// A grade do canal em foco: o que está no ar e o que vem pela frente.
fn guia(app: &mut App, ctx: &egui::Context) {
    let canal = app
        .tocando
        .as_ref()
        .map(|t| t.canal)
        .unwrap_or(app.foco);
    let Some(nome) = app.canais.get(canal).map(|c| c.nome.clone()) else { return };
    let programas = epg::grade(&nome);
    let instante = epg::agora();

    egui::SidePanel::right("guia")
        .exact_width((ctx.screen_rect().width() * 0.32).clamp(300.0, 460.0))
        .frame(
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_unmultiplied(12, 12, 16, 235))
                .inner_margin(egui::Margin::symmetric(14.0, 12.0)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Guia").size(18.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("fechar").clicked() {
                        app.guia_aberto = false;
                    }
                });
            });
            ui.label(egui::RichText::new(&nome).size(13.0).color(ROXO));
            ui.add_space(6.0);

            if programas.is_empty() {
                ui.label(
                    egui::RichText::new(if epg::canais_com_guia() == 0 {
                        "montando o guia…"
                    } else {
                        "este canal não tem programação publicada"
                    })
                    .weak(),
                );
                return;
            }

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for programa in programas.iter().filter(|p| p.fim > instante) {
                    let no_ar = programa.no_ar(instante);
                    ui.horizontal_top(|ui| {
                        ui.label(
                            egui::RichText::new(epg::hora_local(programa.inicio))
                                .monospace()
                                .size(12.0)
                                .color(if no_ar { ROSA } else { egui::Color32::from_gray(120) }),
                        );
                        ui.vertical(|ui| {
                            let titulo = egui::RichText::new(&programa.titulo).size(13.5);
                            ui.label(if no_ar { titulo.strong() } else { titulo });
                            if no_ar {
                                ui.label(egui::RichText::new(programa.horario()).size(11.0).weak());
                            }
                            if no_ar {
                                ui.add(
                                    egui::ProgressBar::new(programa.andamento(instante))
                                        .desired_height(4.0)
                                        .fill(ROXO),
                                );
                                if !programa.descricao.is_empty() {
                                    ui.label(
                                        egui::RichText::new(
                                            programa.descricao.chars().take(220).collect::<String>(),
                                        )
                                        .size(11.5)
                                        .weak(),
                                    );
                                }
                            }
                        });
                    });
                    ui.add_space(6.0);
                }
            });
        });
}
