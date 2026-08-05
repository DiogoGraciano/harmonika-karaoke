//! Onde cada coisa fica em cada sistema.
//!
//! Linux e Windows tem layouts bem diferentes: no Linux o AppImage extraido
//! tem `usr/bin/` + `usr/share/ultrastardx/`, enquanto o zip portable do
//! Windows joga o `ultrastardx.exe` e a pasta `themes/` na mesma raiz.

use serde::Serialize;
use std::path::PathBuf;

/// Nome da pasta de instalacao (igual nos dois sistemas).
pub const APP_FOLDER: &str = "HarmonikaKaraoke";

/// Pasta do jukebox. Fica ao lado da instalacao, e nao dentro dela, porque
/// reinstalar o karaoke apaga `app_dir` inteiro — e o sincronizador custa
/// 170 MB de download que ninguem quer refazer a toa.
pub const JUKEBOX_FOLDER: &str = "HarmonikaKaraoke-jukebox";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Paths {
    /// Raiz da instalacao do UltraStar ja com a marca.
    pub app_dir: PathBuf,
    /// Binario que sera executado.
    pub exe: PathBuf,
    /// Pasta de temas dentro da instalacao (unico lugar de onde o UltraStar le temas).
    pub themes_dir: PathBuf,
    /// Pasta de dados do usuario (config, capas, avatares).
    pub user_dir: PathBuf,
    /// Config isolado do karaoke da empresa, passado via `-ConfigFile`.
    pub config: PathBuf,
    /// Onde o usuario coloca as musicas.
    pub songs: PathBuf,
    /// Raiz do jukebox (sincronizador + ffmpeg).
    pub jukebox_dir: PathBuf,
    /// Binario do USDB Syncer.
    pub syncer: PathBuf,
    /// Pasta com o ffmpeg/ffprobe provisionados por nos. Fica vazia quando a
    /// maquina ja tem os dois no PATH.
    pub ffmpeg_dir: PathBuf,
}

/// Raiz por usuario onde tudo e instalado (nunca precisa de admin).
fn base_dir() -> Result<PathBuf, String> {
    dirs::data_local_dir()
        .ok_or_else(|| "nao foi possivel descobrir a pasta de dados do usuario".to_string())
}

/// Pasta de dados do UltraStar, seguindo a convencao dele em cada sistema:
/// `~/.ultrastardx` no Linux e `%APPDATA%\ultrastardx` no Windows.
fn user_data_dir() -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        dirs::config_dir()
            .map(|d| d.join("ultrastardx"))
            .ok_or_else(|| "nao foi possivel descobrir %APPDATA%".to_string())
    }
    #[cfg(not(windows))]
    {
        dirs::home_dir()
            .map(|d| d.join(".ultrastardx"))
            .ok_or_else(|| "nao foi possivel descobrir a pasta do usuario".to_string())
    }
}

pub fn resolve() -> Result<Paths, String> {
    let base = base_dir()?;
    let app_dir = base.join(APP_FOLDER);
    let jukebox_dir = base.join(JUKEBOX_FOLDER);
    let user_dir = user_data_dir()?;

    // O layout muda conforme o artefato oficial de cada sistema.
    #[cfg(windows)]
    let (exe, themes_dir, syncer_file) = (
        app_dir.join("ultrastardx.exe"),
        app_dir.join("themes"),
        "usdb-syncer.exe",
    );
    #[cfg(not(windows))]
    let (exe, themes_dir, syncer_file) = (
        app_dir.join("usr").join("bin").join("ultrastardx"),
        app_dir.join("usr").join("share").join("ultrastardx").join("themes"),
        "usdb-syncer",
    );

    Ok(Paths {
        exe,
        themes_dir,
        config: user_dir.join("harmonika-config.ini"),
        songs: user_dir.join("songs"),
        syncer: jukebox_dir.join(syncer_file),
        ffmpeg_dir: jukebox_dir.join("ffmpeg"),
        user_dir,
        app_dir,
        jukebox_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_bate_com_o_artefato_do_sistema() {
        let p = resolve().expect("deve resolver os caminhos");

        // O binario e a pasta de temas moram sempre dentro da instalacao.
        assert!(p.exe.starts_with(&p.app_dir));
        assert!(p.themes_dir.starts_with(&p.app_dir));

        // Config e musicas moram na pasta de dados do usuario.
        assert!(p.config.starts_with(&p.user_dir));
        assert!(p.songs.starts_with(&p.user_dir));
        assert_eq!(p.config.file_name().unwrap(), "harmonika-config.ini");

        assert_eq!(p.app_dir.file_name().unwrap(), APP_FOLDER);

        // O jukebox mora ao lado da instalacao, nunca dentro: reinstalar o
        // karaoke apaga `app_dir`, e o sincronizador tem que sobreviver.
        assert!(!p.jukebox_dir.starts_with(&p.app_dir));
        assert!(!p.app_dir.starts_with(&p.jukebox_dir));
        assert_eq!(p.app_dir.parent(), p.jukebox_dir.parent());
        assert!(p.syncer.starts_with(&p.jukebox_dir));
        assert!(p.ffmpeg_dir.starts_with(&p.jukebox_dir));

        if cfg!(windows) {
            assert!(p.exe.ends_with("ultrastardx.exe"));
            assert_eq!(p.themes_dir, p.app_dir.join("themes"));
        } else {
            assert!(p.exe.ends_with("usr/bin/ultrastardx"));
            assert!(p.themes_dir.ends_with("usr/share/ultrastardx/themes"));
        }
    }
}
