# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/); versões seguem [SemVer](https://semver.org/lang/pt-BR/).

## [1.0.0] - 2026-10-21

Primeira versão pública.

### Adicionado
- Widgets de desktop nativos (Rust, Direct2D/DirectComposition, sem Electron nem WebView): cerca de 40 tipos de cartão, mais notas de texto, na camada da área de trabalho (sempre atrás dos aplicativos, sem roubar o foco).
- Visual estilo macOS: cartões com material liso, anéis concêntricos (Sistema), previsão por hora, calendário com próximo evento, progresso do ano em 12 segmentos, capa de música tingindo o cartão; temas claro, escuro e alto contraste; vidro translúcido opcional.
- Menu da bandeja próprio (cantos arredondados, ícones, submenus) com **Ajustes**: iniciar com o Windows, modo desempenho (esconde os cartões sob apps em tela cheia), vidro e widgets de internet.
- Modo de edição com galeria na base da tela (categorias, clicar ou arrastar para adicionar), selos de remover, espaços "+" tracejados para escolher o lugar e "Restaurar padrão" com confirmação.
- Primeira execução com 3 widgets (Relógio, Calendário, Sistema).
- Persistência atômica com geração anterior e recuperação; widgets de internet desligados por padrão (opt-in), sem telemetria.
- Pacote reproduzível (`tools/package.ps1`): ZIP com `Pod.exe`, instalador/desinstalador PowerShell, licenças de terceiros e checksums.

### Limitações conhecidas
- Acessibilidade não validada (Narrator, foco de teclado nos cartões, IME).
- Testado só em Windows 11; pacote sem assinatura digital.

[1.0.0]: https://github.com/4TyllaL/Pod/releases/tag/v1.0.0
