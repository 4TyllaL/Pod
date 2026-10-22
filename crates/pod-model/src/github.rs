//! Modelo puro do widget GitHub: pedidos montados pelo host, leitura das respostas da API e estado.
//! O token nunca passa por aqui: vive no cofre do Windows e só o corretor HTTPS o anexa, e só para
//! `api.github.com`.

use serde_json::Value;

/// Host da API (único que recebe o token).
pub const API_HOST: &str = "api.github.com";

/// Consultas que o host sabe montar (o widget nunca fornece URL).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Query {
    /// Quem é o dono do token.
    User,
    /// Notificações não lidas (até 30).
    Notifications,
    /// PRs abertos que pedem a revisão do usuário.
    ReviewRequests,
    /// PRs abertos do usuário.
    MyPulls,
}

impl Query {
    pub fn url(self) -> String {
        let base = "https://api.github.com";
        match self {
            Query::User => format!("{base}/user"),
            Query::Notifications => format!("{base}/notifications?per_page=30"),
            Query::ReviewRequests => format!("{base}/search/issues?q=is%3Apr+is%3Aopen+review-requested%3A%40me&per_page=3"),
            Query::MyPulls => format!("{base}/search/issues?q=is%3Apr+is%3Aopen+author%3A%40me&per_page=1"),
        }
    }
}

/// Token plausível: só caracteres de token do GitHub (`ghp_…`, `github_pat_…`, clássicos), 20 a 255.
pub fn clean_token(s: &str) -> Option<String> {
    let t = s.trim();
    let ok = (20..=255).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    ok.then(|| t.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pull {
    pub title: String,
    pub repo: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub login: String,
    /// Notificações não lidas (a API devolve até 30; `unread_capped` diz se bateu no teto).
    pub unread: u32,
    pub unread_capped: bool,
    pub review_requests: u32,
    pub my_pulls: u32,
    /// Os primeiros PRs que pedem revisão.
    pub reviews: Vec<Pull>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubState {
    /// "Widgets de internet" desligado.
    Off,
    /// Sem token no cofre.
    NoToken,
    Loading,
    Ready(Summary),
    /// Token recusado (revogado ou sem permissão).
    Rejected,
    Failed(String),
}

pub fn parse_login(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    let l = v.get("login")?.as_str()?;
    (!l.is_empty() && l.len() <= 64).then(|| l.to_string())
}

/// Quantidade de notificações no array devolvido.
pub fn parse_notification_count(body: &str) -> Option<u32> {
    let v: Value = serde_json::from_str(body).ok()?;
    Some(v.as_array()?.len() as u32)
}

/// `(total, primeiros itens)` de uma busca de PRs.
pub fn parse_search(body: &str) -> Option<(u32, Vec<Pull>)> {
    let v: Value = serde_json::from_str(body).ok()?;
    let total = v.get("total_count")?.as_u64()?.min(u32::MAX as u64) as u32;
    let items = v
        .get("items")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|i| {
                    let title = i.get("title")?.as_str()?.split_whitespace().collect::<Vec<_>>().join(" ");
                    // repository_url = https://api.github.com/repos/dono/nome
                    let repo = i.get("repository_url")?.as_str()?.rsplit('/').take(2).collect::<Vec<_>>();
                    let repo = if repo.len() == 2 { format!("{}/{}", repo[1], repo[0]) } else { String::new() };
                    Some(Pull { title, repo })
                })
                .collect()
        })
        .unwrap_or_default();
    Some((total, items))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_fixed_and_on_the_api_host() {
        for q in [Query::User, Query::Notifications, Query::ReviewRequests, Query::MyPulls] {
            assert!(q.url().starts_with("https://api.github.com/"), "{q:?}");
        }
    }

    #[test]
    fn tokens_are_checked_for_shape() {
        assert_eq!(clean_token("  ghp_abcdefghijklmnopqrstuvwxyz0123456789 ").as_deref(), Some("ghp_abcdefghijklmnopqrstuvwxyz0123456789"));
        for bad in ["", "curto", "ghp_abc def ghi jkl mno pqr stu", "ghp_\u{e7}abcdefghijklmnopqrstuvwxyz", &"a".repeat(300)] {
            assert_eq!(clean_token(bad), None, "{bad}");
        }
    }

    #[test]
    fn login_and_counts_are_read() {
        assert_eq!(parse_login(r#"{"login":"octo","id":1}"#).as_deref(), Some("octo"));
        assert_eq!(parse_login(r#"{"message":"Bad credentials"}"#), None);
        assert_eq!(parse_notification_count(r#"[{"id":"1"},{"id":"2"}]"#), Some(2));
        assert_eq!(parse_notification_count("[]"), Some(0));
        assert_eq!(parse_notification_count("{}"), None);
    }

    #[test]
    fn search_gives_total_and_first_pulls() {
        let body = r#"{"total_count":5,"items":[{"title":"Corrige  o\nbug","repository_url":"https://api.github.com/repos/octo/hello"},{"title":"B","repository_url":"https://api.github.com/repos/o/r"}]}"#;
        let (n, pulls) = parse_search(body).unwrap();
        assert_eq!(n, 5);
        assert_eq!(pulls[0], Pull { title: "Corrige o bug".into(), repo: "octo/hello".into() });
        assert_eq!(pulls.len(), 2);
        assert_eq!(parse_search("{}"), None);
        assert_eq!(parse_search("lixo"), None);
    }
}
