# POD

> Finally, a name that actually tells you what it is.

Widgets nativos para a área de trabalho do Windows, em Rust: cartões (relógio, calendário, lembretes, timers, métricas do
sistema, música, clima e outros) e notas de texto, que ficam atrás dos seus aplicativos e voltam como estavam depois de
reiniciar. Sem Electron, sem WebView, sem conta e sem telemetria.

[![CI](https://github.com/4TyllaL/Pod/actions/workflows/ci.yml/badge.svg)](https://github.com/4TyllaL/Pod/actions/workflows/ci.yml) [![Release](https://img.shields.io/github/v/release/4TyllaL/Pod)](https://github.com/4TyllaL/Pod/releases/latest) [![Licença: MIT](https://img.shields.io/badge/licen%C3%A7a-MIT-blue)](LICENSE)

**Versão 1.0.0** · Windows 11 x64 · pacote sem assinatura digital.

### [⬇ Baixar a última versão](https://github.com/4TyllaL/Pod/releases/latest)

Baixe o `Pod.exe` e abra (portátil, não precisa instalar) ou o ZIP com instalador. Detalhes em [Instalar](#instalar).

> **Leia antes de usar:** esta V1 **não foi validada para acessibilidade** (leitores de tela como o Narrator, foco de teclado
> nos cartões, método de entrada/IME) e foi testada só em Windows 11, numa máquina de desenvolvimento. A lista completa está
> em [Limitações conhecidas](#limitações-conhecidas).

## O que tem

- **Cartões de grade** em vários tamanhos (1×1 a 4×4, conforme o widget), com arrastar, encaixe na grade e redimensionar
  pelas bordas. Quase 40 tipos: Relógio, Progresso do tempo, Calendário, Contagem regressiva, Lembretes, Água, Conversor,
  Favoritos, Encomendas, Timer, Cronômetro, Pomodoro, Processador, Memória, Disco, Bateria, **Sistema** (anéis), Rede, Música,
  Notificações, Uso do Claude, Hábitos, Feriados, Agenda, Atalhos, Hora no mundo, Downloads, Capturas de tela e Área de
  transferência; e, **só se você ligar "Widgets de internet"**, Clima, Notícias, GitHub e Bolsa.
- **Notas** de texto simples, com posição, tamanho e conteúdo persistidos.
- **Camada de área de trabalho:** os widgets ficam sempre atrás dos aplicativos e não roubam o foco ao clicar.
- **Modo de edição** (bandeja → Editar widgets): galeria na base da tela (por categoria; clique ou arraste para adicionar), selos "−" para remover, espaços "+" tracejados para escolher o lugar e "Restaurar padrão" (pede confirmação; não apaga notas).
- **Temas** claro e escuro (seguem o Windows) e alto contraste; **vidro** translúcido opcional.
- **Persistência segura:** gravação atômica com geração anterior, recuperação automática e arquivo preservado em `recovery`.
- Na primeira execução aparecem 3 widgets (Relógio, Calendário e Sistema).

## Instalar

Baixe o pacote `Pod-1.0.0-win-x64.zip`, extraia e rode, no PowerShell da pasta extraída:

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1 -Launch
```

Instala para o seu usuário (sem administrador) em `%LOCALAPPDATA%\Programs\Pod`, cria o atalho no Menu Iniciar e **não** inicia
com o Windows (use `-Autostart` se quiser). Atualizar: `install.ps1 -CloseRunning`. Desinstalar: `uninstall.ps1` (preserva seus
dados; `-RemoveData` apaga). Detalhes e privacidade no `README.txt` do pacote.

Sem instalar: `Pod.exe --data-dir "D:\MeusDadosDoPod"`.

## Compilar e testar

Requisitos: Windows, o toolchain fixado em `rust-toolchain.toml` (MSVC) e o Windows SDK (o `rc.exe` compila ícone e manifesto).

```powershell
cargo test                                   # suíte completa (app e modelo)
cargo build --release                        # build de produção
powershell -ExecutionPolicy Bypass -File tools\package.ps1   # testes + release determinística + ZIP em dist\
```

Os modos de prova e diagnóstico (`--shadow-*`, `--focus-card`) só existem em builds de **debug**; o executável de distribuição não
os contém. Os scripts em `tools/verify-*.ps1` rodam o app real com dados isolados (`--data-dir`) e **movem o cursor e enviam
teclas de verdade**: não os rode com a sua tela em uso.

## Documentação

- [Arquitetura](docs/ARCHITECTURE.md) (baseline histórico), [decisões](docs/decisions/) e [direção visual](docs/DESIGN.md).
- [Validações e limites de cada marco](docs/validation/): leia os limites antes de considerar um cenário comprovado.
- [Notas da V1](docs/validation/v1-release.md): o que foi verificado, o que não foi e o que fica para depois.

## Limitações conhecidas

- **Acessibilidade não validada:** Narrator e outros leitores de tela, IME e foco de teclado nos cartões (não há como pôr o foco do
  teclado num cartão; os botões respondem ao UI Automation `Invoke`).
- Testado só em Windows 11 na máquina de desenvolvimento: **não** em Windows 10, **não** em máquina virtual limpa, **não** com
  monitores de escalas (DPI) diferentes.
- Pacote **não assinado** (o SmartScreen pode avisar); instalador em PowerShell, não MSI.
- Emoji restaurado de uma nota pode aparecer como caixa; vidro não desfoca o fundo; sombra dos cartões é a curta do Windows
  (a sombra ampla foi estudada e **não** entrou: ver [ADR-0005](docs/decisions/0005-glass-default-and-wide-shadow.md)).
- Fora da V1: chat de IA, cenas/mascote e atualização automática.

## Licença

[MIT](LICENSE). Licenças de terceiros: `THIRD-PARTY-LICENSES.txt` no pacote (gerado por `tools/third-party.ps1`).
