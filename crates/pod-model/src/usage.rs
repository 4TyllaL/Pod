//! Serviço `ClaudeUsage` (ADR-0001, W4): quanto o Claude Code consumiu, lido **localmente** dos
//! transcritos em `~/.claude/projects/**/*.jsonl` (sem rede). Aqui ficam as regras puras: ler uma linha,
//! deduplicar por mensagem, somar por dia local e formatar. O app lê os arquivos de forma incremental.
//!
//! Privacidade: só números de uso e a data de cada resposta são extraídos; o texto das conversas não é
//! guardado nem passa para o widget.

use std::collections::HashMap;

/// Dias mostrados (hoje e os 6 anteriores).
pub const DAYS: usize = 7;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_create: u64,
}

impl Tokens {
    /// O que o cartão chama de "tokens": entrada + saída + escrita de cache (a leitura de cache,
    /// muito maior e muito mais barata, vai à parte).
    pub fn billed(&self) -> u64 {
        self.input + self.output + self.cache_create
    }

    fn add(&mut self, o: &Tokens) {
        self.input += o.input;
        self.output += o.output;
        self.cache_read += o.cache_read;
        self.cache_create += o.cache_create;
    }
}

/// Uma resposta do modelo, já com o dia local em que aconteceu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageEntry {
    /// `message.id` (ou `requestId`): a mesma resposta aparece em várias linhas e conta uma vez.
    pub id: String,
    /// Dias desde 1970-01-01, no fuso local.
    pub day: i64,
    pub tokens: Tokens,
}

/// Dias desde 1970-01-01 (civil, gregoriano). Algoritmo de Howard Hinnant.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// "2026-10-11T16:18:18.925Z" → segundos desde 1970 (UTC). `None` se não for esse formato.
pub fn parse_iso_utc(ts: &str) -> Option<i64> {
    let b = ts.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || (b[10] != b'T' && b[10] != b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let num = |a: usize, z: usize| ts.get(a..z)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, s) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    Some(days_from_civil(y, mo as u32, d as u32) * 86_400 + h * 3_600 + mi * 60 + s)
}

/// Dia local (dias desde 1970) de um instante UTC, dado o deslocamento do fuso em minutos a leste de UTC
/// (Brasil = -180).
pub fn local_day(utc_secs: i64, offset_min: i32) -> i64 {
    (utc_secs + offset_min as i64 * 60).div_euclid(86_400)
}

/// Lê uma linha de transcrito. `None` para tudo que não é uma resposta do assistente com uso.
/// Atalho barato: linhas sem `"usage"` nem são decodificadas.
pub fn parse_line(line: &str, offset_min: i32) -> Option<UsageEntry> {
    if !line.contains("\"usage\"") || !line.contains("\"assistant\"") {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v.get("type")?.as_str()? != "assistant" {
        return None;
    }
    let msg = v.get("message")?;
    let usage = msg.get("usage")?;
    let get = |k: &str| usage.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
    let tokens = Tokens {
        input: get("input_tokens"),
        output: get("output_tokens"),
        cache_read: get("cache_read_input_tokens"),
        cache_create: get("cache_creation_input_tokens"),
    };
    let ts = parse_iso_utc(v.get("timestamp")?.as_str()?)?;
    let id = msg
        .get("id")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("requestId").and_then(|x| x.as_str()))
        .or_else(|| v.get("uuid").and_then(|x| x.as_str()))?
        .to_string();
    Some(UsageEntry { id, day: local_day(ts, offset_min), tokens })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageSnapshot {
    /// Tokens por dia, do mais antigo (há 6 dias) ao de hoje.
    pub days: [Tokens; DAYS],
    /// Respostas de hoje.
    pub messages_today: u32,
    /// Quantos arquivos de transcrito foram lidos (0 = pasta ausente ou vazia).
    pub files: usize,
}

impl UsageSnapshot {
    pub fn empty() -> Self {
        Self { days: [Tokens::default(); DAYS], messages_today: 0, files: 0 }
    }
    pub fn today(&self) -> &Tokens {
        &self.days[DAYS - 1]
    }
    pub fn week(&self) -> Tokens {
        let mut t = Tokens::default();
        self.days.iter().for_each(|d| t.add(d));
        t
    }
    /// Valores do gráfico (um por dia).
    pub fn chart(&self) -> Vec<f32> {
        self.days.iter().map(|d| d.billed() as f32).collect()
    }
}

/// Acumula respostas sem duplicar: uma resposta repetida **substitui** a anterior (a última linha de uma
/// mesma resposta traz o uso final).
#[derive(Debug, Default)]
pub struct Aggregator {
    by_id: HashMap<String, UsageEntry>,
}

impl Aggregator {
    pub fn add(&mut self, e: UsageEntry) {
        self.by_id.insert(e.id.clone(), e);
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Esquece o que é mais velho que a janela e devolve o resumo de `today` (dia local).
    pub fn snapshot(&mut self, today: i64, files: usize) -> UsageSnapshot {
        let oldest = today - (DAYS as i64 - 1);
        self.by_id.retain(|_, e| e.day >= oldest);
        let mut s = UsageSnapshot::empty();
        s.files = files;
        for e in self.by_id.values() {
            if e.day > today {
                continue; // relógio adiantado em algum arquivo: não conta
            }
            let idx = DAYS - 1 - (today - e.day) as usize;
            s.days[idx].add(&e.tokens);
            if e.day == today {
                s.messages_today += 1;
            }
        }
        s
    }
}

/// "1,2 mi", "340 mil", "950", "12 mi" (vírgula decimal; uma casa só abaixo de 10).
pub fn format_tokens(n: u64) -> String {
    let compact = |v: f64, unit: &str| {
        if v < 10.0 {
            format!("{v:.1} {unit}").replace('.', ",")
        } else {
            format!("{} {unit}", v.round() as u64)
        }
    };
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => compact(n as f64 / 1_000.0, "mil").replace(",0 ", " "),
        1_000_000..=999_999_999 => compact(n as f64 / 1_000_000.0, "mi").replace(",0 ", " "),
        _ => compact(n as f64 / 1_000_000_000.0, "bi").replace(",0 ", " "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> i64 {
        days_from_civil(2026, 10, 18)
    }

    fn line(id: &str, ts: &str, out: u64) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","uuid":"u","message":{{"id":"{id}","usage":{{"input_tokens":2,"output_tokens":{out},"cache_read_input_tokens":100,"cache_creation_input_tokens":10}}}}}}"#
        )
    }

    #[test]
    fn civil_dates_and_iso_timestamps() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(2026, 10, 18), 20_744);
        assert_eq!(parse_iso_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso_utc("2026-10-11T16:18:18.925Z"), Some(days_from_civil(2026, 10, 11) * 86_400 + 16 * 3_600 + 18 * 60 + 18));
        assert_eq!(parse_iso_utc("nao e data"), None);
        assert_eq!(parse_iso_utc("2026-13-01T00:00:00Z"), None);
    }

    #[test]
    fn the_local_day_follows_the_time_zone() {
        // 2026-10-18 02:00 UTC ainda é dia 17 em Brasília (UTC-3).
        let t = parse_iso_utc("2026-10-18T02:00:00Z").unwrap();
        assert_eq!(local_day(t, 0), today());
        assert_eq!(local_day(t, -180), today() - 1);
        assert_eq!(local_day(t, 120), today());
    }

    #[test]
    fn only_assistant_lines_with_usage_are_read() {
        let e = parse_line(&line("m1", "2026-10-18T12:00:00Z", 50), 0).unwrap();
        assert_eq!(e.id, "m1");
        assert_eq!(e.tokens, Tokens { input: 2, output: 50, cache_read: 100, cache_create: 10 });
        assert_eq!(e.day, today());
        assert_eq!(parse_line(r#"{"type":"user","message":{"content":"oi"}}"#, 0), None);
        assert_eq!(parse_line("não é json {", 0), None);
        assert_eq!(parse_line(r#"{"type":"assistant","timestamp":"2026-10-18T12:00:00Z","message":{"id":"x"}}"#, 0), None, "sem usage");
        // Sem o id da mensagem, usa requestId.
        let l = r#"{"type":"assistant","timestamp":"2026-10-18T12:00:00Z","requestId":"r9","message":{"usage":{"output_tokens":5}}}"#;
        assert_eq!(parse_line(l, 0).unwrap().id, "r9");
    }

    #[test]
    fn repeated_responses_count_once_and_the_last_one_wins() {
        let mut a = Aggregator::default();
        a.add(parse_line(&line("m1", "2026-10-18T12:00:00Z", 10), 0).unwrap());
        a.add(parse_line(&line("m1", "2026-10-18T12:00:01Z", 40), 0).unwrap());
        a.add(parse_line(&line("m2", "2026-10-18T13:00:00Z", 5), 0).unwrap());
        let s = a.snapshot(today(), 3);
        assert_eq!(s.messages_today, 2);
        assert_eq!(s.today().output, 45, "40 + 5, nao 10 + 40 + 5");
        assert_eq!(s.files, 3);
    }

    #[test]
    fn days_are_placed_oldest_first_and_old_or_future_ones_are_dropped() {
        let mut a = Aggregator::default();
        for (i, day) in [(1, today()), (2, today() - 1), (3, today() - 6), (4, today() - 7), (5, today() + 1)] {
            let ts = format!("2026-10-{:02}T12:00:00Z", 18 + (day - today()));
            a.add(parse_line(&line(&format!("m{i}"), &ts, 100 * i as u64), 0).unwrap());
        }
        let s = a.snapshot(today(), 1);
        assert_eq!(s.today().output, 100);
        assert_eq!(s.days[DAYS - 2].output, 200, "ontem");
        assert_eq!(s.days[0].output, 300, "ha 6 dias");
        assert_eq!(s.week().output, 600, "7 dias; o de ha 7 dias e o do futuro ficam de fora");
        assert_eq!(a.len(), 4, "o mais antigo que a janela foi esquecido");
        assert_eq!(s.chart().len(), DAYS);
    }

    #[test]
    fn billed_tokens_leave_cache_reads_out_and_format_is_compact() {
        let t = Tokens { input: 1, output: 2, cache_read: 1_000_000, cache_create: 4 };
        assert_eq!(t.billed(), 7);
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(950), "950");
        assert_eq!(format_tokens(1_000), "1 mil");
        assert_eq!(format_tokens(1_300), "1,3 mil");
        assert_eq!(format_tokens(340_000), "340 mil");
        assert_eq!(format_tokens(1_200_000), "1,2 mi");
        assert_eq!(format_tokens(12_400_000), "12 mi");
        assert_eq!(format_tokens(3_000_000_000), "3 bi");
    }
}
