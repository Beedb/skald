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
