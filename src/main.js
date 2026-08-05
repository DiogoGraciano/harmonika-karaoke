const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);

const el = {
  notas: $("notas"),
  ponto: $("ponto"),
  andamento: $("andamento-texto"),
  recado: $("recado"),
  btnInstalar: $("btn-instalar"),
  btnAbrir: $("btn-abrir"),
  btnPasta: $("btn-pasta"),
  btnDesinstalar: $("btn-desinstalar"),
  musicas: $("musicas"),
  musicasConta: $("musicas-conta"),
  caminhos: $("caminhos"),
  rodapeElevacao: $("rodape-elevacao"),

  jukebox: $("jukebox"),
  jkPonto: $("jukebox-ponto"),
  jkTexto: $("jukebox-texto"),
  jkBarra: $("jukebox-barra"),
  jkBarraPreenche: $("jukebox-barra-preenche"),
  jkInstalar: $("btn-jukebox-instalar"),
  jkConta: $("btn-jukebox-conta"),
  jkAbrir: $("btn-jukebox-abrir"),
  jkParar: $("btn-jukebox-parar"),
  jkPalco: $("jukebox-palco"),
  jkQr: $("jukebox-qr"),
  jkEndereco: $("jukebox-endereco"),
  jkDica: $("jukebox-dica"),
  jkRecado: $("jukebox-recado"),
};

// Cada nota cobre uma faixa da barra de progresso que o backend emite.
// Os limites batem com os percentuais de `install` no lib.rs.
const FASES = [
  { fase: "baixar", de: 0, ate: 60 },
  { fase: "extrair", de: 60, ate: 78 },
  { fase: "marca", de: 78, ate: 88 },
  { fase: "configurar", de: 88, ate: 94 },
  { fase: "atalho", de: 94, ate: 100 },
];

function pintarNotas(pct) {
  for (const { fase, de, ate } of FASES) {
    const li = el.notas.querySelector(`[data-fase="${fase}"]`);
    if (!li) continue;
    const p = Math.min(1, Math.max(0, (pct - de) / (ate - de)));
    li.querySelector(".nota-preenche").style.setProperty("--p", p.toFixed(3));
    li.toggleAttribute("data-ativa", p > 0 && p < 1);
    li.toggleAttribute("data-feita", p >= 1);
  }
}

function andamento(texto, estado) {
  el.andamento.textContent = texto;
  el.ponto.dataset.estado = estado || "";
}

function recado(texto, tipo) {
  if (!texto) {
    el.recado.hidden = true;
    return;
  }
  el.recado.textContent = texto;
  el.recado.dataset.tipo = tipo || "info";
  el.recado.hidden = false;
}

function ocupado(estado) {
  el.btnInstalar.disabled = estado;
  el.btnAbrir.disabled = estado;
  el.btnPasta.disabled = estado;
  el.btnDesinstalar.disabled = estado;
}

function mostrarCaminhos(paths) {
  const linhas = [
    ["Aplicativo", paths.appDir],
    ["Músicas", paths.songs],
    ["Configuração", paths.config],
  ];
  el.caminhos.innerHTML = "";
  for (const [rotulo, valor] of linhas) {
    const dt = document.createElement("dt");
    dt.textContent = rotulo;
    const dd = document.createElement("dd");
    dd.textContent = valor;
    el.caminhos.append(dt, dd);
  }
}

// ---- Jukebox ----

function recadoJukebox(texto, tipo) {
  if (!texto) {
    el.jkRecado.hidden = true;
    return;
  }
  el.jkRecado.textContent = texto;
  el.jkRecado.dataset.tipo = tipo || "info";
  el.jkRecado.hidden = false;
}

function ocupadoJukebox(estado) {
  el.jkInstalar.disabled = estado;
  el.jkConta.disabled = estado;
  el.jkAbrir.disabled = estado;
  el.jkParar.disabled = estado;
}

function barraJukebox(pct) {
  el.jkBarra.hidden = pct === null;
  if (pct !== null) {
    el.jkBarraPreenche.style.setProperty("--p", (pct / 100).toFixed(3));
  }
}

// O que os convidados conseguem fazer depende do release do sincronizador
// instalado: `--allow-downloading` só existe a partir da 0.25.0.
function dicaJukebox(jk) {
  return jk.requests
    ? "Quem pedir uma música pelo celular, esta máquina baixa na hora."
    : "Nesta versão do sincronizador os convidados procuram e votam — quem baixa é você, pelo sincronizador.";
}

function aplicarJukebox(jk) {
  el.jkTexto.textContent = jk.detail;
  el.jkPonto.dataset.estado = jk.running ? "ok" : "";

  el.jkInstalar.hidden = jk.running || (jk.installed && !jk.outdated);
  el.jkInstalar.textContent = jk.installed
    ? "Atualizar o jukebox"
    : "Instalar o jukebox";
  el.jkConta.hidden = !jk.installed || jk.running;
  el.jkAbrir.hidden = !jk.installed || jk.running;
  el.jkParar.hidden = !jk.running;

  el.jkPalco.hidden = !jk.running;
  if (jk.running) {
    // SVG gerado pelo próprio backend, não vem de fora.
    el.jkQr.innerHTML = jk.qr || "";
    el.jkEndereco.textContent = jk.address || "";
    el.jkDica.textContent = dicaJukebox(jk);
  }
}

async function atualizarJukebox() {
  const jk = await invoke("jukebox_status");
  aplicarJukebox(jk);
  return jk;
}

function aplicarStatus(st) {
  andamento(st.detail, st.installed ? "ok" : "");
  mostrarCaminhos(st.paths);

  el.btnInstalar.textContent = st.installed
    ? "Reinstalar o karaokê"
    : "Instalar o karaokê";
  el.btnAbrir.hidden = !st.installed;
  el.btnDesinstalar.hidden = !st.installed;
  el.musicas.hidden = !st.installed;
  // O jukebox alimenta a pasta de músicas do karaokê — sem karaokê instalado
  // ele não tem para onde baixar.
  el.jukebox.hidden = !st.installed;

  if (st.installed) {
    el.musicasConta.textContent =
      st.songs === 0
        ? "Nenhuma música ainda. Cada música é uma pasta com o arquivo .txt e o áudio juntos."
        : st.songs === 1
          ? "1 música pronta para cantar."
          : `${st.songs} músicas prontas para cantar.`;
    pintarNotas(100);
  } else {
    pintarNotas(0);
  }
}

async function atualizarStatus() {
  const st = await invoke("status");
  aplicarStatus(st);
  return st;
}

async function iniciar() {
  try {
    const info = await invoke("platform_info");
    el.rodapeElevacao.textContent = `${info.elevation} Baixa o UltraStar Deluxe ${info.usdxVersion} do site oficial do projeto e confere o SHA-256 antes de instalar.`;
  } catch (e) {
    // Sem essa informacao o instalador ainda funciona; segue em frente.
  }

  try {
    await atualizarStatus();
  } catch (e) {
    andamento("Não foi possível verificar esta máquina.", "erro");
    recado(String(e), "erro");
  }

  try {
    await atualizarJukebox();
  } catch (e) {
    el.jkTexto.textContent = "Não foi possível verificar o jukebox.";
    el.jkPonto.dataset.estado = "erro";
  }

  await listen("progresso", (evento) => {
    const { pct, message } = evento.payload;
    pintarNotas(pct);
    andamento(message, pct >= 100 ? "ok" : "trabalhando");
  });

  await listen("progresso-jukebox", (evento) => {
    const { pct, message } = evento.payload;
    barraJukebox(pct);
    el.jkTexto.textContent = message;
    el.jkPonto.dataset.estado = pct >= 100 ? "ok" : "trabalhando";
  });
}

el.btnInstalar.addEventListener("click", async () => {
  ocupado(true);
  recado(null);
  pintarNotas(0);
  andamento("Começando…", "trabalhando");
  try {
    const st = await invoke("install");
    aplicarStatus(st);
    await atualizarJukebox();
    recado(
      "Pronto. O karaokê já está no menu de aplicativos como Harmonika&Co Karaokê.",
      "ok",
    );
  } catch (e) {
    pintarNotas(0);
    andamento("A instalação parou.", "erro");
    recado(String(e), "erro");
    try {
      await atualizarStatus();
    } catch (_) {}
  } finally {
    ocupado(false);
  }
});

el.btnAbrir.addEventListener("click", async () => {
  try {
    await invoke("launch");
    recado("Abrindo o karaokê…", "ok");
  } catch (e) {
    recado(String(e), "erro");
  }
});

el.btnPasta.addEventListener("click", async () => {
  try {
    await invoke("open_songs_folder");
  } catch (e) {
    recado(String(e), "erro");
  }
});

el.jkInstalar.addEventListener("click", async () => {
  ocupadoJukebox(true);
  recadoJukebox(null);
  barraJukebox(0);
  el.jkTexto.textContent = "Começando…";
  el.jkPonto.dataset.estado = "trabalhando";
  try {
    aplicarJukebox(await invoke("jukebox_install"));
    recadoJukebox(
      "Jukebox instalado. Antes da festa, abra o sincronizador uma vez para " +
        "entrar na conta do usdb.eu — é esse login que traz a lista de músicas.",
      "ok",
    );
  } catch (e) {
    el.jkPonto.dataset.estado = "erro";
    recadoJukebox(String(e), "erro");
    try {
      await atualizarJukebox();
    } catch (_) {}
  } finally {
    barraJukebox(null);
    ocupadoJukebox(false);
  }
});

el.jkConta.addEventListener("click", async () => {
  try {
    await invoke("jukebox_configure");
    recadoJukebox(
      "Abrindo o sincronizador. Entre na conta do usdb.eu e espere a lista de " +
        "músicas carregar — o jukebox mostra o que estiver nessa lista.",
      "ok",
    );
  } catch (e) {
    recadoJukebox(String(e), "erro");
  }
});

el.jkAbrir.addEventListener("click", async () => {
  ocupadoJukebox(true);
  recadoJukebox(null);
  el.jkTexto.textContent = "Subindo o jukebox…";
  el.jkPonto.dataset.estado = "trabalhando";
  try {
    aplicarJukebox(await invoke("jukebox_start"));
  } catch (e) {
    el.jkPonto.dataset.estado = "erro";
    recadoJukebox(String(e), "erro");
    try {
      await atualizarJukebox();
    } catch (_) {}
  } finally {
    ocupadoJukebox(false);
  }
});

el.jkParar.addEventListener("click", async () => {
  ocupadoJukebox(true);
  try {
    aplicarJukebox(await invoke("jukebox_stop"));
    recadoJukebox("Jukebox fora do ar.", "ok");
  } catch (e) {
    recadoJukebox(String(e), "erro");
  } finally {
    ocupadoJukebox(false);
  }
});

el.btnDesinstalar.addEventListener("click", async () => {
  ocupado(true);
  recado(null);
  andamento("Removendo…", "trabalhando");
  try {
    const st = await invoke("uninstall");
    aplicarStatus(st);
    // A desinstalação leva o jukebox junto; o cartão precisa refletir isso.
    await atualizarJukebox();
    recado(
      "Karaokê e jukebox removidos. As músicas e a configuração continuam no lugar.",
      "ok",
    );
  } catch (e) {
    andamento("Não foi possível remover.", "erro");
    recado(String(e), "erro");
  } finally {
    ocupado(false);
  }
});

iniciar();
