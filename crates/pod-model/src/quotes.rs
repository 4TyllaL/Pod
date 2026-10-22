//! Cotações (widget Bolsa): três fontes selecionáveis, catálogos fixos de ativos, montagem das URLs
//! (só o host monta, a partir de símbolos do catálogo) e leitura das respostas. Só mostra números
//! atrasados de serviços públicos; nunca aconselha compra ou venda.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    /// AwesomeAPI: moedas e cripto em reais (grátis, sem chave).
    Awesome,
    /// Brapi: ações da B3. Sem token só atende 4 papéis (limite do plano grátis).
    Brapi,
    /// Yahoo Finance (endpoint público sem documentação oficial): B3 e mundo.
    Yahoo,
}

impl Source {
    pub const ALL: [Source; 3] = [Source::Awesome, Source::Brapi, Source::Yahoo];

    pub fn id(self) -> &'static str {
        match self {
            Source::Awesome => "awesome",
            Source::Brapi => "brapi",
            Source::Yahoo => "yahoo",
        }
    }

    pub fn from_id(s: &str) -> Option<Source> {
        Source::ALL.into_iter().find(|x| x.id() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            Source::Awesome => "Moedas e cripto (AwesomeAPI)",
            Source::Brapi => "A\u{e7}\u{f5}es B3 (Brapi)",
            Source::Yahoo => "Mercado mundial (Yahoo Finance)",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Source::Awesome => "AwesomeAPI",
            Source::Brapi => "Brapi",
            Source::Yahoo => "Yahoo",
        }
    }

    /// Ativos escolh\u{ed}veis: (s\u{ed}mbolo da fonte, nome curto no quadro, nome completo).
    pub fn assets(self) -> &'static [(&'static str, &'static str, &'static str)] {
        match self {
            Source::Awesome => &[
                ("USD-BRL", "D\u{f3}lar", "D\u{f3}lar americano"),
                ("EUR-BRL", "Euro", "Euro"),
                ("GBP-BRL", "Libra", "Libra esterlina"),
                ("ARS-BRL", "Peso", "Peso argentino"),
                ("JPY-BRL", "Iene", "Iene japon\u{ea}s"),
                ("BTC-BRL", "BTC", "Bitcoin"),
                ("ETH-BRL", "ETH", "Ethereum"),
                ("SOL-BRL", "SOL", "Solana"),
            ],
            Source::Brapi => &[
                ("PETR4", "PETR4", "Petrobras PN"),
                ("VALE3", "VALE3", "Vale ON"),
                ("ITUB4", "ITUB4", "Ita\u{fa} Unibanco PN"),
                ("MGLU3", "MGLU3", "Magazine Luiza ON"),
            ],
            Source::Yahoo => &[
                ("^BVSP", "IBOV", "Ibovespa"),
                ("PETR4.SA", "PETR4", "Petrobras PN"),
                ("VALE3.SA", "VALE3", "Vale ON"),
                ("ITUB4.SA", "ITUB4", "Ita\u{fa} Unibanco PN"),
                ("BBDC4.SA", "BBDC4", "Bradesco PN"),
                ("ABEV3.SA", "ABEV3", "Ambev ON"),
                ("WEGE3.SA", "WEGE3", "WEG ON"),
                ("AAPL", "AAPL", "Apple"),
                ("MSFT", "MSFT", "Microsoft"),
                ("NVDA", "NVDA", "Nvidia"),
                ("TSLA", "TSLA", "Tesla"),
                ("^GSPC", "S&P500", "S&P 500"),
                ("BTC-USD", "BTCUSD", "Bitcoin em d\u{f3}lares"),
            ],
        }
    }

    pub fn default_symbols(self) -> Vec<String> {
        let n = match self {
            Source::Awesome => 4,
            Source::Brapi => 4,
            Source::Yahoo => 5,
        };
        self.assets().iter().take(n).map(|a| a.0.to_string()).collect()
    }

    pub fn known(self, symbol: &str) -> bool {
        self.assets().iter().any(|a| a.0 == symbol)
    }

    pub fn short_name(self, symbol: &str) -> &'static str {
        self.assets().iter().find(|a| a.0 == symbol).map_or("?", |a| a.1)
    }
}

pub const MAX_SYMBOLS: usize = 8;

/// O que um widget pede: fonte e símbolos (sempre do catálogo da fonte).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QuoteRequest {
    pub source: Source,
    pub symbols: Vec<String>,
}

impl QuoteRequest {
    /// Descarta o que não é do catálogo, repetições e o que passa do limite; sem nada, usa o padrão.
    pub fn clean(source: Source, symbols: &[String]) -> QuoteRequest {
        let mut v: Vec<String> = Vec::new();
        for s in symbols {
            if source.known(s) && !v.contains(s) && v.len() < MAX_SYMBOLS {
                v.push(s.clone());
            }
        }
        if v.is_empty() {
            v = source.default_symbols();
        }
        QuoteRequest { source, symbols: v }
    }

    /// URLs a buscar (uma ou várias). Símbolos fora do catálogo nunca chegam aqui.
    pub fn urls(&self) -> Vec<String> {
        let ok: Vec<&String> = self.symbols.iter().filter(|s| self.source.known(s)).collect();
        match self.source {
            Source::Awesome => {
                if ok.is_empty() {
                    return Vec::new();
                }
                let pairs: Vec<&str> = ok.iter().map(|s| s.as_str()).collect();
                vec![format!("https://economia.awesomeapi.com.br/json/last/{}", pairs.join(","))]
            }
            Source::Brapi => {
                if ok.is_empty() {
                    return Vec::new();
                }
                let t: Vec<&str> = ok.iter().map(|s| s.as_str()).collect();
                vec![format!("https://brapi.dev/api/quote/{}", t.join(","))]
            }
            Source::Yahoo => ok
                .iter()
                .map(|s| format!("https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=1d", crate::net::percent_encode(s)))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Quote {
    /// Nome mostrado no quadro (do catálogo).
    pub label: String,
    pub price: f64,
    /// Variação do dia em %, com sinal.
    pub change_pct: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum QuoteState {
    /// "Widgets de internet" desligado.
    Off,
    Loading,
    Failed(String),
    Ready { source: Source, quotes: Vec<Quote> },
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|x| x.is_finite())
}

/// AwesomeAPI: `{"USDBRL":{"bid":"5.04","pctChange":"-0.47",…},…}` na ordem pedida.
pub fn parse_awesome(body: &str, symbols: &[String]) -> Vec<Quote> {
    let Ok(v) = serde_json::from_str::<Value>(body) else { return Vec::new() };
    symbols
        .iter()
        .filter_map(|sym| {
            let key = sym.replace('-', "");
            let o = v.get(&key)?;
            Some(Quote { label: Source::Awesome.short_name(sym).to_string(), price: num(o.get("bid")?)?, change_pct: num(o.get("pctChange")?)? })
        })
        .collect()
}

/// Brapi: `{"results":[{"symbol":"PETR4","regularMarketPrice":58,"regularMarketChangePercent":-2.82},…}`.
pub fn parse_brapi(body: &str, symbols: &[String]) -> Vec<Quote> {
    let Ok(v) = serde_json::from_str::<Value>(body) else { return Vec::new() };
    let Some(results) = v.get("results").and_then(Value::as_array) else { return Vec::new() };
    symbols
        .iter()
        .filter_map(|sym| {
            let r = results.iter().find(|r| r.get("symbol").and_then(Value::as_str) == Some(sym.as_str()))?;
            Some(Quote {
                label: Source::Brapi.short_name(sym).to_string(),
                price: num(r.get("regularMarketPrice")?)?,
                change_pct: num(r.get("regularMarketChangePercent")?)?,
            })
        })
        .collect()
}

/// Yahoo (`chart`): preço atual e fechamento anterior; a variação é calculada aqui.
pub fn parse_yahoo(body: &str, symbol: &str) -> Option<Quote> {
    let v: Value = serde_json::from_str(body).ok()?;
    let meta = v.get("chart")?.get("result")?.as_array()?.first()?.get("meta")?;
    let price = num(meta.get("regularMarketPrice")?)?;
    let prev = num(meta.get("chartPreviousClose")?).or_else(|| num(meta.get("previousClose")?))?;
    let change = if prev > 0.0 { (price - prev) / prev * 100.0 } else { 0.0 };
    Some(Quote { label: Source::Yahoo.short_name(symbol).to_string(), price, change_pct: change })
}

/// "5,04", "58,00", "413.798" (sem casas a partir de 1.000), com separador de milhar.
pub fn format_price(p: f64) -> String {
    // 4 casas abaixo de 1 (iene, peso), nenhuma a partir de 1.000, senão 2.
    let (dec, scaled) = if p.abs() >= 1000.0 { (0usize, p.round()) } else if p.abs() >= 1.0 || p == 0.0 { (2, p) } else { (4, p) };
    let s = format!("{:.*}", dec, scaled.abs());
    let (int, frac) = s.split_once('.').map_or((s.as_str(), None), |(i, f)| (i, Some(f)));
    let mut out = String::new();
    for (i, c) in int.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    let mut out: String = out.chars().rev().collect();
    if let Some(f) = frac {
        out.push(',');
        out.push_str(f);
    }
    if scaled < 0.0 {
        out.insert(0, '\u{2212}');
    }
    out
}

/// "+2,82%" / "\u{2212}0,48%" (sinal tipográfico), sempre com 2 casas.
pub fn format_pct(p: f64) -> String {
    let r = (p * 100.0).round() / 100.0;
    let s = format!("{:.2}", r.abs()).replace('.', ",");
    if r > 0.0 {
        format!("+{s}%")
    } else if r < 0.0 {
        format!("\u{2212}{s}%")
    } else {
        format!("{s}%")
    }
}

pub fn is_up(q: &Quote) -> bool {
    q.change_pct >= 0.005
}

pub fn is_down(q: &Quote) -> bool {
    q.change_pct <= -0.005
}

#[cfg(test)]
mod tests {
    use super::*;

    fn syms(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn catalogs_are_consistent() {
        for s in Source::ALL {
            let a = s.assets();
            assert!(!a.is_empty() && a.len() >= s.default_symbols().len());
            let mut ids: Vec<&str> = a.iter().map(|x| x.0).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), a.len(), "{s:?} sem repeti\u{e7}\u{e3}o");
            assert_eq!(Source::from_id(s.id()), Some(s));
            assert!(s.default_symbols().iter().all(|d| s.known(d)));
        }
        assert_eq!(Source::from_id("outra"), None);
    }

    #[test]
    fn requests_are_cleaned_and_urls_only_use_catalog_symbols() {
        let r = QuoteRequest::clean(Source::Brapi, &syms(&["PETR4", "ABEV3", "PETR4", "https://evil/x", "VALE3"]));
        assert_eq!(r.symbols, syms(&["PETR4", "VALE3"]));
        assert_eq!(r.urls(), vec!["https://brapi.dev/api/quote/PETR4,VALE3".to_string()]);
        let none = QuoteRequest::clean(Source::Awesome, &syms(&["lixo"]));
        assert_eq!(none.symbols, Source::Awesome.default_symbols());
        let aw = QuoteRequest::clean(Source::Awesome, &syms(&["USD-BRL", "BTC-BRL"]));
        assert_eq!(aw.urls(), vec!["https://economia.awesomeapi.com.br/json/last/USD-BRL,BTC-BRL".to_string()]);
        let y = QuoteRequest::clean(Source::Yahoo, &syms(&["^BVSP", "AAPL"]));
        assert_eq!(
            y.urls(),
            vec![
                "https://query1.finance.yahoo.com/v8/finance/chart/%5EBVSP?interval=1d&range=1d".to_string(),
                "https://query1.finance.yahoo.com/v8/finance/chart/AAPL?interval=1d&range=1d".to_string(),
            ]
        );
        for url in y.urls().into_iter().chain(aw.urls()).chain(r.urls()) {
            assert!(crate::net::url_allowed(&url), "{url}");
        }
        let many: Vec<String> = Source::Yahoo.assets().iter().map(|a| a.0.to_string()).collect();
        assert_eq!(QuoteRequest::clean(Source::Yahoo, &many).symbols.len(), MAX_SYMBOLS);
    }

    #[test]
    fn awesome_brapi_and_yahoo_responses_are_read() {
        let aw = r#"{"USDBRL":{"bid":"5.0402","pctChange":"-0.479808"},"BTCBRL":{"bid":"413798","pctChange":"-0.067"}}"#;
        let q = parse_awesome(aw, &syms(&["USD-BRL", "EUR-BRL", "BTC-BRL"]));
        assert_eq!(q.len(), 2, "o que falta na resposta some");
        assert_eq!((q[0].label.as_str(), q[0].price), ("D\u{f3}lar", 5.0402));
        let br = r#"{"results":[{"symbol":"VALE3","regularMarketPrice":66.36,"regularMarketChangePercent":0.23},{"symbol":"PETR4","regularMarketPrice":58,"regularMarketChangePercent":-2.82}]}"#;
        let q = parse_brapi(br, &syms(&["PETR4", "VALE3"]));
        assert_eq!((q[0].label.as_str(), q[0].price, q[0].change_pct), ("PETR4", 58.0, -2.82), "na ordem pedida");
        assert!(parse_brapi(r#"{"error":true,"message":"Token"}"#, &syms(&["PETR4"])).is_empty());
        let y = r#"{"chart":{"result":[{"meta":{"regularMarketPrice":58.0,"chartPreviousClose":59.68}}]}}"#;
        let q = parse_yahoo(y, "PETR4.SA").unwrap();
        assert!((q.change_pct - -2.8150).abs() < 0.01);
        assert_eq!(q.label, "PETR4");
        assert!(parse_yahoo("{}", "AAPL").is_none() && parse_yahoo("lixo", "AAPL").is_none());
    }

    #[test]
    fn prices_and_percentages_use_brazilian_formatting() {
        assert_eq!(format_price(5.0402), "5,04");
        assert_eq!(format_price(58.0), "58,00");
        assert_eq!(format_price(413_798.4), "413.798");
        assert_eq!(format_price(205_079.25), "205.079");
        assert_eq!(format_price(1234.5), "1.235");
        assert_eq!(format_price(0.0), "0,00");
        assert_eq!(format_price(0.0338), "0,0338");
        assert_eq!(format_price(0.0036), "0,0036");
        assert_eq!(format_pct(-2.82), "\u{2212}2,82%");
        assert_eq!(format_pct(0.23), "+0,23%");
        assert_eq!(format_pct(0.0), "0,00%");
        assert_eq!(format_pct(-0.001), "0,00%");
        assert!(is_up(&Quote { label: String::new(), price: 1.0, change_pct: 0.1 }));
        assert!(is_down(&Quote { label: String::new(), price: 1.0, change_pct: -0.1 }));
        assert!(!is_up(&Quote { label: String::new(), price: 1.0, change_pct: 0.0 }));
    }
}
