//! Sincronização pelo Google Drive: conta (OAuth com PKCE e retorno em 127.0.0.1) e a pasta
//! `Skáld` (com `musicas/` e `dados/`). Escopo `drive.file`: o Skáld só enxerga o que ele criou.
//! Internet só quando a pessoa pede (entrar, sair, sincronizar).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const ESCOPO: &str = "https://www.googleapis.com/auth/drive.file";
const AUTORIZAR: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN: &str = "https://oauth2.googleapis.com/token";
const REVOGAR: &str = "https://oauth2.googleapis.com/revoke";
const API: &str = "https://www.googleapis.com/drive/v3";
const PASTA: &str = "Skáld";
const TIPO_PASTA: &str = "application/vnd.google-apps.folder";
/// Tempo para a pessoa autorizar no navegador.
const ESPERA: Duration = Duration::from_secs(300);

/// Pedido de cancelar o login em andamento (botão "Cancelar").
static CANCELAR: AtomicBool = AtomicBool::new(false);
static ENTRANDO: AtomicBool = AtomicBool::new(false);

/// Credenciais do cliente OAuth, gravadas na compilação (ver `build.rs`).
fn credenciais() -> Option<(&'static str, &'static str)> {
    match (option_env!("SKALD_GOOGLE_CLIENT_ID"), option_env!("SKALD_GOOGLE_CLIENT_SECRET")) {
        (Some(i), Some(s)) if !i.is_empty() && !s.is_empty() => Some((i, s)),
        _ => None,
    }
}

/// Conta conectada, guardada em `google.json` na pasta de dados (fora do banco, para não ir
/// junto no backup .db). Só o dono do arquivo lê.
#[derive(Serialize, Deserialize, Clone)]
struct Conta {
    refresh_token: String,
    email: String,
    nome: String,
    /// IDs das pastas no Drive.
    pasta: String,
    musicas: String,
    dados: String,
}

#[derive(Serialize, Debug)]
pub struct Estado {
    /// Esta cópia do Skáld foi compilada com as credenciais do Google.
    pub disponivel: bool,
    pub conectado: bool,
    pub email: Option<String>,
    pub nome: Option<String>,
}

fn arquivo(dados: &Path) -> PathBuf {
    dados.join("google.json")
}

fn carregar(dados: &Path) -> Option<Conta> {
    serde_json::from_str(&std::fs::read_to_string(arquivo(dados)).ok()?).ok()
}

fn gravar(dados: &Path, conta: &Conta) -> Result<(), String> {
    std::fs::create_dir_all(dados).map_err(|e| e.to_string())?;
    let destino = arquivo(dados);
    let tmp = destino.with_extension("tmp");
    let texto = serde_json::to_string(conta).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, texto).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, &destino).map_err(|e| e.to_string())
}

pub fn estado(dados: &Path) -> Estado {
    let conta = carregar(dados);
    Estado {
        disponivel: credenciais().is_some(),
        conectado: conta.is_some(),
        email: conta.as_ref().map(|c| c.email.clone()),
        nome: conta.map(|c| c.nome),
    }
}

/// Texto aleatório seguro (base64 sem preenchimento) com `bytes` bytes de entropia.
fn aleatorio(bytes: usize) -> Result<String, String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf))
}

fn desafio_pkce(verificador: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verificador.as_bytes()))
}

fn erro(e: ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(401) | ureq::Error::StatusCode(400) => {
            "O Google recusou o acesso. Saia e entre de novo.".into()
        }
        e => crate::enriquecer::erro_rede(e, "Google Drive"),
    }
}

#[derive(Deserialize)]
struct Token {
    access_token: String,
    refresh_token: Option<String>,
}

/// Login completo: abre o navegador (`abrir`), espera a autorização, cria ou acha as pastas e
/// guarda a conta. Bloqueia até terminar, cancelar ou passar `ESPERA`.
pub fn entrar(dados: &Path, abrir: impl FnOnce(&str) -> Result<(), String>) -> Result<Estado, String> {
    let (id, segredo) = credenciais().ok_or("A sincronização não está disponível nesta versão do Skáld.")?;
    if ENTRANDO.swap(true, Ordering::SeqCst) {
        return Err("Já há um login em andamento no navegador.".into());
    }
    CANCELAR.store(false, Ordering::SeqCst);
    let r = (|| {
        let verificador = aleatorio(48)?;
        let estado_oauth = aleatorio(16)?;
        let ouvinte = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let retorno = format!("http://127.0.0.1:{}", ouvinte.local_addr().map_err(|e| e.to_string())?.port());
        let consulta = form_urlencoded::Serializer::new(String::new())
            .extend_pairs([
                ("client_id", id),
                ("redirect_uri", retorno.as_str()),
                ("response_type", "code"),
                ("scope", ESCOPO),
                ("code_challenge", desafio_pkce(&verificador).as_str()),
                ("code_challenge_method", "S256"),
                ("state", estado_oauth.as_str()),
                ("access_type", "offline"),
                ("prompt", "consent"),
            ])
            .finish();
        abrir(&format!("{AUTORIZAR}?{consulta}"))?;
        let codigo = esperar_codigo(&ouvinte, &estado_oauth)?;

        let texto = crate::enriquecer::agente()
            .post(TOKEN)
            .send_form([
                ("code", codigo.as_str()),
                ("client_id", id),
                ("client_secret", segredo),
                ("redirect_uri", retorno.as_str()),
                ("grant_type", "authorization_code"),
                ("code_verifier", verificador.as_str()),
            ])
            .map_err(erro)?
            .body_mut()
            .read_to_string()
            .map_err(|e| e.to_string())?;
        let token: Token = serde_json::from_str(&texto).map_err(|e| e.to_string())?;
        let refresh_token = token
            .refresh_token
            .ok_or("O Google não liberou o acesso contínuo. Tente entrar de novo.")?;
        let acesso = token.access_token;

        let (email, nome) = usuario(&acesso)?;
        let pasta = achar_ou_criar_pasta(&acesso, PASTA, "root")?;
        let musicas = achar_ou_criar_pasta(&acesso, "musicas", &pasta)?;
        let dados_drive = achar_ou_criar_pasta(&acesso, "dados", &pasta)?;
        gravar(
            dados,
            &Conta { refresh_token, email, nome, pasta, musicas, dados: dados_drive },
        )?;
        Ok(estado(dados))
    })();
    ENTRANDO.store(false, Ordering::SeqCst);
    r
}

pub fn cancelar() {
    CANCELAR.store(true, Ordering::SeqCst);
}

/// Desconecta: revoga o acesso no Google (se houver internet) e apaga a conta guardada.
/// Nada no Drive é apagado.
pub fn sair(dados: &Path) -> Result<Estado, String> {
    if let Some(conta) = carregar(dados).filter(|c| !c.refresh_token.is_empty()) {
        let _ = crate::enriquecer::agente()
            .post(REVOGAR)
            .send_form([("token", conta.refresh_token.as_str())]);
    }
    match std::fs::remove_file(arquivo(dados)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    Ok(estado(dados))
}

/// Espera o navegador voltar para `http://127.0.0.1:<porta>/?code=…&state=…`.
fn esperar_codigo(ouvinte: &TcpListener, estado_oauth: &str) -> Result<String, String> {
    ouvinte.set_nonblocking(true).map_err(|e| e.to_string())?;
    let limite = Instant::now() + ESPERA;
    loop {
        if CANCELAR.load(Ordering::SeqCst) {
            return Err("Login cancelado.".into());
        }
        if Instant::now() > limite {
            return Err("O tempo para autorizar no navegador acabou. Tente de novo.".into());
        }
        let mut conexao = match ouvinte.accept() {
            Ok((c, _)) => c,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(150));
                continue;
            }
            Err(e) => return Err(e.to_string()),
        };
        let _ = conexao.set_nonblocking(false);
        let _ = conexao.set_read_timeout(Some(Duration::from_secs(5)));
        let mut linha = String::new();
        let _ = BufReader::new(&conexao).read_line(&mut linha);
        // "GET /?code=…&state=… HTTP/1.1"
        let caminho = linha.split_whitespace().nth(1).unwrap_or("");
        let Some(consulta) = caminho.strip_prefix("/?") else {
            responder(&mut conexao, "404 Not Found", "");
            continue;
        };
        let pares: HashMap<String, String> = form_urlencoded::parse(consulta.as_bytes()).into_owned().collect();
        if pares.get("state").map(String::as_str) != Some(estado_oauth) {
            responder(&mut conexao, "400 Bad Request", "Pedido inválido. Volte ao Skáld e tente de novo.");
            continue;
        }
        if let Some(e) = pares.get("error") {
            responder(&mut conexao, "200 OK", "Acesso não autorizado. Pode fechar esta aba.");
            return Err(if e == "access_denied" {
                "Você não autorizou o acesso ao Google Drive.".into()
            } else {
                format!("O Google recusou o login ({e}).")
            });
        }
        if let Some(c) = pares.get("code") {
            responder(&mut conexao, "200 OK", "Pronto! Pode fechar esta aba e voltar ao Skáld.");
            return Ok(c.clone());
        }
        responder(&mut conexao, "400 Bad Request", "Pedido inválido.");
    }
}

fn responder(conexao: &mut TcpStream, status: &str, mensagem: &str) {
    let corpo = format!(
        "<!doctype html><html lang=\"pt-BR\"><meta charset=\"utf-8\"><title>Skáld</title>\
         <body style=\"font-family:sans-serif;text-align:center;padding-top:4em\"><h1>Skáld</h1><p>{mensagem}</p></body></html>"
    );
    let _ = write!(
        conexao,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
        corpo.len()
    );
}

fn get_json(acesso: &str, url: &str) -> Result<serde_json::Value, String> {
    let texto = crate::enriquecer::agente()
        .get(url)
        .header("Authorization", &format!("Bearer {acesso}"))
        .call()
        .map_err(erro)?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&texto).map_err(|e| e.to_string())
}

/// E-mail e nome da conta (`about` funciona com o escopo `drive.file`).
fn usuario(acesso: &str) -> Result<(String, String), String> {
    let v = get_json(acesso, &format!("{API}/about?fields=user(displayName,emailAddress)"))?;
    let campo = |c: &str| v["user"][c].as_str().unwrap_or("").to_string();
    Ok((campo("emailAddress"), campo("displayName")))
}

/// Pasta `nome` dentro de `pai` criada pelo Skáld (a mais antiga, se houver várias); cria se não houver.
fn achar_ou_criar_pasta(acesso: &str, nome: &str, pai: &str) -> Result<String, String> {
    let q = format!(
        "name = '{}' and mimeType = '{TIPO_PASTA}' and '{}' in parents and trashed = false",
        nome.replace('\\', "\\\\").replace('\'', "\\'"),
        pai
    );
    let consulta = form_urlencoded::Serializer::new(String::new())
        .extend_pairs([("q", q.as_str()), ("fields", "files(id)"), ("orderBy", "createdTime"), ("spaces", "drive")])
        .finish();
    let v = get_json(acesso, &format!("{API}/files?{consulta}"))?;
    if let Some(id) = v["files"][0]["id"].as_str() {
        return Ok(id.to_string());
    }
    let corpo = serde_json::json!({ "name": nome, "mimeType": TIPO_PASTA, "parents": [pai] }).to_string();
    let texto = crate::enriquecer::agente()
        .post(&format!("{API}/files?fields=id"))
        .header("Authorization", &format!("Bearer {acesso}"))
        .header("Content-Type", "application/json; charset=utf-8")
        .send(corpo)
        .map_err(erro)?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&texto).map_err(|e| e.to_string())?;
    v["id"].as_str().map(str::to_string).ok_or_else(|| "O Google Drive não devolveu a pasta criada.".into())
}

// ---------- Músicas (etapa 64) ----------

const UPLOAD: &str = "https://www.googleapis.com/upload/drive/v3/files";
static SINCRONIZANDO: AtomicBool = AtomicBool::new(false);

/// Agente para arquivos grandes: sem prazo total, só por leitura/escrita.
fn agente_arquivos() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_per_read(Some(Duration::from_secs(60)))
        .timeout_per_write(Some(Duration::from_secs(60)))
        .user_agent(crate::enriquecer::CLIENTE)
        .build()
        .into()
}

/// Token de acesso renovado pelo refresh token (vale 1 h; renova antes de 45 min).
struct Acesso {
    id: &'static str,
    segredo: &'static str,
    refresh: String,
    token: String,
    desde: Instant,
}

impl Acesso {
    fn novo(conta: &Conta) -> Result<Self, String> {
        let (id, segredo) = credenciais().ok_or("A sincronização não está disponível nesta versão do Skáld.")?;
        let mut a = Acesso { id, segredo, refresh: conta.refresh_token.clone(), token: String::new(), desde: Instant::now() };
        a.renovar()?;
        Ok(a)
    }

    fn renovar(&mut self) -> Result<(), String> {
        let texto = crate::enriquecer::agente()
            .post(TOKEN)
            .send_form([
                ("client_id", self.id),
                ("client_secret", self.segredo),
                ("refresh_token", self.refresh.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .map_err(erro)?
            .body_mut()
            .read_to_string()
            .map_err(|e| e.to_string())?;
        let t: Token = serde_json::from_str(&texto).map_err(|e| e.to_string())?;
        self.token = t.access_token;
        self.desde = Instant::now();
        Ok(())
    }

    fn cabecalho(&mut self) -> Result<String, String> {
        if self.desde.elapsed() > Duration::from_secs(45 * 60) {
            self.renovar()?;
        }
        Ok(format!("Bearer {}", self.token))
    }
}

/// Música do aparelho com hash conhecido.
pub struct Local {
    pub hash: String,
    pub caminho: PathBuf,
}

#[derive(Serialize, Clone)]
pub struct Progresso {
    /// "listando", "subindo" ou "baixando".
    pub fase: &'static str,
    pub feito: u32,
    pub total: u32,
    pub nome: String,
}

#[derive(Serialize, Default, Debug)]
pub struct ResumoSync {
    pub subidas: u32,
    pub baixadas: u32,
    /// Já estavam nos dois lados.
    pub iguais: u32,
    /// Erros por arquivo (os primeiros); o resto continua.
    pub falhas: Vec<String>,
    pub total_falhas: u32,
    pub cancelado: bool,
}

impl ResumoSync {
    fn falhou(&mut self, nome: &str, e: String) {
        self.total_falhas += 1;
        if self.falhas.len() < 5 {
            self.falhas.push(format!("{nome}: {e}"));
        }
    }
}

struct Remoto {
    id: String,
    hash: String,
    nome: String,
}

/// Arquivos da pasta `musicas/` (nome original; hash em `appProperties.hash`).
fn listar_musicas(acesso: &mut Acesso, pasta: &str) -> Result<Vec<Remoto>, String> {
    let mut lista = Vec::new();
    let mut pagina: Option<String> = None;
    loop {
        let q = format!("'{pasta}' in parents and trashed = false");
        let mut pares = vec![
            ("q", q.as_str()),
            ("fields", "nextPageToken,files(id,name,appProperties)"),
            ("pageSize", "1000"),
            ("spaces", "drive"),
        ];
        if let Some(p) = &pagina {
            pares.push(("pageToken", p.as_str()));
        }
        let consulta = form_urlencoded::Serializer::new(String::new()).extend_pairs(pares).finish();
        let v = get_json_com(acesso, &format!("{API}/files?{consulta}"))?;
        for f in v["files"].as_array().into_iter().flatten() {
            let (Some(id), Some(nome_drive)) = (f["id"].as_str(), f["name"].as_str()) else { continue };
            // Hash em `appProperties.hash`; os primeiros envios usavam `<hash>.<ext>` como nome.
            let hash = f["appProperties"]["hash"]
                .as_str()
                .unwrap_or_else(|| nome_drive.split('.').next().unwrap_or(""));
            if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                continue;
            }
            let nome = f["appProperties"]["nome"].as_str().unwrap_or(nome_drive).to_string();
            lista.push(Remoto { id: id.into(), hash: hash.to_ascii_lowercase(), nome });
        }
        match v["nextPageToken"].as_str() {
            Some(p) => pagina = Some(p.to_string()),
            None => return Ok(lista),
        }
    }
}

fn get_json_com(acesso: &mut Acesso, url: &str) -> Result<serde_json::Value, String> {
    let texto = crate::enriquecer::agente()
        .get(url)
        .header("Authorization", &acesso.cabecalho()?)
        .call()
        .map_err(erro)?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&texto).map_err(|e| e.to_string())
}

/// Sobe um arquivo (envio retomável numa parte só) com o nome original; o hash vai em
/// `appProperties.hash` (o Drive aceita nomes repetidos; quem identifica é o hash).
fn subir(acesso: &mut Acesso, pasta: &str, local: &Local) -> Result<(), String> {
    let nome = local.caminho.file_name().and_then(|n| n.to_str()).unwrap_or("musica").to_string();
    let arquivo = std::fs::File::open(&local.caminho).map_err(|e| e.to_string())?;
    let tamanho = arquivo.metadata().map_err(|e| e.to_string())?.len();
    let meta = serde_json::json!({ "name": nome, "parents": [pasta], "appProperties": { "hash": local.hash } }).to_string();
    let resposta = crate::enriquecer::agente()
        .post(&format!("{UPLOAD}?uploadType=resumable&fields=id"))
        .header("Authorization", &acesso.cabecalho()?)
        .header("Content-Type", "application/json; charset=utf-8")
        .header("X-Upload-Content-Length", &tamanho.to_string())
        .send(meta)
        .map_err(erro)?;
    let destino = resposta
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .ok_or("O Google Drive não aceitou o envio.")?
        .to_string();
    agente_arquivos()
        .put(&destino)
        .header("Authorization", &acesso.cabecalho()?)
        .send(&arquivo)
        .map_err(erro)?;
    Ok(())
}

/// Nome de arquivo seguro (sem separadores nem caracteres proibidos no Windows).
fn nome_seguro(nome: &str) -> String {
    let limpo: String = nome
        .chars()
        .map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let limpo = limpo.trim().trim_start_matches('.').to_string();
    if limpo.is_empty() { "musica".into() } else { limpo }
}

/// Caminho livre em `pasta` para `nome` ("x.mp3", "x (2).mp3", ...).
fn caminho_livre(pasta: &Path, nome: &str) -> PathBuf {
    let p = Path::new(nome);
    let base = p.file_stem().and_then(|s| s.to_str()).unwrap_or(nome);
    let ext = p.extension().and_then(|e| e.to_str());
    let mut n = 1;
    loop {
        let candidato = match (n, ext) {
            (1, _) => pasta.join(nome),
            (_, Some(e)) => pasta.join(format!("{base} ({n}).{e}")),
            (_, None) => pasta.join(format!("{base} ({n})")),
        };
        if !candidato.exists() {
            return candidato;
        }
        n += 1;
    }
}

/// Baixa para `pasta` (arquivo `.parte` renomeado no fim) e confere o hash.
fn baixar(acesso: &mut Acesso, remoto: &Remoto, pasta: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(pasta).map_err(|e| e.to_string())?;
    let destino = caminho_livre(pasta, &nome_seguro(&remoto.nome));
    let parte = destino.with_extension(format!(
        "{}.parte",
        destino.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    let r = (|| {
        let mut resposta = agente_arquivos()
            .get(&format!("{API}/files/{}?alt=media", remoto.id))
            .header("Authorization", &acesso.cabecalho()?)
            .call()
            .map_err(erro)?;
        let mut leitor = resposta.body_mut().with_config().limit(u64::MAX).reader();
        let mut arquivo = std::fs::File::create(&parte).map_err(|e| e.to_string())?;
        std::io::copy(&mut leitor, &mut arquivo).map_err(|e| e.to_string())?;
        arquivo.sync_all().map_err(|e| e.to_string())?;
        drop(arquivo);
        let hash = crate::import::hash_arquivo(&parte).map_err(|e| e.to_string())?;
        if hash != remoto.hash {
            return Err("o arquivo baixado veio diferente do original".to_string());
        }
        std::fs::rename(&parte, &destino).map_err(|e| e.to_string())
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(&parte);
    }
    r.map(|()| destino)
}

/// "Sincronizar agora": sobe as músicas que o Drive não tem e baixa para `destino` as que o
/// aparelho não tem (`conhecidos` = hashes de todos os registros, presentes ou não, para não
/// baixar o que a pessoa só moveu). Nunca apaga nada. Devolve o resumo e os arquivos baixados.
pub fn sincronizar_musicas(
    dados: &Path,
    locais: Vec<Local>,
    conhecidos: std::collections::HashSet<String>,
    destino: &Path,
    mut progresso: impl FnMut(Progresso),
) -> Result<(ResumoSync, Vec<PathBuf>), String> {
    let conta = carregar(dados).ok_or("Entre com a conta do Google primeiro.")?;
    if SINCRONIZANDO.swap(true, Ordering::SeqCst) {
        return Err("A sincronização já está em andamento.".into());
    }
    CANCELAR.store(false, Ordering::SeqCst);
    let r = (|| {
        progresso(Progresso { fase: "listando", feito: 0, total: 0, nome: String::new() });
        let mut acesso = Acesso::novo(&conta)?;
        let remotos = listar_musicas(&mut acesso, &conta.musicas)?;
        let no_drive: std::collections::HashSet<&str> = remotos.iter().map(|r| r.hash.as_str()).collect();

        let mut resumo = ResumoSync::default();
        // Um arquivo por hash (cópias iguais no aparelho sobem uma vez).
        let mut vistos = std::collections::HashSet::new();
        let subir_lista: Vec<&Local> = locais
            .iter()
            .filter(|l| vistos.insert(l.hash.as_str()))
            .filter(|l| {
                let tem = no_drive.contains(l.hash.as_str());
                if tem {
                    resumo.iguais += 1;
                }
                !tem
            })
            .collect();
        let baixar_lista: Vec<&Remoto> = {
            let mut vistos = std::collections::HashSet::new();
            remotos
                .iter()
                .filter(|r| !conhecidos.contains(&r.hash) && vistos.insert(r.hash.as_str()))
                .collect()
        };

        for (i, local) in subir_lista.iter().enumerate() {
            if CANCELAR.load(Ordering::SeqCst) {
                resumo.cancelado = true;
                return Ok((resumo, Vec::new()));
            }
            let nome = local.caminho.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            progresso(Progresso { fase: "subindo", feito: i as u32, total: subir_lista.len() as u32, nome: nome.clone() });
            match subir(&mut acesso, &conta.musicas, local) {
                Ok(()) => resumo.subidas += 1,
                Err(e) => resumo.falhou(&nome, e),
            }
        }

        let mut baixados = Vec::new();
        for (i, remoto) in baixar_lista.iter().enumerate() {
            if CANCELAR.load(Ordering::SeqCst) {
                resumo.cancelado = true;
                break;
            }
            progresso(Progresso { fase: "baixando", feito: i as u32, total: baixar_lista.len() as u32, nome: remoto.nome.clone() });
            match baixar(&mut acesso, remoto, destino) {
                Ok(c) => {
                    resumo.baixadas += 1;
                    baixados.push(c);
                }
                Err(e) => resumo.falhou(&remoto.nome, e),
            }
        }
        Ok((resumo, baixados))
    })();
    SINCRONIZANDO.store(false, Ordering::SeqCst);
    r
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn pkce_do_rfc7636() {
        // Exemplo do apêndice B da RFC 7636.
        assert_eq!(
            desafio_pkce("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(aleatorio(48).unwrap().len(), 64);
    }

    #[test]
    fn nomes_de_arquivo() {
        assert_eq!(nome_seguro("AC/DC: Back?.mp3"), "AC_DC_ Back_.mp3");
        assert_eq!(nome_seguro("../x.flac"), "_x.flac");
        assert_eq!(nome_seguro("  "), "musica");
        let pasta = crate::teste_util::pasta("skald-drive-nomes");
        std::fs::write(pasta.join("a.mp3"), b"").unwrap();
        std::fs::write(pasta.join("a (2).mp3"), b"").unwrap();
        assert_eq!(caminho_livre(&pasta, "a.mp3"), pasta.join("a (3).mp3"));
        assert_eq!(caminho_livre(&pasta, "b.mp3"), pasta.join("b.mp3"));
    }

    #[test]
    fn conta_guardada_e_apagada() {
        let pasta = crate::teste_util::pasta("skald-drive");
        assert!(!estado(&pasta).conectado);
        let conta = Conta {
            refresh_token: "r".into(),
            email: "a@b.c".into(),
            nome: "A".into(),
            pasta: "1".into(),
            musicas: "2".into(),
            dados: "3".into(),
        };
        gravar(&pasta, &conta).unwrap();
        let e = estado(&pasta);
        assert!(e.conectado);
        assert_eq!(e.email.as_deref(), Some("a@b.c"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let modo = std::fs::metadata(arquivo(&pasta)).unwrap().permissions().mode();
            assert_eq!(modo & 0o777, 0o600);
        }
        // Sem rede a revogação falha calada; a conta sai do aparelho do mesmo jeito.
        std::fs::write(arquivo(&pasta), serde_json::to_string(&Conta { refresh_token: String::new(), ..conta }).unwrap()).unwrap();
        assert!(!sair(&pasta).unwrap().conectado);
    }
}
