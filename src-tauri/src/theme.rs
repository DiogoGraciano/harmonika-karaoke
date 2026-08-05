//! Geracao do tema Harmonika a partir do tema Modern que vem no UltraStar.
//!
//! A ideia e nao carregar uma copia do `Modern.ini` (160KB) dentro do
//! instalador: baixamos o UltraStar, lemos o `Modern.ini` daquela versao e
//! reescrevemos so as chaves que interessam. Assim o tema acompanha a versao
//! do UltraStar em vez de congelar num snapshot.
//!
//! Tudo aqui e funcao pura sobre texto — sem tocar em disco — para poder ser
//! testado com `cargo test`.

/// Nome do tema e do skin (precisam bater entre `Harmonika.ini` e o skin).
pub const THEME_NAME: &str = "Harmonika";

/// Cores da marca, extraidas do logotipo da empresa.
/// Entram no par `LightOrange`/`DarkOrange` porque o skin usa `Color=Orange`:
/// e o que faz botoes, barras e destaques da interface inteira mudarem de cor
/// sem precisar reeditar textura por textura.
pub const BRAND_ORANGE: &str = "247 92 31";
pub const BRAND_ORANGE_DARK: &str = "176 56 16";

/// Erro de reescrita: a chave esperada nao existe no arquivo original.
/// Falhamos alto de proposito — se o UltraStar renomear uma secao numa versao
/// futura, e melhor o instalador parar do que gerar um tema sem a marca.
fn missing(section: &str, key: &str) -> String {
    format!(
        "chave '{key}' nao encontrada na secao [{section}] do tema original \
         (o formato do UltraStar mudou nesta versao?)"
    )
}

/// Reescreve o valor de `key` dentro de `section`, preservando o espacamento
/// original em volta do `=`. Devolve erro se a chave nao existir.
fn set_key(ini: &str, section: &str, key: &str, value: &str) -> Result<String, String> {
    let mut out = String::with_capacity(ini.len());
    let mut in_section = false;
    let mut hit = false;

    for line in ini.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_section = trimmed[1..trimmed.len() - 1].eq_ignore_ascii_case(section);
        } else if in_section && !hit {
            if let Some(eq) = line.find('=') {
                if line[..eq].trim().eq_ignore_ascii_case(key) {
                    // mantem tudo ate o '=' como estava e troca so o valor
                    out.push_str(&line[..=eq]);
                    if line[eq + 1..].starts_with(' ') {
                        out.push(' ');
                    }
                    out.push_str(value);
                    out.push('\n');
                    hit = true;
                    continue;
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }

    if hit {
        Ok(out)
    } else {
        Err(missing(section, key))
    }
}

/// Substitui um texto que precisa existir; erro se nao aparecer.
fn replace_required(s: &str, from: &str, to: &str) -> Result<String, String> {
    if !s.contains(from) {
        return Err(format!(
            "trecho '{from}' nao encontrado no tema original \
             (o formato do UltraStar mudou nesta versao?)"
        ));
    }
    Ok(s.replace(from, to))
}

/// Gera o `Harmonika.ini` (definicao do tema) a partir do `Modern.ini`.
pub fn theme_ini(modern_ini: &str) -> Result<String, String> {
    let s = set_key(modern_ini, "Theme", "Name", THEME_NAME)?;
    let s = set_key(&s, "Theme", "Creator", "Harmonika&Co")?;
    let s = set_key(&s, "Theme", "DefaultSkin", THEME_NAME)?;
    let s = set_key(&s, "Colors", "LightOrange", BRAND_ORANGE)?;
    let s = set_key(&s, "Colors", "DarkOrange", BRAND_ORANGE_DARK)?;
    Ok(s)
}

/// Gera o skin `Harmonika/Harmonika.ini` a partir do `Modern/Blue.ini`.
pub fn skin_ini(blue_ini: &str) -> Result<String, String> {
    let s = set_key(blue_ini, "Skin", "Theme", THEME_NAME)?;
    let s = set_key(&s, "Skin", "Name", THEME_NAME)?;
    let s = set_key(&s, "Skin", "Color", "Orange")?;
    // Os fundos sao referenciados por nome de arquivo dentro de [Textures].
    let s = replace_required(&s, "[bg-load]blue.jpg", "[bg-load]harmonika.jpg")?;
    let s = replace_required(&s, "[bg-main]blue.jpg", "[bg-main]harmonika.jpg")?;
    Ok(s)
}

/// Monta o `harmonika-config.ini` do UltraStar ja apontando para o tema da
/// empresa. O app e iniciado com `-ConfigFile` apontando para este arquivo,
/// o que mantem a configuracao isolada: abrir um UltraStar comum na mesma
/// maquina nao reescreve o tema de volta para o padrao.
pub fn config_ini(songs_dir: &str) -> String {
    format!(
        "[Themes]\n\
         Theme={theme}\n\
         Skin={theme}\n\
         Color=Orange\n\
         \n\
         [Game]\n\
         Language=Portuguese\n\
         \n\
         [Directories]\n\
         SongDir1={songs}\n",
        theme = THEME_NAME,
        songs = songs_dir
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODERN: &str = "[Theme]\n\
Name = Modern\n\
Creator = Ultrastar Deluxe Team\n\
US_Version = USD 110\n\
DefaultSkin = Blue\n\
\n\
[Colors]\n\
White = 255 255 255\n\
LightOrange = 168 155 136\n\
DarkOrange = 151 131 76\n\
LightBlue = 119 187 210\n";

    const BLUE: &str = "[Skin]\n\
Theme=Modern\n\
Name=Blue\n\
Color=Blue\n\
\n\
[Textures]\n\
LoadingBG      = [bg-load]blue.jpg\n\
MainBG         = [bg-main]blue.jpg\n\
SongBG         = [bg-main]blue.jpg\n";

    #[test]
    fn tema_recebe_nome_e_cores_da_marca() {
        let t = theme_ini(MODERN).unwrap();
        assert!(t.contains("Name = Harmonika"));
        assert!(t.contains("Creator = Harmonika&Co"));
        assert!(t.contains("DefaultSkin = Harmonika"));
        assert!(t.contains("LightOrange = 247 92 31"));
        assert!(t.contains("DarkOrange = 176 56 16"));
        // nao pode sobrar o valor antigo
        assert!(!t.contains("168 155 136"));
        assert!(!t.contains("Name = Modern"));
    }

    #[test]
    fn tema_preserva_o_resto_do_arquivo() {
        let t = theme_ini(MODERN).unwrap();
        assert!(t.contains("US_Version = USD 110"));
        assert!(t.contains("White = 255 255 255"));
        assert!(t.contains("LightBlue = 119 187 210"));
    }

    #[test]
    fn skin_aponta_para_os_fundos_da_marca() {
        let s = skin_ini(BLUE).unwrap();
        assert!(s.contains("Theme=Harmonika"));
        assert!(s.contains("Name=Harmonika"));
        assert!(s.contains("Color=Orange"));
        assert!(s.contains("[bg-load]harmonika.jpg"));
        assert!(s.contains("[bg-main]harmonika.jpg"));
        assert!(!s.contains("blue.jpg"));
    }

    #[test]
    fn set_key_so_altera_dentro_da_secao_certa() {
        // 'Name' existe nas duas secoes; so a de [Skin] pode mudar
        let ini = "[Skin]\nName=Blue\n\n[Outra]\nName=Blue\n";
        let out = set_key(ini, "Skin", "Name", "Harmonika").unwrap();
        assert!(out.contains("[Skin]\nName=Harmonika"));
        assert!(out.contains("[Outra]\nName=Blue"));
    }

    #[test]
    fn falha_alto_se_o_formato_mudar() {
        let sem_colors = "[Theme]\nName = Modern\nCreator = X\nDefaultSkin = Blue\n";
        let err = theme_ini(sem_colors).unwrap_err();
        assert!(err.contains("LightOrange"), "erro inesperado: {err}");

        let sem_bg = "[Skin]\nTheme=Modern\nName=Blue\nColor=Blue\n";
        assert!(skin_ini(sem_bg).unwrap_err().contains("bg-load"));
    }

    #[test]
    fn config_aponta_tema_e_pasta_de_musicas() {
        let c = config_ini("/home/user/.ultrastardx/songs");
        assert!(c.contains("Theme=Harmonika"));
        assert!(c.contains("Skin=Harmonika"));
        assert!(c.contains("Language=Portuguese"));
        assert!(c.contains("SongDir1=/home/user/.ultrastardx/songs"));
    }
}
