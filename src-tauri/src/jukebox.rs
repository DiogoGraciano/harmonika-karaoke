//! Jukebox de festa: o USDB Syncer rodando em modo `serve`.
//!
//! O sincronizador oficial (<https://github.com/bohning/usdb_syncer>) sabe
//! subir um servidor web com o acervo do usdb.eu. Este modulo baixa o release
//! oficial dele, provisiona o ffmpeg que ele precisa e cuida do processo,
//! para os convidados abrirem o acervo no celular apontando a camera para um
//! QR code na tela.
//!
//! Nada e redistribuido junto com o instalador: o sincronizador e GPL-3.0 e
//! vem direto do release oficial, conferido pelo SHA-256, igual ao UltraStar.

use crate::paths::{self, Paths};
use crate::{baixar, Reporter};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

// ---- Release do USDB Syncer que o instalador usa ----
// Mesmo criterio do UltraStar: URL do release oficial + SHA-256 conferido
// antes de instalar. Para subir de versao, troque os dois valores do bloco
// do sistema correspondente (e a constante de versao).

pub const SYNCER_VERSION: &str = "0.24.0";

#[cfg(not(windows))]
const SYNCER_URL: &str = "https://github.com/bohning/usdb_syncer/releases/download/v0.24.0/USDB_Syncer-0.24.0-Linux";
#[cfg(not(windows))]
const SYNCER_SHA256: &str = "e1ead1debb17944b01354aea37785ce0615c48f7e02c4985da0873fd7439509d";

#[cfg(windows)]
const SYNCER_URL: &str = "https://github.com/bohning/usdb_syncer/releases/download/v0.24.0/USDB_Syncer-0.24.0-Windows-portable.exe";
#[cfg(windows)]
const SYNCER_SHA256: &str = "ad0cadee8966c135c1e08674702abf5d5fc96d10f934a3bd9f3d7f33def986bf";

// ---- ffmpeg ----
// O sincronizador nao embute ffmpeg: ele chama `ffmpeg` e `ffprobe` pelo PATH
// para converter o audio que o yt-dlp baixa. Sem eles todo download falha na
// ultima etapa. Se a maquina ja tem os dois, nao baixamos nada.
//
// A tag `autobuild-*` do BtbN e imutavel — as tags `latest` sao reescritas a
// cada build, e o SHA-256 fixado aqui deixaria de bater no dia seguinte.

// Repetidos fora da URL porque e o teste que garante que ela continua
// apontando para a tag fixa. O `FFMPEG_BUILD` tambem nomeia a pasta que sai
// do tar no Linux; no Windows ele so vive nos testes.
#[allow(dead_code)]
const FFMPEG_TAG: &str = "autobuild-2026-08-05-15-18";
#[allow(dead_code)]
const FFMPEG_BUILD: &str = "N-125972-ge13b2e00e8";

#[cfg(not(windows))]
const FFMPEG_URL: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-05-15-18/ffmpeg-N-125972-ge13b2e00e8-linux64-gpl.tar.xz";
#[cfg(not(windows))]
const FFMPEG_SHA256: &str = "edc1946e62ae646f46a4715d39ebc610f3db0a643ceb60ed16b9e3393701db47";
#[cfg(not(windows))]
const FFMPEG_FILE: &str = "ffmpeg.tar.xz";

#[cfg(windows)]
const FFMPEG_URL: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-05-15-18/ffmpeg-N-125972-ge13b2e00e8-win64-gpl.zip";
#[cfg(windows)]
const FFMPEG_SHA256: &str = "c69c2fe832c6f0f0a5b79394a854752ae7825a916a13f61d7127ac0542a732a9";
#[cfg(windows)]
const FFMPEG_FILE: &str = "ffmpeg.zip";

/// Titulo que aparece no topo da pagina que os convidados abrem.
const TITULO: &str = "Harmonika&Co Karaokê";

/// Porta preferida do jukebox. Se estiver ocupada, o sistema escolhe outra.
const PORTA_PREFERIDA: u16 = 9770;

/// Quanto esperamos o servidor atender antes de desistir. O binario e um
/// bundle PyInstaller: a primeira execucao se descompacta antes de subir.
const ESPERA_SUBIR: Duration = Duration::from_secs(45);

// ---- Tipos trocados com a interface ----

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JukeboxStatus {
    pub installed: bool,
    /// Versao do sincronizador instalada (vazio se nao instalado).
    pub version: String,
    /// Este release deixa o convidado pedir download pelo celular.
    pub requests: bool,
    /// Vale reinstalar: versao antiga ou instalacao sem ffmpeg.
    pub outdated: bool,
    pub running: bool,
    /// Endereco do jukebox enquanto ele esta no ar.
    pub address: Option<String>,
    /// QR code do endereco, em SVG, pronto para ir na tela.
    pub qr: Option<String>,
    /// De onde vem o ffmpeg: `sistema`, `jukebox` ou `faltando`.
    pub ffmpeg: String,
    pub detail: String,
}

/// O que gravamos ao instalar. Guardamos o que o binario sabe fazer junto da
/// versao porque descobrir isso custa uns 2s de execucao do bundle.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Marca {
    version: String,
    requests: bool,
}

struct Servidor {
    processo: Child,
    endereco: String,
}

/// Processo do jukebox, guardado pelo Tauri entre uma chamada e outra.
#[derive(Default)]
pub struct Estado(Mutex<Option<Servidor>>);

// ---- Descoberta do ambiente ----

fn nome_bin(nome: &str) -> String {
    if cfg!(windows) {
        format!("{nome}.exe")
    } else {
        nome.to_string()
    }
}

/// Procura um executavel no PATH, do mesmo jeito que o `shutil.which` que o
/// sincronizador usa para achar o ffmpeg.
fn no_path(nome: &str) -> bool {
    let alvo = nome_bin(nome);
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| dir.join(&alvo).is_file())
    })
}

fn ffmpeg_proprio(p: &Paths) -> bool {
    p.ffmpeg_dir.join(nome_bin("ffmpeg")).is_file()
        && p.ffmpeg_dir.join(nome_bin("ffprobe")).is_file()
}

fn ffmpeg_estado(p: &Paths) -> &'static str {
    if ffmpeg_proprio(p) {
        "jukebox"
    } else if no_path("ffmpeg") && no_path("ffprobe") {
        "sistema"
    } else {
        "faltando"
    }
}

/// Descobre o IP desta maquina na rede local.
///
/// E o mesmo truque do proprio sincronizador (`webserver.get_local_ip`): um
/// socket UDP "conectado" nao envia pacote nenhum, so faz o sistema escolher
/// a interface de saida. Tem que ser identico ao dele — o `--host` do release
/// 0.24.0 esta quebrado (so aceita inteiro), entao quem escolhe o endereco de
/// bind e o proprio servidor, e o QR precisa apontar para o mesmo lugar.
fn ip_local() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("10.255.255.255:1")?;
            s.local_addr()
        })
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

fn porta_livre(ip: &str) -> Result<u16, String> {
    if TcpListener::bind((ip, PORTA_PREFERIDA)).is_ok() {
        return Ok(PORTA_PREFERIDA);
    }
    let ouvinte = TcpListener::bind((ip, 0))
        .map_err(|e| format!("nao ha porta livre em {ip} para o jukebox: {e}"))?;
    ouvinte
        .local_addr()
        .map(|a| a.port())
        .map_err(|e| format!("nao foi possivel escolher a porta do jukebox: {e}"))
}

fn marca_path(p: &Paths) -> PathBuf {
    p.jukebox_dir.join(".harmonika-jukebox")
}

fn ler_marca(p: &Paths) -> Marca {
    fs::read_to_string(marca_path(p))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Acrescenta o nosso ffmpeg ao PATH do processo filho.
///
/// Preferimos isso a escrever na configuracao do sincronizador (QSettings, que
/// no Windows e o registro): o efeito e o mesmo, vale so para o processo que
/// nos criamos e nao deixa rastro na maquina.
fn com_ffmpeg(p: &Paths, cmd: &mut Command) {
    if !ffmpeg_proprio(p) {
        return;
    }
    let atual = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs = vec![p.ffmpeg_dir.clone()];
    dirs.extend(std::env::split_paths(&atual));
    if let Ok(novo) = std::env::join_paths(dirs) {
        cmd.env("PATH", novo);
    }
}

fn sem_janela(cmd: &mut Command) {
    #[cfg(windows)]
    cmd.creation_flags(crate::CREATE_NO_WINDOW);
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Pergunta ao binario se ele aceita pedidos de download pelo celular.
///
/// O release 0.24.0 ainda nao tem `--allow-downloading` — nele o jukebox e
/// vitrine e votacao. Perguntar ao proprio binario, em vez de comparar numero
/// de versao, faz o recurso se acender sozinho quando o release sair.
fn aceita_pedidos(syncer: &Path) -> bool {
    let mut cmd = Command::new(syncer);
    cmd.args(["serve", "--help"]);
    sem_janela(&mut cmd);
    cmd.output()
        .map(|out| {
            let texto = String::from_utf8_lossy(&out.stdout);
            texto.contains("--allow-downloading")
        })
        .unwrap_or(false)
}

// ---- Instalacao ----

#[cfg(not(windows))]
fn extrair_ffmpeg(arquivo: &Path, tmp: &Path, destino: &Path) -> Result<(), String> {
    // Tirar so os dois binarios do tar evita descompactar ~350 MB (o pacote
    // ainda traz ffplay, documentacao e presets, que nao usamos).
    let out = Command::new("tar")
        .arg("-xJf")
        .arg(arquivo)
        .arg("-C")
        .arg(tmp)
        .arg("--wildcards")
        .arg("*/bin/ffmpeg")
        .arg("*/bin/ffprobe")
        .output()
        .map_err(|e| format!("falha ao extrair o ffmpeg (o `tar` esta instalado?): {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "falha ao extrair o ffmpeg: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    let bin = tmp
        .join(format!("ffmpeg-{FFMPEG_BUILD}-linux64-gpl"))
        .join("bin");
    fs::create_dir_all(destino)
        .map_err(|e| format!("erro ao criar {}: {e}", destino.display()))?;
    for nome in ["ffmpeg", "ffprobe"] {
        let origem = bin.join(nome);
        let alvo = destino.join(nome);
        fs::rename(&origem, &alvo)
            .map_err(|e| format!("o pacote do ffmpeg nao trouxe {nome}: {e}"))?;
        crate::make_executable(&alvo)?;
    }
    Ok(())
}

#[cfg(windows)]
fn extrair_ffmpeg(arquivo: &Path, _tmp: &Path, destino: &Path) -> Result<(), String> {
    use std::io::copy;

    let zip = fs::File::open(arquivo)
        .map_err(|e| format!("erro ao abrir o pacote do ffmpeg: {e}"))?;
    let mut zip = zip::ZipArchive::new(zip)
        .map_err(|e| format!("o pacote do ffmpeg nao e um zip valido: {e}"))?;
    fs::create_dir_all(destino)
        .map_err(|e| format!("erro ao criar {}: {e}", destino.display()))?;

    // So os dois que interessam: o zip tambem traz ffplay, de ~145 MB.
    let mut achados = 0;
    for i in 0..zip.len() {
        let mut entrada = zip
            .by_index(i)
            .map_err(|e| format!("erro ao ler o pacote do ffmpeg: {e}"))?;
        let nome = entrada.name().replace('\\', "/");
        let alvo = match nome.rsplit('/').next() {
            Some(n @ ("ffmpeg.exe" | "ffprobe.exe")) if nome.contains("/bin/") => {
                destino.join(n)
            }
            _ => continue,
        };
        let mut saida = fs::File::create(&alvo)
            .map_err(|e| format!("erro ao gravar {}: {e}", alvo.display()))?;
        copy(&mut entrada, &mut saida)
            .map_err(|e| format!("erro ao extrair {}: {e}", alvo.display()))?;
        achados += 1;
    }
    if achados != 2 {
        return Err("o pacote do ffmpeg nao trouxe ffmpeg.exe e ffprobe.exe".to_string());
    }
    Ok(())
}

pub async fn instalar_com(rep: Reporter) -> Result<JukeboxStatus, String> {
    let p = paths::resolve()?;
    let tmp = p.jukebox_dir.join("tmp");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp)
        .map_err(|e| format!("erro ao criar {}: {e}", tmp.display()))?;

    // Nao rebaixamos 120 MB de ffmpeg so porque o sincronizador subiu de
    // versao: se ja temos o nosso, ou se a maquina tem o dela, esta resolvido.
    let precisa_ffmpeg = !ffmpeg_proprio(&p) && (!no_path("ffmpeg") || !no_path("ffprobe"));
    // Sem ffmpeg para baixar, o download do sincronizador ocupa a barra toda.
    let faixa_syncer = if precisa_ffmpeg { (0, 55) } else { (0, 90) };

    let resultado = async {
        rep(0, "download", "Conectando ao servidor do sincronizador…");
        let bruto = tmp.join("syncer");
        baixar(
            &rep,
            SYNCER_URL,
            SYNCER_SHA256,
            &bruto,
            "o sincronizador",
            faixa_syncer,
        )
        .await?;

        let p2 = p.clone();
        let rep2 = std::sync::Arc::clone(&rep);
        tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
            rep2(faixa_syncer.1 + 1, "instalar", "Verificado. Instalando…");
            let _ = fs::remove_file(&p2.syncer);
            fs::rename(&bruto, &p2.syncer)
                .map_err(|e| format!("erro ao instalar o sincronizador: {e}"))?;
            #[cfg(unix)]
            crate::make_executable(&p2.syncer)?;
            Ok(())
        })
        .await
        .map_err(|e| format!("erro interno ao instalar o sincronizador: {e}"))??;

        if precisa_ffmpeg {
            let pacote = tmp.join(FFMPEG_FILE);
            baixar(&rep, FFMPEG_URL, FFMPEG_SHA256, &pacote, "o ffmpeg", (57, 90)).await?;

            let p2 = p.clone();
            let tmp2 = tmp.clone();
            let rep2 = std::sync::Arc::clone(&rep);
            tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
                rep2(91, "extracao", "Extraindo o ffmpeg…");
                let _ = fs::remove_dir_all(&p2.ffmpeg_dir);
                extrair_ffmpeg(&pacote, &tmp2, &p2.ffmpeg_dir)
            })
            .await
            .map_err(|e| format!("erro interno ao extrair o ffmpeg: {e}"))??;
        }

        let p2 = p.clone();
        let rep2 = std::sync::Arc::clone(&rep);
        tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
            rep2(96, "conferir", "Conferindo o que este release sabe fazer…");
            let marca = Marca {
                version: SYNCER_VERSION.to_string(),
                requests: aceita_pedidos(&p2.syncer),
            };
            let json = serde_json::to_string(&marca)
                .map_err(|e| format!("erro ao gravar a marca do jukebox: {e}"))?;
            crate::write_file(&marca_path(&p2), json.as_bytes())
        })
        .await
        .map_err(|e| format!("erro interno ao conferir o sincronizador: {e}"))??;

        Ok::<(), String>(())
    }
    .await;

    let _ = fs::remove_dir_all(&tmp);
    resultado?;

    rep(100, "pronto", "Jukebox instalado.");
    status_de(&p, None)
}

// ---- Uso ----

/// Abre a janela do sincronizador para a pessoa entrar na conta do usdb.eu e
/// puxar a lista de musicas.
///
/// E um passo obrigatorio antes do jukebox: o servidor le a lista do banco
/// local, e quem preenche esse banco e a janela, no primeiro login.
pub fn configurar() -> Result<(), String> {
    let p = paths::resolve()?;
    if !p.syncer.is_file() {
        return Err("o sincronizador ainda nao esta instalado.".to_string());
    }
    fs::create_dir_all(&p.songs)
        .map_err(|e| format!("erro ao criar a pasta de musicas: {e}"))?;

    let mut cmd = Command::new(&p.syncer);
    cmd.arg("--songpath").arg(&p.songs);
    cmd.current_dir(&p.jukebox_dir);
    com_ffmpeg(&p, &mut cmd);
    sem_janela(&mut cmd);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("nao foi possivel abrir o sincronizador: {e}"))
}

/// Ultimas linhas do registro, para o erro dizer alguma coisa util.
fn cauda_do_log(log: &Path) -> String {
    let Ok(texto) = fs::read_to_string(log) else {
        return String::new();
    };
    let linhas: Vec<&str> = texto.lines().collect();
    linhas[linhas.len().saturating_sub(5)..].join("\n")
}

/// Espera o servidor atender, ou explica por que ele nao subiu.
///
/// O processo ja esta guardado no estado quando isto roda, e por isso a espera
/// so toca no mutex com `try_lock`: fechar o instalador no meio da subida
/// precisa conseguir derrubar o processo na hora, sem esperar estes segundos.
fn esperar_subir(estado: &Estado, ip: &str, porta: u16, log: &Path) -> Result<(), String> {
    let limite = Instant::now() + ESPERA_SUBIR;
    while Instant::now() < limite {
        if TcpStream::connect((ip, porta)).is_ok() {
            return Ok(());
        }
        if let Ok(mut guarda) = estado.0.try_lock() {
            match guarda.as_mut() {
                // Alguem derrubou o jukebox enquanto ele subia.
                None => return Err("o jukebox foi interrompido.".to_string()),
                Some(servidor) => {
                    if let Ok(Some(saida)) = servidor.processo.try_wait() {
                        return Err(format!(
                            "o jukebox encerrou sozinho ({saida}).\n{}",
                            cauda_do_log(log).trim()
                        ));
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(format!(
        "o jukebox não respondeu em {}s. O registro está em {}.",
        ESPERA_SUBIR.as_secs(),
        log.display()
    ))
}

pub fn iniciar(estado: &Estado) -> Result<JukeboxStatus, String> {
    let p = paths::resolve()?;
    if !p.syncer.is_file() {
        return Err("o sincronizador ainda nao esta instalado.".to_string());
    }

    {
        let guarda = estado
            .0
            .lock()
            .map_err(|_| "o estado do jukebox ficou inconsistente.".to_string())?;
        if let Some(servidor) = guarda.as_ref() {
            return status_de(&p, Some(servidor.endereco.clone()));
        }
    }

    fs::create_dir_all(&p.songs)
        .map_err(|e| format!("erro ao criar a pasta de musicas: {e}"))?;

    let ip = ip_local();
    let porta = porta_livre(&ip)?;
    let marca = ler_marca(&p);
    let log = p.jukebox_dir.join("jukebox.log");
    let saida = fs::File::create(&log)
        .map_err(|e| format!("erro ao criar o registro do jukebox: {e}"))?;
    let erros = saida
        .try_clone()
        .map_err(|e| format!("erro ao preparar o registro do jukebox: {e}"))?;

    let mut cmd = Command::new(&p.syncer);
    // `--songpath` e opcao do parser principal, entao vem antes do subcomando.
    // Sem ela o sincronizador baixaria para a pasta dele, e nao para a que o
    // UltraStar le.
    cmd.arg("--songpath").arg(&p.songs);
    cmd.arg("serve")
        .arg("--port")
        .arg(porta.to_string())
        .arg("--title")
        .arg(TITULO)
        // Sem isto a pagina so mostra o que ja esta baixado — e numa maquina
        // nova isso e uma lista vazia.
        .arg("--show-nonlocal");
    if marca.requests {
        cmd.arg("--allow-downloading");
    }
    cmd.current_dir(&p.jukebox_dir);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::from(saida))
        .stderr(Stdio::from(erros));
    com_ffmpeg(&p, &mut cmd);
    sem_janela(&mut cmd);

    let processo = cmd
        .spawn()
        .map_err(|e| format!("nao foi possivel iniciar o jukebox: {e}"))?;

    let endereco = format!("http://{ip}:{porta}");
    // Guardar antes de esperar: enquanto o servidor sobe, o processo ja tem
    // que estar ao alcance de quem for derruba-lo.
    estado
        .0
        .lock()
        .map_err(|_| "o estado do jukebox ficou inconsistente.".to_string())?
        .replace(Servidor {
            processo,
            endereco: endereco.clone(),
        });

    if let Err(e) = esperar_subir(estado, &ip, porta, &log) {
        encerrar(estado);
        return Err(e);
    }

    status_de(&p, Some(endereco))
}

pub fn parar(estado: &Estado) -> Result<JukeboxStatus, String> {
    encerrar(estado);
    status(estado)
}

/// Derruba o servidor se ele estiver no ar. Usado ao parar pela interface e
/// ao fechar o instalador, para nao deixar processo orfao segurando a porta.
pub fn encerrar(estado: &Estado) {
    let Ok(mut guarda) = estado.0.lock() else {
        return;
    };
    if let Some(mut servidor) = guarda.take() {
        let _ = servidor.processo.kill();
        let _ = servidor.processo.wait();
    }
}

// ---- Estado para a interface ----

fn qr_svg(url: &str) -> Option<String> {
    use qrcode::render::svg;
    use qrcode::QrCode;

    let svg = QrCode::new(url).ok().map(|code| {
        code.render()
            .min_dimensions(220, 220)
            .quiet_zone(true)
            .dark_color(svg::Color("#0b0a0f"))
            .light_color(svg::Color("#f2f3f7"))
            .build()
    })?;
    // O renderizador prefixa uma declaracao XML, que o navegador descarta ao
    // inserir o SVG na pagina. Cortamos aqui para entregar so a arvore.
    let inicio = svg.find("<svg")?;
    Some(svg[inicio..].to_string())
}

fn status_de(p: &Paths, endereco: Option<String>) -> Result<JukeboxStatus, String> {
    let installed = p.syncer.is_file();
    let marca = ler_marca(p);
    let ffmpeg = ffmpeg_estado(p);
    let running = endereco.is_some();
    let outdated = installed && (ffmpeg == "faltando" || marca.version != SYNCER_VERSION);

    let detail = if !installed {
        "O jukebox ainda não está nesta máquina.".to_string()
    } else if running {
        if marca.requests {
            "No ar. Os convidados podem pedir músicas pelo celular.".to_string()
        } else {
            "No ar. Os convidados podem procurar e votar nas músicas.".to_string()
        }
    } else if ffmpeg == "faltando" {
        "Instalado, mas sem ffmpeg — os downloads vão falhar. Reinstale o jukebox."
            .to_string()
    } else if marca.version != SYNCER_VERSION {
        format!(
            "Instalado na versão {} — este instalador traz a {SYNCER_VERSION}.",
            marca.version
        )
    } else {
        format!("Pronto (USDB Syncer {SYNCER_VERSION}).")
    };

    Ok(JukeboxStatus {
        installed,
        version: marca.version,
        requests: marca.requests,
        outdated,
        running,
        qr: endereco.as_deref().and_then(qr_svg),
        address: endereco,
        ffmpeg: ffmpeg.to_string(),
        detail,
    })
}

pub fn status(estado: &Estado) -> Result<JukeboxStatus, String> {
    let p = paths::resolve()?;
    let endereco = estado
        .0
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|s| s.endereco.clone()));
    status_de(&p, endereco)
}

/// Remove o jukebox da maquina. Chamado junto da desinstalacao do karaoke.
pub fn remover(estado: &Estado) -> Result<(), String> {
    encerrar(estado);
    let p = paths::resolve()?;
    if p.jukebox_dir.exists() {
        fs::remove_dir_all(&p.jukebox_dir)
            .map_err(|e| format!("erro ao remover o jukebox: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_do_syncer_esta_coerente() {
        assert!(
            SYNCER_URL.contains(SYNCER_VERSION),
            "a URL deve apontar para a versao declarada"
        );
        assert!(SYNCER_URL.starts_with("https://"), "download so por HTTPS");
        assert_eq!(SYNCER_SHA256.len(), 64, "SHA-256 tem 64 caracteres hex");
        assert!(SYNCER_SHA256.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn ffmpeg_vem_de_uma_tag_imutavel() {
        // As tags `latest` do BtbN sao reescritas a cada build; se a URL virar
        // uma delas, o SHA-256 fixado aqui para de bater sem aviso.
        assert!(FFMPEG_URL.contains(FFMPEG_TAG), "use a tag autobuild fixa");
        assert!(!FFMPEG_URL.contains("latest"));
        assert!(FFMPEG_URL.contains(FFMPEG_BUILD));
        assert!(FFMPEG_URL.starts_with("https://"));
        assert_eq!(FFMPEG_SHA256.len(), 64);
        assert!(FFMPEG_SHA256.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn o_jukebox_nao_e_apagado_ao_reinstalar_o_karaoke() {
        let p = paths::resolve().unwrap();
        assert!(!marca_path(&p).starts_with(&p.app_dir));
        assert!(marca_path(&p).starts_with(&p.jukebox_dir));
    }

    #[test]
    fn ip_local_e_um_endereco_valido() {
        let ip = ip_local();
        assert!(
            ip.parse::<std::net::IpAddr>().is_ok(),
            "ip_local devolveu {ip:?}"
        );
    }

    #[test]
    fn o_qr_carrega_o_endereco() {
        let svg = qr_svg("http://192.168.0.10:9770").expect("deve gerar o QR");
        // Precisa comecar no proprio elemento: a interface joga isto direto
        // na pagina, e o navegador descartaria uma declaracao XML na frente.
        assert!(svg.starts_with("<svg"), "veio {:?}", &svg[..20.min(svg.len())]);
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn marca_ausente_vira_padrao_seguro() {
        // Sem o arquivo, nao assumimos que o release aceita pedidos.
        let marca = Marca::default();
        assert!(!marca.requests);
        assert!(marca.version.is_empty());
    }

    #[test]
    fn nome_do_binario_segue_o_sistema() {
        if cfg!(windows) {
            assert_eq!(nome_bin("ffmpeg"), "ffmpeg.exe");
        } else {
            assert_eq!(nome_bin("ffmpeg"), "ffmpeg");
        }
    }
}
