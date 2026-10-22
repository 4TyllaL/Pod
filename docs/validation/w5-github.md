# W5 — cofre de segredos e widget GitHub

## O que existe
- `vault.rs`: Gerenciador de Credenciais do Windows (`CredWriteW/CredReadW/CredDeleteW`), alvo `Pod/github-token`.
  Nada de segredo vai para `workspace.json`, logs ou estado de widgets.
- `InputSpec.secret`: a janelinha de entrada vira campo mascarado (`ES_PASSWORD`, até 255). No OK o host valida
  o formato (`github::clean_token`), grava no cofre, apaga o campo e fecha — **o texto nunca vira evento do
  widget**. Formato inválido: a janela continua aberta com o motivo.
- Corretor: `https_get_github` anexa `Authorization: Bearer` **somente** para `api.github.com`; outro host
  (ou `http`, ou `api.github.com.evil.com`) é recusado antes da rede. Hosts permitidos agora: 8.
- Provedor `github.rs`: thread só com opt-in "Widgets de internet" ligado e widget GitHub visível; lê o
  token do cofre a cada ciclo (2 min; falha tenta em 1 min); consultas fixas montadas pelo host
  (`github::Query`: `/user`, `/notifications?per_page=30`, duas buscas de PR com `per_page` pequeno).
- Widget `pod.github` (1×1, 2×1, 2×2): não lidas, PRs para revisar, meus PRs e os 2 primeiros títulos.
  Estados: internet desligada, sem token (botão Conectar), buscando, token recusado (401/403), falha.
  Menu: "Conectar / trocar token…", "Esquecer token". Estado durável vazio.

## Testes (automatizados)
- Modelo: URLs fixas no host da API, formato do token, leitura de login/contagem/busca.
- Cofre: ida e volta real no Credential Manager com alvo de teste próprio (apagado no fim).
- Corretor: token nunca para outro host. Provedor parado não faz pedido nem lê o cofre.
- Widget: assina/cancela, `Conectar` pede campo secreto vazio, snapshot sem segredo, views válidas.
- Galeria: 21 hashes novos (3 formatos × 7 temas).

## Não verificado (honesto)
- **Chamada real à API**: não há token neste ambiente e o agente não digita credenciais. Para validar:
  ligar "Widgets de internet", adicionar o widget, "Conectar" com um token de leitura
  (`notifications`, `repo` leitura) e conferir os números contra github.com.
- Janela de entrada mascarada e Narrator não foram testados ao vivo.
- Escopo mínimo sugerido ao usuário: token fine-grained somente leitura; o Pod nunca escreve no GitHub.
- Termos da API do GitHub e limite de taxa (60/h sem token; 5000/h com) a confirmar antes de distribuir.
