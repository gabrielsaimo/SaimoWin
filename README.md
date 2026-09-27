# Saimo TV para Windows

Aplicativo nativo: janela, lista e letreiro desenhados pela GPU (egui sobre
OpenGL) e vídeo pelo mpv. Não há navegador nem página HTML em lugar nenhum.

Versão atual: **2.0.0**. O instalador sai no release único de
[SaimoPlayer](https://github.com/gabrielsaimo/SaimoPlayer/releases/latest), e o
programa avisa quando há versão nova.

## Novidades da 2.0

- **Pular abertura e recapitulação** com os tempos do
  [TheIntroDB](https://theintrodb.org) (`src/pulos.rs`): o botão aparece só
  quando o trecho foi marcado para aquele episódio. Enter pula.
- **Próximo episódio** nos créditos, seguindo sozinho em 10 s. Enter assiste
  agora, Esc dispensa. Sem créditos marcados, o cartão sobe nos últimos 30 s.
- Episódio que acaba emenda no seguinte, na mesma versão (dublado/legendado).
- Fonte que "termina" nos primeiros 30 s é tratada como quebrada: vai para a
  próxima em vez de fechar.
- Abre no último canal assistido.

| Peça | O que faz |
|---|---|
| `src/main.rs` | estado do app, teclado, troca de fonte |
| `src/tela.rs` | desenho: vídeo no fundo, lista e letreiro por cima |
| `src/mpv.rs` | ponte com o libmpv, carregado da DLL em tempo de execução |
| `src/catalogo.rs` | a mesma lista publicada dos outros aplicativos |
| `src/telemetria.rs` | Saimo Monitor, plataforma `windows` |
| `src/atualizacao.rs` | versão nova no GitHub, download pelo navegador |
| `src/pulos.rs` | TheIntroDB: abertura, recapitulação e créditos |
| `src/vod.rs`, `src/progresso.rs` | acervo e onde cada título parou |

## Montar o pacote

Do próprio Mac, com o mingw-w64 (`brew install mingw-w64`):

```bash
cd "/Volumes/SSD 1TB/DEV/Saimo/SaimoWin" && ./empacotar.sh
```

Sai `dist/SaimoTV-Instalador.msi`: o executável, a `libmpv-2.dll` e o
instalador que põe os dois em `%LOCALAPPDATA%\Programs\Saimo TV`, com atalho
no menu Iniciar e na Área de Trabalho, ficha em "Aplicativos instalados" e
desinstalação pelo painel do Windows. Instala sem senha de administrador,
porque instala só para quem está usando o computador.

Precisa do `wixl` (`brew install msitools`). As telas do assistente estão em
`instalador/ui`, que é a cópia traduzida das do WiX. O `--baixar` pega a
biblioteca do mpv de novo (compilação oficial do shinchiro/mpv-winbuild-cmake).

## Rodar no Mac para conferir a interface

O binário do Mac serve para ver lista, busca e teclado sem um PC por perto:

```bash
cargo run --release
cargo test                                   # inclui a leitura do TheIntroDB
cargo check --target x86_64-pc-windows-gnu   # confere o alvo de verdade
```

`SAIMO_MPV` aponta para outra biblioteca do mpv e `SAIMO_SEM_TELEMETRIA=1`
impede que o teste vire aparelho no painel.

## Guia e acervo

O guia sai das mesmas fontes dos outros aplicativos: meuguia.tv para a TV aberta
e os canais grandes, o guia da própria Pluto TV casado pelo id que está no link
do canal, e dois feeds XMLTV para o resto. Fica guardado por seis horas.

Filmes e séries vêm do acervo publicado em `vod/`, fatiado por letra, com capa
do TMDB (a mesma busca pontuada do TV Box), favoritos e "continuar de onde parou".

A capa é procurada só para as linhas visíveis: a letra "A" tem 2.672 filmes.
