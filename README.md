# Saimo TV para Windows

Aplicativo nativo: janela, lista e letreiro desenhados pela GPU (egui sobre
OpenGL) e vídeo pelo mpv. Não há navegador nem página HTML em lugar nenhum.

| Peça | O que faz |
|---|---|
| `src/main.rs` | estado do app, teclado, troca de fonte |
| `src/tela.rs` | desenho: vídeo no fundo, lista e letreiro por cima |
| `src/mpv.rs` | ponte com o libmpv, carregado da DLL em tempo de execução |
| `src/catalogo.rs` | a mesma lista publicada dos outros aplicativos |
| `src/telemetria.rs` | Saimo Monitor, plataforma `windows` |
| `src/atualizacao.rs` | versão nova no GitHub, download pelo navegador |

## Montar o pacote

Do próprio Mac, com o mingw-w64 (`brew install mingw-w64`):

```bash
cd "/Volumes/SSD 1TB/DEV/Saimo/SaimoWin" && ./empacotar.sh
```

Sai `dist/SaimoTV-Windows.zip` com o executável e a `libmpv-2.dll`. O
`--baixar` pega a biblioteca do mpv de novo (compilação oficial do
shinchiro/mpv-winbuild-cmake).

## Rodar no Mac para conferir a interface

O binário do Mac serve para ver lista, busca e teclado sem um PC por perto:

```bash
cargo run --release
```

`SAIMO_MPV` aponta para outra biblioteca do mpv e `SAIMO_SEM_TELEMETRIA=1`
impede que o teste vire aparelho no painel.

## Guia e acervo

O guia sai das mesmas fontes dos outros aplicativos: meuguia.tv para a TV aberta
e os canais grandes, o guia da própria Pluto TV casado pelo id que está no link
do canal, e dois feeds XMLTV para o resto. Fica guardado por seis horas.

Filmes e séries vêm do acervo publicado em `vod/`, fatiado por letra.

## O que ainda não tem

Capas dos filmes e continuar de onde parou.
