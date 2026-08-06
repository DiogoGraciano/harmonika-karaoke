//! Instalador do Harmonika&Co Karaoke.
//!
//! Baixa o artefato oficial do UltraStar Deluxe (AppImage no Linux, zip
//! portable no Windows), confere o SHA-256, extrai numa pasta do usuario,
//! injeta o tema com a identidade visual da empresa e cria o atalho.
//!
//! Nao precisa de privilegio de administrador em nenhum dos dois sistemas:
//! tudo acontece dentro da pasta do usuario.

mod jukebox;
mod paths;
mod theme;

use paths::Paths;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Flag do Windows que impede a criacao de uma janela de console.
#[cfg(windows)]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
pub(crate) fn powershell() -> Command {
    let mut c = Command::new("powershell");
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

// ---- Release do UltraStar que o instalador usa ----
// Os hashes vem da propria pagina de release do projeto. Se um dia subir a
// versao, troque os tres valores do bloco do sistema correspondente.

const USDX_VERSION: &str = "2026.6.0";

#[cfg(not(windows))]
const USDX_URL: &str = "https://github.com/UltraStar-Deluxe/USDX/releases/download/v2026.6.0/UltraStarDeluxe-linux-2026.6.0.AppImage";
#[cfg(not(windows))]
const USDX_SHA256: &str = "66585288840ffd12abcd0f2a4eaa80a6f5318a6b2e7a957c95ea94064711decd";
#[cfg(not(windows))]
const USDX_FILE: &str = "UltraStarDeluxe.AppImage";

#[cfg(windows)]
const USDX_URL: &str = "https://github.com/UltraStar-Deluxe/USDX/releases/download/v2026.6.0/UltraStarDeluxe-windows-portable-2026.6.0.zip";
#[cfg(windows)]
const USDX_SHA256: &str = "50b982cf3b1174b978156098463052f8df5f9e1757dc3334d1195ca6b25d1a84";
#[cfg(windows)]
const USDX_FILE: &str = "UltraStarDeluxe-portable.zip";

/// Nome exibido do produto, usado no cabecalho do lancador do Linux.
#[cfg(not(windows))]
pub(crate) const PRODUTO: &str = "Harmonika&Co Karaoke";

// ---- Assets da marca embutidos no binario (~340KB) ----
// Vao embutidos porque a maquina de destino nao tem como gera-los: eles sao
// derivados das imagens da marca com ImageMagick, na hora de compilar.

const BG_MAIN: &[u8] = include_bytes!("../assets/bg-main.jpg");
const BG_LOAD: &[u8] = include_bytes!("../assets/bg-load.jpg");
const ICON_MAIN: &[u8] = include_bytes!("../assets/icon-main.png");
const MARK: &[u8] = include_bytes!("../assets/mark.png");
// O icone do atalho vem em formatos diferentes: PNG no Linux, ICO no Windows.
#[cfg(not(windows))]
const ICON_256: &[u8] = include_bytes!("../assets/icon-256.png");
#[cfg(windows)]
const ICON_ICO: &[u8] = include_bytes!("../assets/icon.ico");

// ---- Tipos trocados com a interface ----

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub os: String,
    pub usdx_version: String,
    pub download_url: String,
    /// Explica ao usuario que nao vai pedir senha de administrador.
    pub elevation: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusResult {
    pub installed: bool,
    /// Versao do UltraStar registrada na instalacao (vazio se nao instalado).
    pub version: String,
    /// Quantas pastas de musica existem hoje.
    pub songs: usize,
    pub paths: Paths,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    step: String,
    pct: u8,
    message: String,
}

/// Para onde o andamento e reportado. A instalacao e a mesma nos dois modos —
/// pela janela (manda evento para a interface) ou pela linha de comando
/// (imprime no terminal).
pub(crate) type Reporter = Arc<dyn Fn(u8, &str, &str) + Send + Sync>;

/// Reporta o andamento na janela pelo evento `evento`. A instalacao do
/// karaoke e a do jukebox correm em cartoes diferentes da interface, entao
/// cada uma tem o seu evento e nao mexe na barra da outra.
pub(crate) fn reporter_evento(app: AppHandle, evento: &'static str) -> Reporter {
    Arc::new(move |pct, step, message| {
        let _ = app.emit(
            evento,
            Progress {
                step: step.to_string(),
                pct,
                message: message.to_string(),
            },
        );
    })
}

fn reporter_terminal() -> Reporter {
    Arc::new(|pct, step, message| {
        println!("[{pct:>3}%] {step}: {message}");
    })
}

/// Arquivo que marca a instalacao e guarda a versao instalada.
fn marker(p: &Paths) -> PathBuf {
    p.app_dir.join(".harmonika-versao")
}

// ---- Utilitarios de disco ----

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("erro ao criar {}: {e}", to.display()))?;
    let entries =
        fs::read_dir(from).map_err(|e| format!("erro ao ler {}: {e}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("erro ao listar {}: {e}", from.display()))?;
        let dest = to.join(entry.file_name());
        let kind = entry
            .file_type()
            .map_err(|e| format!("erro ao inspecionar {}: {e}", entry.path().display()))?;
        if kind.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), &dest)
                .map_err(|e| format!("erro ao copiar {}: {e}", entry.path().display()))?;
        }
    }
    Ok(())
}

pub(crate) fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("erro ao criar {}: {e}", parent.display()))?;
    }
    fs::write(path, bytes).map_err(|e| format!("erro ao gravar {}: {e}", path.display()))
}

#[cfg(unix)]
pub(crate) fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = fs::metadata(path)
        .map_err(|e| format!("erro ao ler permissoes de {}: {e}", path.display()))?
        .permissions();
    perm.set_mode(0o755);
    fs::set_permissions(path, perm)
        .map_err(|e| format!("erro ao dar permissao de execucao a {}: {e}", path.display()))
}

/// Conta pastas dentro do diretorio de musicas (cada musica e uma pasta).
fn count_songs(dir: &Path) -> usize {
    fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .count()
        })
        .unwrap_or(0)
}

// ---- Etapa 1: download com verificacao de integridade ----

/// Baixa `url` conferindo o SHA-256 contra `esperado`, reportando o andamento
/// na faixa `de..ate` da barra. `rotulo` e o nome que aparece para o usuario.
///
/// Um arquivo que nao confere e apagado e vira erro: melhor parar do que
/// instalar um binario que nao e o oficial.
pub(crate) async fn baixar(
    rep: &Reporter,
    url: &str,
    esperado: &str,
    destino: &Path,
    rotulo: &str,
    (de, ate): (u8, u8),
) -> Result<(), String> {
    use futures_util::StreamExt;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1800))
        .build()
        .map_err(|e| format!("erro ao criar cliente HTTP: {e}"))?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("falha ao baixar {rotulo}: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "o servidor respondeu {} ao baixar {rotulo}",
            resp.status()
        ));
    }

    let total = resp.content_length().unwrap_or(0);
    if let Some(parent) = destino.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("erro ao criar {}: {e}", parent.display()))?;
    }
    let mut arquivo = fs::File::create(destino)
        .map_err(|e| format!("erro ao criar {}: {e}", destino.display()))?;
    let mut hasher = Sha256::new();
    let mut baixado: u64 = 0;
    let mut ultimo_pct = u8::MAX;
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("conexao interrompida durante o download: {e}"))?;
        hasher.update(&chunk);
        arquivo
            .write_all(&chunk)
            .map_err(|e| format!("erro ao gravar o download: {e}"))?;
        baixado += chunk.len() as u64;

        // So emite quando o inteiro muda, senao inunda a interface de eventos.
        let pct = if total > 0 {
            de + (baixado * u64::from(ate - de) / total) as u8
        } else {
            de
        };
        if pct != ultimo_pct {
            ultimo_pct = pct;
            rep(
                pct,
                "download",
                &format!(
                    "Baixando {rotulo}… {} MB{}",
                    baixado / 1_048_576,
                    if total > 0 {
                        format!(" de {} MB", total / 1_048_576)
                    } else {
                        String::new()
                    }
                ),
            );
        }
    }
    arquivo
        .flush()
        .map_err(|e| format!("erro ao finalizar o download: {e}"))?;

    let hash = format!("{:x}", hasher.finalize());
    if hash != esperado {
        let _ = fs::remove_file(destino);
        return Err(format!(
            "o arquivo baixado de {rotulo} nao confere com o oficial \
             (SHA-256 {hash}). Download corrompido ou interceptado — \
             nada foi instalado."
        ));
    }
    Ok(())
}

// ---- Etapa 2: extracao (o formato muda por sistema) ----

#[cfg(not(windows))]
fn extrair(baixado: &Path, tmp: &Path, app_dir: &Path) -> Result<(), String> {
    make_executable(baixado)?;

    // O AppImage extrai para ./squashfs-root, relativo ao diretorio atual.
    let out = Command::new(baixado)
        .arg("--appimage-extract")
        .current_dir(tmp)
        .output()
        .map_err(|e| format!("falha ao extrair o AppImage: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "falha ao extrair o AppImage: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    let extraido = tmp.join("squashfs-root");
    if !extraido.is_dir() {
        return Err("o AppImage nao gerou a pasta esperada ao extrair".to_string());
    }
    // tmp e app_dir ficam no mesmo sistema de arquivos, entao rename funciona.
    fs::rename(&extraido, app_dir)
        .map_err(|e| format!("erro ao mover a instalacao para {}: {e}", app_dir.display()))
}

#[cfg(windows)]
fn extrair(baixado: &Path, _tmp: &Path, app_dir: &Path) -> Result<(), String> {
    let arquivo = fs::File::open(baixado)
        .map_err(|e| format!("erro ao abrir o arquivo baixado: {e}"))?;
    let mut zip = zip::ZipArchive::new(arquivo)
        .map_err(|e| format!("o arquivo baixado nao e um zip valido: {e}"))?;
    fs::create_dir_all(app_dir)
        .map_err(|e| format!("erro ao criar {}: {e}", app_dir.display()))?;
    zip.extract(app_dir)
        .map_err(|e| format!("falha ao extrair o zip do UltraStar: {e}"))
}

// ---- Etapa 3: injecao do tema da empresa ----

/// Cria o tema Harmonika dentro da instalacao. O UltraStar so le temas da
/// pasta de temas da propria instalacao — um tema na pasta do usuario e
/// simplesmente ignorado, e o app ainda reescreve a config de volta para o
/// tema padrao. Por isso o tema entra aqui dentro.
fn injetar_tema(p: &Paths) -> Result<(), String> {
    let modern_dir = p.themes_dir.join("Modern");
    let modern_ini = p.themes_dir.join("Modern.ini");
    let harmonika_dir = p.themes_dir.join(theme::THEME_NAME);

    if !modern_ini.is_file() || !modern_dir.is_dir() {
        return Err(
            "o tema Modern nao foi encontrado no UltraStar baixado — \
             o instalador precisa dele como base."
                .to_string(),
        );
    }

    // O tema da marca herda as texturas do Modern e troca so o que e da marca.
    let _ = fs::remove_dir_all(&harmonika_dir);
    copy_dir(&modern_dir, &harmonika_dir)?;

    // Fora os skins e fundos originais: o tema da empresa tem os seus.
    for entry in fs::read_dir(&harmonika_dir)
        .map_err(|e| format!("erro ao ler o tema copiado: {e}"))?
        .flatten()
    {
        let nome = entry.file_name().to_string_lossy().to_string();
        let sobra = nome.ends_with(".ini")
            || nome.starts_with("[bg-main]")
            || nome.starts_with("[bg-load]");
        if sobra {
            let _ = fs::remove_file(entry.path());
        }
    }

    // Imagens da marca.
    write_file(&harmonika_dir.join("[bg-main]harmonika.jpg"), BG_MAIN)?;
    write_file(&harmonika_dir.join("[bg-load]harmonika.jpg"), BG_LOAD)?;
    write_file(&harmonika_dir.join("[icon]main.png"), ICON_MAIN)?;
    write_file(&harmonika_dir.join("[brand]mark.png"), MARK)?;

    // Definicao do tema e do skin, derivadas do Modern desta versao.
    let modern = fs::read_to_string(&modern_ini)
        .map_err(|e| format!("erro ao ler o Modern.ini: {e}"))?;
    write_file(
        &p.themes_dir.join(format!("{}.ini", theme::THEME_NAME)),
        theme::theme_ini(&modern)?.as_bytes(),
    )?;

    let blue = fs::read_to_string(modern_dir.join("Blue.ini"))
        .map_err(|e| format!("erro ao ler o skin Blue.ini: {e}"))?;
    write_file(
        &harmonika_dir.join(format!("{}.ini", theme::THEME_NAME)),
        theme::skin_ini(&blue)?.as_bytes(),
    )?;

    Ok(())
}

// ---- Etapa 4: configuracao e atalho ----

fn escrever_config(p: &Paths) -> Result<(), String> {
    fs::create_dir_all(&p.songs)
        .map_err(|e| format!("erro ao criar a pasta de musicas: {e}"))?;
    let conteudo = theme::config_ini(&p.songs.to_string_lossy());
    write_file(&p.config, conteudo.as_bytes())
}

/// Onde este proprio programa esta, para o atalho apontar de volta para ele.
///
/// Dentro de um AppImage o `current_exe` devolve o caminho no squashfs
/// montado em `/tmp`, que deixa de existir assim que o app fecha — um atalho
/// para la nasceria quebrado. O caminho de verdade vem do `APPIMAGE`, que o
/// runtime do AppImage exporta. Nos pacotes .deb e no instalador do Windows
/// o binario ja mora num lugar fixo, e o `current_exe` basta.
fn caminho_do_painel() -> Result<PathBuf, String> {
    #[cfg(not(windows))]
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        let caminho = PathBuf::from(appimage);
        if caminho.is_file() {
            return Ok(caminho);
        }
    }
    std::env::current_exe()
        .map_err(|e| format!("nao foi possivel descobrir o caminho do instalador: {e}"))
}

// `_p` so existe para o gemeo do Windows, que ainda precisa do `app_dir` para
// guardar o .ico. Do lado do Linux o atalho e todo relativo a pasta do usuario.
#[cfg(not(windows))]
fn criar_atalho(_p: &Paths) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("nao foi possivel descobrir a pasta do usuario")?;
    let bin = home.join(".local/bin/harmonika-karaoke");
    let apps = home.join(".local/share/applications");
    let icone = home.join(".local/share/icons/hicolor/256x256/apps/harmonika-karaoke.png");
    let painel = caminho_do_painel()?;

    // O atalho abre o painel, nao o karaoke direto: e de la que se chega ao
    // jukebox, que o UltraStar sozinho nao tem. Quem cuida do karaoke em si
    // (LD_LIBRARY_PATH, -ConfigFile) e o botao "Abrir o karaoke" no painel.
    //
    // O script no meio do caminho existe porque o `Exec=` do .desktop tem
    // regras proprias de escape: um caminho com espaco ou aspas quebraria a
    // entrada do menu. Apontando para um caminho nosso, sem espacos, o
    // problema nao aparece.
    let script = format!(
        "#!/bin/sh\n\
         # {produto} — gerado pelo instalador\n\
         exec \"{painel}\" \"$@\"\n",
        produto = PRODUTO,
        painel = painel.display()
    );
    write_file(&bin, script.as_bytes())?;
    make_executable(&bin)?;
    write_file(&icone, ICON_256)?;

    let desktop = format!(
        "[Desktop Entry]\n\
         Version=1.0\n\
         Type=Application\n\
         Name=Harmonika&Co Karaokê\n\
         Comment=Karaokê corporativo da Harmonika&Co — cantar e jukebox\n\
         Icon=harmonika-karaoke\n\
         Exec={bin}\n\
         Terminal=false\n\
         StartupNotify=false\n\
         Categories=AudioVideo;Audio;Game;\n\
         Keywords=karaoke;karaokê;música;cantar;harmonika;\n",
        bin = bin.display()
    );
    write_file(&apps.join("harmonika-karaoke.desktop"), desktop.as_bytes())?;

    // Best-effort: se nao existir, o atalho aparece no proximo login.
    let _ = Command::new("update-desktop-database").arg(&apps).output();
    Ok(())
}

#[cfg(windows)]
fn criar_atalho(p: &Paths) -> Result<(), String> {
    let ico = p.app_dir.join("harmonika.ico");
    write_file(&ico, ICON_ICO)?;

    // O atalho abre o painel, nao o karaoke direto: e de la que se chega ao
    // jukebox, que o UltraStar sozinho nao tem.
    let painel = caminho_do_painel()?;
    let dir = painel
        .parent()
        .map(|d| d.to_path_buf())
        .unwrap_or_else(|| p.app_dir.clone());

    // Aspas simples dobradas: e como o PowerShell escapa dentro de string.
    let q = |v: String| v.replace('\'', "''");
    let script = format!(
        "$ws = New-Object -ComObject WScript.Shell\n\
         $dests = @(\n\
         \x20 (Join-Path $env:APPDATA 'Microsoft\\Windows\\Start Menu\\Programs\\Harmonika&Co Karaoke.lnk'),\n\
         \x20 (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Harmonika&Co Karaoke.lnk')\n\
         )\n\
         foreach ($d in $dests) {{\n\
         \x20 $s = $ws.CreateShortcut($d)\n\
         \x20 $s.TargetPath = '{painel}'\n\
         \x20 $s.WorkingDirectory = '{dir}'\n\
         \x20 $s.IconLocation = '{ico}'\n\
         \x20 $s.Description = 'Karaoke corporativo da Harmonika&Co — cantar e jukebox'\n\
         \x20 $s.Save()\n\
         }}\n",
        painel = q(painel.display().to_string()),
        dir = q(dir.display().to_string()),
        ico = q(ico.display().to_string())
    );

    let out = powershell()
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .output()
        .map_err(|e| format!("falha ao criar o atalho: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "falha ao criar o atalho: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

#[cfg(not(windows))]
fn remover_atalho() {
    if let Some(home) = dirs::home_dir() {
        let _ = fs::remove_file(home.join(".local/bin/harmonika-karaoke"));
        let _ = fs::remove_file(home.join(".local/share/applications/harmonika-karaoke.desktop"));
        let _ = fs::remove_file(
            home.join(".local/share/icons/hicolor/256x256/apps/harmonika-karaoke.png"),
        );
    }
}

#[cfg(windows)]
fn remover_atalho() {
    let script = "@(\n\
         (Join-Path $env:APPDATA 'Microsoft\\Windows\\Start Menu\\Programs\\Harmonika&Co Karaoke.lnk'),\n\
         (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Harmonika&Co Karaoke.lnk')\n\
         ) | ForEach-Object { Remove-Item -LiteralPath $_ -ErrorAction SilentlyContinue }\n";
    let _ = powershell()
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script])
        .output();
}

// ---- Comandos Tauri ----

#[tauri::command]
fn platform_info() -> PlatformInfo {
    let os = if cfg!(windows) { "windows" } else { "linux" };
    PlatformInfo {
        os: os.to_string(),
        usdx_version: USDX_VERSION.to_string(),
        download_url: USDX_URL.to_string(),
        elevation: "Instala na sua pasta de usuário — não pede senha de administrador."
            .to_string(),
    }
}

#[tauri::command]
fn status() -> Result<StatusResult, String> {
    let p = paths::resolve()?;
    let installed = p.exe.is_file();
    let version = fs::read_to_string(marker(&p))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let songs = count_songs(&p.songs);

    let detail = if !installed {
        "Ainda não instalado nesta máquina.".to_string()
    } else if version.is_empty() {
        "Instalado (versão não registrada).".to_string()
    } else if version == USDX_VERSION {
        format!("Instalado e atualizado (UltraStar {version}).")
    } else {
        format!("Instalado na versão {version} — este instalador traz a {USDX_VERSION}.")
    };

    Ok(StatusResult {
        installed,
        version,
        songs,
        paths: p,
        detail,
    })
}

/// Instalacao completa. E o mesmo caminho de codigo para a janela e para a
/// linha de comando — so muda para onde o andamento e reportado.
pub async fn instalar_com(rep: Reporter) -> Result<StatusResult, String> {
    let p = paths::resolve()?;

    // Area de trabalho no mesmo sistema de arquivos do destino, para que o
    // rename da extracao seja instantaneo em vez de copiar tudo de novo.
    let tmp = p.app_dir.with_file_name(format!("{}.tmp", paths::APP_FOLDER));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|e| format!("erro ao criar a pasta temporaria: {e}"))?;

    let baixado = tmp.join(USDX_FILE);
    let resultado = async {
        rep(0, "download", "Conectando ao servidor do UltraStar…");
        baixar(&rep, USDX_URL, USDX_SHA256, &baixado, "o UltraStar", (0, 60)).await?;

        let p2 = p.clone();
        let tmp2 = tmp.clone();
        let rep2 = Arc::clone(&rep);
        tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
            rep2(62, "extracao", "Verificado. Extraindo os arquivos…");
            let _ = fs::remove_dir_all(&p2.app_dir);
            if let Some(parent) = p2.app_dir.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("erro ao criar {}: {e}", parent.display()))?;
            }
            extrair(&baixado, &tmp2, &p2.app_dir)?;

            rep2(78, "tema", "Aplicando a identidade visual…");
            injetar_tema(&p2)?;

            rep2(88, "config", "Escrevendo a configuração…");
            escrever_config(&p2)?;

            rep2(94, "atalho", "Criando o atalho…");
            criar_atalho(&p2)?;

            write_file(&marker(&p2), USDX_VERSION.as_bytes())?;
            Ok(())
        })
        .await
        .map_err(|e| format!("erro interno durante a instalacao: {e}"))?
    }
    .await;

    let _ = fs::remove_dir_all(&tmp);
    resultado?;

    rep(100, "pronto", "Instalação concluída.");
    status()
}

#[tauri::command]
async fn install(app: AppHandle) -> Result<StatusResult, String> {
    instalar_com(reporter_evento(app, "progresso")).await
}

/// Instalacao sem janela, para implantar em varias maquinas de uma vez:
/// `instalador-harmonika-karaoke --instalar`. Devolve o codigo de saida.
pub fn instalar_cli() -> i32 {
    match tauri::async_runtime::block_on(instalar_com(reporter_terminal())) {
        Ok(st) => {
            println!("Instalado em {}", st.paths.app_dir.display());
            println!("Musicas em  {}", st.paths.songs.display());
            0
        }
        Err(e) => {
            eprintln!("Falhou: {e}");
            1
        }
    }
}

#[tauri::command]
async fn uninstall(jukebox: State<'_, jukebox::Estado>) -> Result<StatusResult, String> {
    // O jukebox sai antes: derruba o servidor e libera a pasta de 170 MB.
    jukebox::remover(&jukebox)?;

    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let p = paths::resolve()?;
        // As musicas e a configuracao ficam — sao do usuario, nao do app.
        // Se a pasta ja nao existe, ainda vale limpar os atalhos orfaos.
        if p.app_dir.exists() {
            fs::remove_dir_all(&p.app_dir)
                .map_err(|e| format!("erro ao remover a instalacao: {e}"))?;
        }
        remover_atalho();
        Ok(())
    })
    .await
    .map_err(|e| format!("erro interno ao desinstalar: {e}"))??;
    status()
}

// ---- Comandos do jukebox ----

#[tauri::command]
fn jukebox_status(
    jukebox: State<'_, jukebox::Estado>,
) -> Result<jukebox::JukeboxStatus, String> {
    jukebox::status(&jukebox)
}

#[tauri::command]
async fn jukebox_install(app: AppHandle) -> Result<jukebox::JukeboxStatus, String> {
    jukebox::instalar_com(reporter_evento(app, "progresso-jukebox")).await
}

#[tauri::command]
fn jukebox_configure() -> Result<(), String> {
    jukebox::configurar()
}

/// Subir o servidor espera ele atender, entao sai da thread da interface.
#[tauri::command]
async fn jukebox_start(app: AppHandle) -> Result<jukebox::JukeboxStatus, String> {
    tauri::async_runtime::spawn_blocking(move || jukebox::iniciar(&app.state()))
        .await
        .map_err(|e| format!("erro interno ao iniciar o jukebox: {e}"))?
}

#[tauri::command]
fn jukebox_stop(jukebox: State<'_, jukebox::Estado>) -> Result<jukebox::JukeboxStatus, String> {
    jukebox::parar(&jukebox)
}

/// As bibliotecas que vieram dentro do AppImage extraido, na frente do que o
/// nosso proprio processo ja tiver.
#[cfg(not(windows))]
fn ld_library_path(p: &Paths) -> String {
    let lib = p.app_dir.join("usr").join("lib");
    match std::env::var("LD_LIBRARY_PATH") {
        Ok(atual) if !atual.is_empty() => format!("{}:{atual}", lib.display()),
        _ => lib.to_string_lossy().to_string(),
    }
}

/// `SCHED_IDLE` do kernel: a classe de quem so roda quando ninguem mais quer a
/// CPU.
#[cfg(not(windows))]
const SCHED_IDLE: i32 = 5;

/// Politica de agendamento do processo atual, ou `None` se o `/proc` nao
/// estiver do jeito esperado.
#[cfg(not(windows))]
fn politica_de_agendamento() -> Option<i32> {
    politica_do_stat(&fs::read_to_string("/proc/self/stat").ok()?)
}

/// A politica e o campo 41 do `/proc/<pid>/stat`. A contagem comeca depois do
/// ultimo `)` porque o campo 2 e o nome do executavel — que pode ter espacos e
/// parenteses dentro e estragaria um `split` ingenuo da linha inteira.
#[cfg(not(windows))]
fn politica_do_stat(stat: &str) -> Option<i32> {
    let depois_do_nome = &stat[stat.rfind(')')? + 1..];
    // O primeiro campo depois do nome e o 3 (estado), entao o 41 e o indice 38.
    depois_do_nome.split_whitespace().nth(38)?.parse().ok()
}

/// Inicia o karaoke por um servico transitorio do systemd do usuario.
///
/// `None` quando a maquina nao tem `systemd-run` — ai o jeito e tentar do
/// modo normal, que ainda pode funcionar dependendo do `RLIMIT_NICE`.
#[cfg(not(windows))]
fn launch_pelo_systemd(p: &Paths) -> Option<Result<(), String>> {
    let mut cmd = Command::new("systemd-run");
    cmd.arg("--user")
        .arg("--quiet")
        // Sem isto o unit fica para tras depois que o karaoke fecha, e a
        // proxima chamada esbarra num nome ja usado.
        .arg("--collect")
        .args(["-p", &format!("WorkingDirectory={}", p.app_dir.display())])
        .args(["-p", "Nice=0"])
        .args(["-p", "CPUSchedulingPolicy=other"])
        .arg(format!("--setenv=LD_LIBRARY_PATH={}", ld_library_path(p)));

    // O gerenciador de sessao costuma conhecer essas variaveis, mas nem todo
    // desktop as exporta para ele; repassamos as nossas quando existem.
    for chave in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XAUTHORITY",
        "XDG_RUNTIME_DIR",
        "XDG_SESSION_TYPE",
    ] {
        if let Ok(valor) = std::env::var(chave) {
            cmd.arg(format!("--setenv={chave}={valor}"));
        }
    }

    cmd.arg("--").arg(&p.exe).arg("-ConfigFile").arg(&p.config);

    // `output()` em vez de `spawn()`: o systemd-run so entrega o servico e sai,
    // e assim colhemos tanto o codigo de saida quanto a mensagem de erro dele.
    let saida = cmd.output().ok()?;
    Some(if saida.status.success() {
        Ok(())
    } else {
        Err(format!(
            "nao foi possivel abrir o karaoke: {}",
            String::from_utf8_lossy(&saida.stderr).trim()
        ))
    })
}

#[tauri::command]
fn launch() -> Result<(), String> {
    let p = paths::resolve()?;
    if !p.exe.is_file() {
        return Err("o karaoke ainda nao esta instalado.".to_string());
    }

    // O UltraStar e Free Pascal, e a cthreads cria cada TThread com
    // PTHREAD_EXPLICIT_SCHED — ou seja, pedindo SCHED_OTHER na marra. Se este
    // processo estiver em SCHED_IDLE, o kernel nega a troca (EPERM, porque o
    // RLIMIT_NICE padrao e 0) e o karaoke morre com
    // `EThread: Failed to create new thread` na hora de montar a lista de
    // musicas. O filho herda a politica, e sair do SCHED_IDLE exige
    // CAP_SYS_NICE — entao passamos a bola para o systemd do usuario, que roda
    // em SCHED_OTHER e inicia o karaoke limpo.
    //
    // Nao e caso de laboratorio: o ananicy-cpp, ligado por padrao no CachyOS,
    // poe tudo que descende do `node` em SCHED_IDLE (regra BG_CPUIO) — o que
    // inclui este painel quando ele sobe por `npm run dev`.
    #[cfg(not(windows))]
    if politica_de_agendamento() == Some(SCHED_IDLE) {
        if let Some(resultado) = launch_pelo_systemd(&p) {
            return resultado;
        }
    }

    let mut cmd = Command::new(&p.exe);
    cmd.arg("-ConfigFile").arg(&p.config);
    cmd.current_dir(&p.app_dir);

    #[cfg(not(windows))]
    cmd.env("LD_LIBRARY_PATH", ld_library_path(&p));
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let mut filho = cmd
        .spawn()
        .map_err(|e| format!("nao foi possivel abrir o karaoke: {e}"))?;

    // O painel fica aberto a festa inteira; sem colher o filho, cada rodada de
    // karaoke deixa um zumbi na tabela de processos.
    std::thread::spawn(move || {
        let _ = filho.wait();
    });
    Ok(())
}

#[tauri::command]
fn open_songs_folder() -> Result<(), String> {
    let p = paths::resolve()?;
    fs::create_dir_all(&p.songs)
        .map_err(|e| format!("erro ao criar a pasta de musicas: {e}"))?;

    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("explorer");
        c.creation_flags(CREATE_NO_WINDOW);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = Command::new("xdg-open");

    cmd.arg(&p.songs)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("nao foi possivel abrir a pasta: {e}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(jukebox::Estado::default())
        .invoke_handler(tauri::generate_handler![
            platform_info,
            status,
            install,
            uninstall,
            launch,
            open_songs_folder,
            jukebox_status,
            jukebox_install,
            jukebox_configure,
            jukebox_start,
            jukebox_stop
        ])
        .build(tauri::generate_context!())
        .expect("erro ao iniciar o app tauri")
        .run(|app, evento| {
            // O jukebox e um processo separado: sem isto ele ficaria no ar
            // depois que o instalador fecha, segurando a porta.
            if matches!(evento, tauri::RunEvent::Exit) {
                jukebox::encerrar(&app.state());
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_esta_coerente() {
        assert!(USDX_URL.contains(USDX_VERSION), "a URL deve apontar para a versao declarada");
        assert_eq!(USDX_SHA256.len(), 64, "SHA-256 tem 64 caracteres hex");
        assert!(USDX_SHA256.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(USDX_URL.starts_with("https://"), "download so por HTTPS");
    }

    #[test]
    fn assets_da_marca_foram_embutidos() {
        // JPEG comeca com FF D8 FF, PNG com \x89PNG.
        assert_eq!(&BG_MAIN[..3], &[0xFF, 0xD8, 0xFF]);
        assert_eq!(&BG_LOAD[..3], &[0xFF, 0xD8, 0xFF]);
        assert_eq!(&ICON_MAIN[..4], b"\x89PNG");
        assert_eq!(&MARK[..4], b"\x89PNG");

        // O icone do atalho muda de formato conforme o sistema.
        #[cfg(not(windows))]
        assert_eq!(&ICON_256[..4], b"\x89PNG");
        #[cfg(windows)]
        assert_eq!(&ICON_ICO[..4], &[0x00, 0x00, 0x01, 0x00]);
    }

    #[cfg(not(windows))]
    #[test]
    fn le_a_politica_de_agendamento_do_stat() {
        // Nome de executavel com espaco e parenteses dentro: o `)` do nome nao
        // pode confundir a contagem dos campos.
        // Campo 3 (estado), os campos 4 a 40, e ai o 41 — a politica.
        let miolo = ["0"; 37].join(" ");
        let stat = format!("1234 (ultra star (x)) S {miolo} 5 0 0");
        assert_eq!(politica_do_stat(&stat), Some(SCHED_IDLE));

        // O nosso proprio processo tem que ter uma politica legivel.
        assert!(politica_de_agendamento().is_some());
    }

    #[cfg(not(windows))]
    #[test]
    fn as_libs_do_appimage_vem_na_frente() {
        let p = paths::resolve().unwrap();
        let esperado = p.app_dir.join("usr").join("lib");
        assert!(
            ld_library_path(&p).starts_with(&esperado.to_string_lossy().to_string()),
            "as libs da instalacao tem que ganhar das do sistema"
        );
    }

    #[test]
    fn marker_fica_dentro_da_instalacao() {
        let p = paths::resolve().unwrap();
        assert!(marker(&p).starts_with(&p.app_dir));
    }

    #[test]
    fn o_atalho_aponta_para_um_arquivo_que_existe() {
        // O atalho abre o painel, entao o caminho tem que sobreviver ao
        // fechamento do app — nada de apontar para um /tmp de AppImage.
        let painel = caminho_do_painel().expect("deve descobrir o proprio caminho");
        assert!(painel.is_absolute(), "{} nao e absoluto", painel.display());
        assert!(painel.is_file(), "{} nao existe", painel.display());
    }

    #[cfg(not(windows))]
    #[test]
    fn dentro_de_um_appimage_o_atalho_usa_o_arquivo_e_nao_a_montagem() {
        // Este teste documenta a armadilha: sem consultar o APPIMAGE, o
        // caminho seria o do squashfs em /tmp, que some ao fechar o app.
        let real = std::env::current_exe().unwrap();
        // SAFETY: teste de processo unico; a variavel volta ao fim.
        unsafe { std::env::set_var("APPIMAGE", &real) };
        let escolhido = caminho_do_painel().unwrap();
        unsafe { std::env::remove_var("APPIMAGE") };
        assert_eq!(escolhido, real);
    }

    #[test]
    fn contagem_de_musicas_e_zero_em_pasta_inexistente() {
        assert_eq!(count_songs(Path::new("/caminho/que/nao/existe/xyz")), 0);
    }
}
