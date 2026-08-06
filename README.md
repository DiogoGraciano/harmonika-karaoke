# Harmonika&Co Karaokê

App desktop (Tauri) que instala o karaokê da empresa numa máquina **Windows** ou
**Linux**: baixa o UltraStar Deluxe oficial, confere o SHA-256, extrai, injeta o
tema com a identidade visual da Harmonika&Co e cria o atalho.

**Não pede senha de administrador** — tudo acontece dentro da pasta do usuário.

## O que ele faz

- **Instalar** — baixa o artefato oficial do UltraStar (AppImage no Linux, zip
  portable no Windows), verifica a integridade, extrai e aplica a marca.
- **Abrir o karaokê** — inicia o app já instalado.
- **Pasta de músicas** — abre a pasta onde as músicas entram.
- **Jukebox** — põe o acervo do [usdb.eu](https://usdb.animux.de) na rede da
  festa, para os convidados procurarem música pelo celular.
- **Desinstalar** — remove o app, o jukebox e os atalhos. Músicas e
  configuração ficam.

## Onde ele instala

| | Linux | Windows |
|---|---|---|
| Aplicativo | `~/.local/share/HarmonikaKaraoke` | `%LOCALAPPDATA%\HarmonikaKaraoke` |
| Binário | `usr/bin/ultrastardx` | `ultrastardx.exe` |
| Temas | `usr/share/ultrastardx/themes` | `themes` |
| Config | `~/.ultrastardx/harmonika-config.ini` | `%APPDATA%\ultrastardx\harmonika-config.ini` |
| Músicas | `~/.ultrastardx/songs` | `%APPDATA%\ultrastardx\songs` |
| Atalho | menu de aplicativos + `~/.local/bin/harmonika-karaoke` | Menu Iniciar + Área de Trabalho |

O atalho abre **este painel**, não o UltraStar direto: é daqui que se chega ao
jukebox, que o karaokê sozinho não tem. De dentro dele, "Abrir o karaokê" inicia
o UltraStar com a configuração certa.

Ele aponta para onde o painel está agora. Num `.deb` ou no instalador do Windows
isso é um caminho fixo do sistema e não muda. Rodando pelo **AppImage**, o
atalho aponta para o arquivo `.AppImage` — mover ou apagar esse arquivo quebra o
atalho, e aí é só rodar o instalador de novo do lugar novo. (O caminho vem da
variável `APPIMAGE`, e não do `current_exe`: dentro de um AppImage o segundo
devolve o squashfs montado em `/tmp`, que some quando o app fecha.)
| Jukebox | `~/.local/share/HarmonikaKaraoke-jukebox` | `%LOCALAPPDATA%\HarmonikaKaraoke-jukebox` |

Os layouts diferem porque os artefatos oficiais diferem: o AppImage extraído tem
`usr/bin` + `usr/share`, enquanto o zip portable do Windows joga o `.exe` e a
pasta `themes/` na mesma raiz.

O jukebox fica **ao lado** da instalação, não dentro dela: reinstalar o karaokê
apaga `HarmonikaKaraoke` inteiro, e são 170 MB que ninguém quer baixar de novo.

## Instalação silenciosa

Para implantar em várias máquinas sem abrir a janela:

```bash
instalador-harmonika-karaoke --instalar
```

Imprime o andamento no terminal e sai com `0` (sucesso) ou `1` (falha). É o mesmo
caminho de código da instalação pela janela — só muda para onde o andamento vai.

## Como adicionar músicas

Cada música é uma pasta dentro da pasta de músicas, com um arquivo `.txt` (letra
+ notas, formato UltraStar) mais o áudio, e opcionalmente capa e vídeo:

```
songs/
└── Artista - Musica/
    ├── Artista - Musica.txt
    ├── Artista - Musica.mp3
    ├── Artista - Musica [CO].jpg   (capa, opcional)
    └── Artista - Musica [VD].mp4   (vídeo, opcional)
```

O karaokê relê a pasta a cada inicialização.

## O jukebox

Em vez de encher a pasta na mão, o instalador sobe o acervo do
[usdb.eu](https://usdb.animux.de) na rede local: quem está na festa aponta a
câmera para o QR code na tela e procura música pelo próprio celular.

Quem faz o trabalho é o [USDB Syncer](https://github.com/bohning/usdb_syncer),
no modo `serve`. O instalador baixa o release oficial dele, confere o SHA-256 e
cuida do processo — mesmo caminho do UltraStar.

### O passo a passo

1. **Instalar o jukebox** — baixa o USDB Syncer (~170 MB) e, se a máquina não
   tiver, o `ffmpeg`.
2. **Abrir o sincronizador** — uma vez, antes da festa. É aqui que a pessoa
   entra na conta do usdb.eu (grátis, cada um cria a sua) e espera a lista de
   músicas carregar. **Sem esse login o jukebox abre vazio**: a página lê o
   banco local, e quem o preenche é essa janela.
3. **Abrir o jukebox** — sobe o servidor e mostra o QR code com o endereço.

O jukebox vive enquanto o instalador estiver aberto. Ao fechar a janela o
processo é encerrado, para não deixar servidor órfão segurando a porta.

### O que os convidados conseguem fazer

Depende do release do sincronizador, e o instalador **pergunta ao próprio
binário** em vez de comparar número de versão — o recurso se acende sozinho
quando o release novo sair:

| | 0.24.0 (atual) | 0.25.0 (ainda não lançado) |
|---|---|---|
| Procurar e ordenar o acervo | sim | sim |
| Votar nas músicas | sim | sim |
| Ouvir o que já está baixado | sim | sim |
| Pedir download pelo celular | **não** | sim (`--allow-downloading`) |

Ou seja: hoje o jukebox é vitrine e votação, e quem baixa é o operador, pela
janela do sincronizador. O `--allow-downloading` já está no `main` do projeto.

### Detalhes que custaram tempo

**O endereço.** O `--host` do release 0.24.0 está quebrado — o argumento é
declarado como inteiro, então `--host 192.168.0.10` morre com
`invalid int value`. Quem escolhe o IP é o servidor, com um truque de socket
UDP. O instalador repete **o mesmo truque** para montar o QR code; se as duas
detecções divergissem, o QR apontaria para um endereço onde não há ninguém.

**O ffmpeg.** O sincronizador não embute `ffmpeg`/`ffprobe` — ele os chama pelo
PATH para converter o áudio que o yt-dlp baixa, e sem eles todo download falha
na última etapa. Quando a máquina já tem os dois, não baixamos nada. Quando não
tem, vem um build estático do
[BtbN](https://github.com/BtbN/FFmpeg-Builds) para dentro da pasta do usuário —
ainda sem pedir senha de administrador. A URL aponta para uma tag `autobuild-*`,
que é imutável; as tags `latest` são reescritas a cada build e o SHA-256 fixado
deixaria de bater no dia seguinte. Um teste garante isso.

**A configuração do sincronizador.** Nada é escrito nela. Em vez de mexer no
QSettings dele (que no Windows é o registro), o instalador passa `--songpath` e
ajusta o `PATH` só do processo filho. Mesmo efeito, sem deixar rastro na máquina
— e sem quebrar se o sincronizador renomear uma chave.

### Licença

O código deste repositório é **MIT** — veja [LICENSE](LICENSE).

O USDB Syncer é **GPL-3.0-only** e o ffmpeg do BtbN é um build GPL. Nenhum dos
dois vai embutido neste repositório ou nos instaladores: os dois são baixados do
release oficial na hora da instalação, exatamente como o UltraStar. Assim este
projeto não vira redistribuidor deles.

### Trocar a versão do sincronizador

Em [src-tauri/src/jukebox.rs](src-tauri/src/jukebox.rs), no bloco de constantes:
`SYNCER_VERSION`, `SYNCER_URL` e `SYNCER_SHA256`. Os hashes saem do próprio
release. Os testes conferem que a URL aponta para a versão declarada.

## A identidade visual

Paleta extraída direto das imagens da marca:

| Cor | Hex | RGB |
|---|---|---|
| Laranja claro | `#FF9B53` | 255 155 83 |
| Laranja principal | `#F75C1F` | 247 92 31 |
| Laranja escuro | `#B03810` | 176 56 16 |
| Azul claro | `#3E6CFB` | 62 108 251 |
| Azul escuro | `#1D2F71` | 29 47 113 |

### Como o tema é montado

O instalador **não** carrega uma cópia do `Modern.ini` (160KB). Ele lê o
`Modern.ini` da versão que acabou de baixar e reescreve só as chaves da marca:

- `[Theme]` → `Name`, `Creator`, `DefaultSkin`
- `[Colors]` → `LightOrange` = `247 92 31`, `DarkOrange` = `176 56 16`
- o skin usa `Color=Orange`, então esse par vira a cor de botões, barras e
  destaques da interface inteira — sem reeditar textura por textura

Se o UltraStar renomear alguma dessas chaves numa versão futura, a reescrita
**falha alto** e a instalação para, em vez de gerar um tema sem a marca.

Só as imagens vão embutidas no binário (~340KB), porque a máquina de destino não
tem como gerá-las. Ficam prontas em `src-tauri/assets/`:

| Asset | Uso |
|---|---|
| `bg-load.jpg` | Tela de abertura (símbolo + logotipo + "KARAOKÊ") |
| `bg-main.jpg` | Fundo de todas as telas, com logotipo discreto no topo-direito |
| `icon-main.png` | Ícone do menu principal |
| `mark.png` | Símbolo isolado, em alta resolução |
| `icon-256.png` / `icon.ico` | Ícone do atalho (Linux / Windows) |

## Trocar a versão do UltraStar

Em [src-tauri/src/lib.rs](src-tauri/src/lib.rs), no bloco de constantes do
sistema correspondente: `USDX_VERSION`, `USDX_URL` e `USDX_SHA256`. Os hashes
saem da própria página de release do projeto UltraStar. Um teste garante que a
URL aponte para a versão declarada e que o hash tenha o formato certo.

## Desenvolvimento

Requisitos: Node 18+, Rust stable, e no Linux as dependências do Tauri.

```bash
# Arch / CachyOS
sudo pacman -S --needed base-devel webkit2gtk-4.1 librsvg \
    libappindicator-gtk3 curl wget file openssl

# Debian / Ubuntu
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
    libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf
```

```bash
npm install
npm run dev
```

Testes do backend:

```bash
cd src-tauri && cargo test
```

## Gerar os executáveis

```bash
npm run build:linux     # .deb e .AppImage
npm run build:windows   # .exe (NSIS), cross-compilado com cargo-xwin
npm run build:all
```

Saída em `src-tauri/target/**/release/bundle/`.

Para o build do Windows a partir do Linux:

```fish
rustup target add x86_64-pc-windows-msvc
sudo pacman -S --needed llvm clang lld
cargo install cargo-xwin
set -Ux XWIN_ACCEPT_LICENSE 1
```

Isso já gera o `instalador-harmonika-karaoke.exe` (o aplicativo em si). Para
empacotá-lo no instalador NSIS, falta o `makensis`, que no Arch/CachyOS só existe
no AUR:

```fish
yay -S nsis
```

Sem ele o build compila e linka normalmente, e para na última etapa com
`failed to run command makensis.exe`.

O `.msi` não sai do Linux — depende do WiX rodando em Windows.

**Pelo CI nada disso é necessário**: o runner do Windows é Windows de verdade, e
o Tauri baixa o NSIS sozinho. O workflow entrega `.exe` e `.msi` sem configuração
extra.

## Release

O workflow [release.yml](.github/workflows/release.yml) dispara ao empurrar uma
tag `v*`, roda os testes, compila nativamente em cada sistema e publica a release
com todos os instaladores:

```bash
git tag v1.0.0
git push origin v1.0.0
```

Também dá para disparar pela aba Actions, informando a tag na mão.

## Notas de implementação

Três coisas que não são óbvias e custaram tempo para descobrir:

**1. O tema precisa ficar dentro do aplicativo.** O UltraStar procura temas
apenas no caminho compartilhado da própria instalação. Um tema em
`~/.ultrastardx/themes/` nunca é encontrado — e o app ainda reescreve o
`config.ini` de volta para `Theme=Modern`, sem avisar. É por isso que o
instalador extrai o AppImage em vez de usá-lo direto.

**2. O karaokê não sobrevive ao `SCHED_IDLE`.** Se o painel estiver na classe de
agendamento ociosa, o UltraStar morre com
`EThread: Failed to create new thread` bem na hora de montar a lista de músicas.
A cthreads do Free Pascal cria cada `TThread` com `PTHREAD_EXPLICIT_SCHED`, isto
é, pedindo `SCHED_OTHER` explicitamente; o filho herdou `SCHED_IDLE` e o kernel
nega a volta com `EPERM`, porque o `RLIMIT_NICE` padrão é 0. Sair do
`SCHED_IDLE` exigiria `CAP_SYS_NICE`, então o botão "Abrir o karaokê" detecta a
situação e entrega a partida a um serviço transitório do systemd do usuário, que
roda em `SCHED_OTHER`.

Quem põe o painel em `SCHED_IDLE` é o **ananicy-cpp**, ligado por padrão no
CachyOS: a regra `node` → `BG_CPUIO` marca `nice 16` + `sched idle`, e tudo que
desce do `node` herda — inclusive o painel iniciado por `npm run dev`. Fora do
`SCHED_IDLE` nada muda: o binário é executado direto, com o `LD_LIBRARY_PATH`
montado na mão apontando para as libs do AppImage extraído (e é `usr/bin/ultrastardx`,
não o `AppRun`, que na instalação extraída é só um symlink para ele).

**3. A configuração fica num arquivo próprio**, passado via `-ConfigFile`. Sem
isso, abrir um UltraStar comum na mesma máquina reescreve o tema de volta para o
padrão, quebrando a instalação da empresa.

E dois avisos que aparecem no log são normais — vêm do tema original do
UltraStar, não deste: `SongRouletteStaticCat does not exist` e
`JukeboxSongOptionsLyricSizeSlide does not exist`.
