# M7 — Distribuição do MVP

Ambiente: Windows 11 Pro build 26200, x64, usuário sem elevação, toolchain Rust 1.98.1 MSVC fixado em
`rust-toolchain.toml`, Windows SDK 10.0.26100 (`rc.exe`). Todos os testes de instalação usaram pastas e
chaves de registro isoladas (sandbox); o perfil real (`%LOCALAPPDATA%\Pod`, `Programs\Pod`, Menu Iniciar,
Configurações > Aplicativos) não foi tocado.

## O que existe

- **Build release determinística** (`tools/package.ps1`): `--locked`, perfil `release` (opt-level 3, LTO
  total, 1 unidade de codegen, `panic = "abort"`, símbolos removidos), caminhos remapeados, `/Brepro`,
  **CRT estático** (`.cargo/config.toml`) e pasta de build própria.
- **Recursos do executável** (`crates/pod-app/build.rs`, usa o `rc.exe` do SDK; sem dependência nova):
  manifesto (`asInvoker`, Windows 10/11, **DPI Per-Monitor V2**, caminhos longos, UTF-8), ícone
  (`assets/pod.ico`, 7 resoluções, gerado por `tools/make-icon.ps1`) e informações de versão (vêm do
  Cargo: 0.1.0.0, produto "Pod", `Pod.exe`). O ícone também é o da bandeja, com o tooltip "Pod 0.1.0".
- **Pacote** `Pod-<versão>-win-x64.zip` (396 KiB): `Pod.exe`, `install.ps1`, `uninstall.ps1`, `README.txt`
  (pt-BR), `THIRD-PARTY-LICENSES.txt`, `SHA256SUMS.txt`; ZIP com ordem, datas e pasta raiz fixas.
- **Instalador por usuário** (`install.ps1`): `%LOCALAPPDATA%\Programs\Pod`, atalho no Menu Iniciar, entrada
  em Configurações > Aplicativos; substituição atômica do executável; **sem início com o Windows**, só com
  `-Autostart` (a escolha é mantida em atualizações, `-NoAutostart` remove). Atualização manual: recusa
  se o Pod está rodando; com `-CloseRunning` envia `WM_CLOSE` à janela de controle (fecha normalmente, com
  snapshot final), espera até 20 s, nunca mata o processo, e reinicia com a linha de comando de antes.
- **Desinstalador**: remove executável, atalho, início com o Windows e a entrada; **preserva as notas**;
  `-RemoveData` apaga os dados e só aceita uma pasta chamada `Pod`.
- `--data-dir <pasta>` na linha de comando (modo portátil e diagnóstico).
- **Revisão de licenças** (`tools/third-party.ps1`): 23 crates, todos MIT/Apache-2.0/Unicode-3.0/Unlicense,
  textos incluídos; o script falha se aparecer licença fora da lista ou crate sem arquivo de licença.
- Assinatura Authenticode **opcional** (`POD_SIGN_CERT_THUMBPRINT`, `signtool` do SDK): sem certificado o
  pacote sai explicitamente **não assinado** (README e saída do script dizem isso).

## Verificado

| Item | Resultado |
| --- | --- |
| Reprodutibilidade | mesmo `Pod.exe` (SHA-256 `1cca6c15…`) e mesmo ZIP (`cafc5ac6…`) em três builds: pasta de build diferente e **cópia do código-fonte em outro caminho** |
| Dependências do executável | 21 DLLs, **todas do Windows**; **nenhuma** do Visual C++ (`vcruntime140` foi removida com o CRT estático) e **nenhuma de rede** (`ws2_32`, `winhttp`, `wininet`...) |
| Recursos | versão 0.1.0.0, produto/arquivo originais, manifesto lido de volta (`asInvoker`, `PerMonitorV2`), ícone válido |
| Execução do instalado | vivo, **sem elevação**, `Per-Monitor V2 = true` (do manifesto), em caminho com acento e espaço |
| Dados em pasta Unicode | nota "Instalação ✓" salva e relida; texto digitado gravado |
| Atualização com o Pod aberto | sem `-CloseRunning`: recusa e nada muda; com: fecha (código 0, geração 2→3), reinstala, reinicia com os mesmos argumentos, texto preservado |
| Início com o Windows | ausente por padrão; `-Autostart` grava, reinstalar sem parâmetro mantém, `-NoAutostart` remove |
| Desinstalar | remove app, atalho, chave e pasta; notas preservadas; `-RemoveData` apaga; recusa pasta com outro nome |
| Checksums | `SHA256SUMS.txt` confere com os arquivos extraídos |

## Defeitos encontrados e corrigidos neste milestone

- O executável importava `vcruntime140.dll` (não vem com o Windows): numa máquina limpa ele não abriria
  sem o redistribuível do Visual C++. Corrigido com CRT estático.
- Primeiros scripts sem BOM: o PowerShell 5.1 lia os acentos como ANSI (mensagens quebradas). Corrigido.
- O instalador reiniciaria o Pod sem os argumentos originais (`--data-dir`): agora reaproveita a linha de comando.

## Limitações e pendências (não fingir cobertura)

- **Não testado em máquina virtual limpa, nem em Windows 10.** Só nesta máquina de desenvolvimento, com
  Windows 11. A promessa "máquina limpa sem dependências manuais" se apoia na análise de importações e no CRT
  estático, não em uma instalação limpa de verdade. Windows 10 22H2 e ARM64 não foram testados.
- **Sem assinatura Authenticode** (não há certificado): o SmartScreen pode avisar. A assinatura só foi
  escrita, nunca executada.
- **O instalador é um conjunto de scripts PowerShell**, não um MSI/EXE/MSIX: exige rodar
  `powershell -ExecutionPolicy Bypass -File .\install.ps1`. MSIX/Store, atualização automática e uma
  interface gráfica de instalação ficam para depois (ARCHITECTURE §16).
- **Reprodutibilidade só provada nesta máquina e toolchain** (mesmo SDK e mesmo MSVC); outra máquina com
  outro SDK pode gerar outro binário. O ZIP depende da versão do .NET Framework do PowerShell 5.1.
- **Licença do projeto indefinida** (`UNLICENSED` no Cargo.toml): redistribuir o pacote depende de uma
  decisão sua. O pacote não traz arquivo LICENSE do Pod.
- Não testado: atualização entre versões diferentes (só reinstalação da mesma 0.1.0), downgrade pelo
  instalador, usuário padrão de verdade (sem direitos de administrador no grupo), Unicode no nome de usuário
  do perfil real (testei pastas Unicode, não um perfil), instalação com o Pod rodando por outra sessão.
- Não há logs em arquivo por padrão (só com `POD_LOG`), então um diagnóstico pós-instalação depende de
  reproduzir com essa variável.
- O pacote não inclui os arquivos de teste nem o diretório `tools/` além dos scripts de instalação.
- Marcos anteriores seguem com pendências próprias: IME real e Narrator nunca testados, tema real do sistema
  (`tools/verify-theme.ps1`, para você rodar), ícone da bandeja visível não confirmado, emoji restaurado como
  caixa. Nada disso foi tratado no M7.
