//! Corretor HTTPS (ADR-0001, W5): a **única** porta de saída de rede do Pod. Só `GET` por `https` para
//! os hosts da lista permitida (`pod_model::net::ALLOWED_HOSTS`), com validação de certificado do
//! Windows (WinHTTP), sem redirecionamento, sem cookies, prazo curto e teto de tamanho na resposta.
//! Roda em threads de trabalho (as chamadas bloqueiam); nunca na thread de UI.
//!
//! Quem monta a URL é o host (`Endpoint::url`); este módulo recusa qualquer URL fora da lista.

use windows::core::{w, HSTRING};
use windows::Win32::Networking::WinHttp::*;

use pod_model::net::{url_allowed, MAX_RESPONSE_BYTES};

/// Prazo por etapa (resolver, conectar, enviar, receber), em milissegundos.
pub const TIMEOUT_MS: i32 = 8_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    /// A URL não passou na lista permitida (nunca chegou à rede).
    Refused,
    /// Sem conexão, DNS, TLS ou prazo esgotado.
    Network(String),
    /// Resposta HTTP que não é 200.
    Status(u16),
    TooLarge,
}

impl NetError {
    /// Mensagem curta para o cartão.
    pub fn short(&self) -> String {
        match self {
            NetError::Refused => "Endere\u{e7}o n\u{e3}o permitido".to_string(),
            NetError::Network(_) => "Sem conex\u{e3}o".to_string(),
            NetError::Status(c) => format!("Servi\u{e7}o respondeu {c}"),
            NetError::TooLarge => "Resposta grande demais".to_string(),
        }
    }
}

struct Handle(*mut std::ffi::c_void);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn net_err(what: &str, e: windows::core::Error) -> NetError {
    NetError::Network(format!("{what}: {e}"))
}

/// Divide `https://host/caminho?consulta` em (host, "/caminho?consulta").
fn split_url(url: &str) -> Option<(&str, String)> {
    let rest = url.strip_prefix("https://")?;
    let (host, tail) = match rest.find(['/', '?']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let path = if tail.starts_with('/') { tail.to_string() } else { format!("/{tail}") };
    Some((host, path))
}

/// `GET` HTTPS. Devolve o corpo (até `MAX_RESPONSE_BYTES`) se o status for 200.
pub fn https_get(url: &str) -> Result<Vec<u8>, NetError> {
    get_inner(url, None)
}

/// Como `https_get`, com `Authorization: Bearer` — **só** para `api.github.com`; para qualquer outro host
/// o token nunca é anexado (o pedido é recusado).
pub fn https_get_github(url: &str, token: &str) -> Result<Vec<u8>, NetError> {
    get_inner(url, Some(token))
}

fn get_inner(url: &str, token: Option<&str>) -> Result<Vec<u8>, NetError> {
    if !url_allowed(url) {
        return Err(NetError::Refused);
    }
    let (host, path) = split_url(url).ok_or(NetError::Refused)?;
    if token.is_some() && host != pod_model::github::API_HOST {
        return Err(NetError::Refused);
    }
    unsafe {
        let session = Handle(
            WinHttpOpen(w!("Pod/0.1"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, None, None, 0),
        );
        if session.0.is_null() {
            return Err(NetError::Network("WinHttpOpen".into()));
        }
        WinHttpSetTimeouts(session.0, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS).map_err(|e| net_err("prazos", e))?;
        let connect = Handle(WinHttpConnect(session.0, &HSTRING::from(host), INTERNET_DEFAULT_HTTPS_PORT as u16, 0));
        if connect.0.is_null() {
            return Err(NetError::Network("WinHttpConnect".into()));
        }
        let request = Handle(WinHttpOpenRequest(
            connect.0,
            w!("GET"),
            &HSTRING::from(path.as_str()),
            None,
            None,
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ));
        if request.0.is_null() {
            return Err(NetError::Network("WinHttpOpenRequest".into()));
        }
        // Sem redirecionamento: uma resposta 3xx é erro, nunca um desvio para outro host.
        let policy = WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_ne_bytes();
        WinHttpSetOption(Some(request.0), WINHTTP_OPTION_REDIRECT_POLICY, Some(&policy)).map_err(|e| net_err("redirecionamento", e))?;
        let mut headers = String::from("Accept: application/vnd.github+json
");
        if let Some(t) = token {
            headers.push_str(&format!("Authorization: Bearer {t}
"));
        }
        if host == pod_model::github::API_HOST {
            let wide: Vec<u16> = headers.encode_utf16().collect();
            WinHttpAddRequestHeaders(request.0, &wide, WINHTTP_ADDREQ_FLAG_ADD).map_err(|e| net_err("cabecalhos", e))?;
        }
        WinHttpSendRequest(request.0, None, None, 0, 0, 0).map_err(|e| net_err("enviar", e))?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|e| net_err("receber", e))?;

        let mut status: u32 = 0;
        let mut len = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            None,
            Some(&mut status as *mut u32 as *mut _),
            &mut len,
            std::ptr::null_mut(),
        )
        .map_err(|e| net_err("status", e))?;
        if status != 200 {
            return Err(NetError::Status(status as u16));
        }
        let mut body = Vec::new();
        loop {
            let mut avail = 0u32;
            WinHttpQueryDataAvailable(request.0, &mut avail).map_err(|e| net_err("dados", e))?;
            if avail == 0 {
                break;
            }
            if body.len() + avail as usize > MAX_RESPONSE_BYTES {
                return Err(NetError::TooLarge);
            }
            let start = body.len();
            body.resize(start + avail as usize, 0);
            let mut read = 0u32;
            WinHttpReadData(request.0, body[start..].as_mut_ptr() as *mut _, avail, &mut read).map_err(|e| net_err("ler", e))?;
            body.truncate(start + read as usize);
            if read == 0 {
                break;
            }
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_split_into_host_and_path() {
        assert_eq!(split_url("https://api.open-meteo.com/v1/forecast?a=1"), Some(("api.open-meteo.com", "/v1/forecast?a=1".to_string())));
        assert_eq!(split_url("https://api.open-meteo.com"), Some(("api.open-meteo.com", "/".to_string())));
        assert_eq!(split_url("https://api.open-meteo.com?x=1"), Some(("api.open-meteo.com", "/?x=1".to_string())));
        assert_eq!(split_url("http://x/"), None);
    }

    #[test]
    fn the_token_is_never_sent_to_another_host() {
        assert_eq!(https_get_github("https://api.open-meteo.com/v1/forecast", "ghp_x"), Err(NetError::Refused));
        assert_eq!(https_get_github("https://api.github.com.evil.com/user", "ghp_x"), Err(NetError::Refused));
        assert_eq!(https_get_github("http://api.github.com/user", "ghp_x"), Err(NetError::Refused));
    }

    #[test]
    fn refused_urls_never_touch_the_network() {
        for bad in ["http://api.open-meteo.com/", "https://example.com/", "https://api.open-meteo.com.evil.com/", ""] {
            assert_eq!(https_get(bad), Err(NetError::Refused), "{bad}");
        }
        assert_eq!(NetError::Refused.short(), "Endere\u{e7}o n\u{e3}o permitido");
        assert_eq!(NetError::Status(503).short(), "Servi\u{e7}o respondeu 503");
    }

    /// Chamada real à API do Open-Meteo (internet). `#[ignore]`: rode com `--ignored --nocapture`.
    #[test]
    #[ignore = "usa a internet"]
    fn real_https_get_to_open_meteo() {
        let url = pod_model::net::Endpoint::Geocode { name: "Recife".into() }.url().unwrap();
        let body = https_get(&url).expect("resposta");
        let place = pod_model::net::parse_geocode(std::str::from_utf8(&body).unwrap()).expect("lugar");
        eprintln!("geocodificacao real: {} ({}) {:.2},{:.2}", place.name, place.country, place.lat, place.lon);
        assert!(place.name.contains("Recife"));
        let f = pod_model::net::Endpoint::Forecast { lat: place.lat, lon: place.lon }.url().unwrap();
        let w = pod_model::net::parse_forecast(std::str::from_utf8(&https_get(&f).unwrap()).unwrap()).expect("previsao");
        eprintln!("previsao real: {:.1} graus, codigo {}", w.temp_c, w.code);
    }
}
