//! Busca por texto na galeria "Adicionar widget": comparação sem diferenciar maiúsculas nem
//! acentos do português, por palavras (todas as palavras da busca precisam aparecer).

/// Minúsculas e sem acentos comuns do português ("Relógio" → "relogio").
pub fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            c => c.to_lowercase().next().unwrap_or(c),
        })
        .collect()
}

/// `haystack` (já passado por `fold`) contém todas as palavras de `query`? Busca vazia casa tudo.
pub fn matches(query: &str, haystack: &str) -> bool {
    let q = fold(query);
    q.split_whitespace().all(|w| haystack.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_removes_case_and_accents() {
        assert_eq!(fold("Relógio"), "relogio");
        assert_eq!(fold("ÁGUA e Área de Transferência"), "agua e area de transferencia");
        assert_eq!(fold("Cronômetro"), "cronometro");
        assert_eq!(fold("ação"), "acao");
        assert_eq!(fold("abc 123"), "abc 123");
    }

    #[test]
    fn every_word_must_match_and_empty_matches_all() {
        let h = fold("Área de transferência sistema");
        assert!(matches("", &h) && matches("   ", &h));
        assert!(matches("area", &h) && matches("TRANSFER", &h));
        assert!(matches("area sistema", &h), "palavras em qualquer ordem");
        assert!(!matches("area relogio", &h), "uma palavra ausente recusa");
        assert!(matches("Área", &h), "acento na busca");
    }
}
