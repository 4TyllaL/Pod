# W4 — widget Notifications (permissão do Windows)

Ambiente: Windows 11 build 26200, 150% de DPI, release com asserts de debug.

## O que existe

- **Serviço `Notifications`** (`pod-model::notifications`, `notif.rs`): `Effect::Notifications(bool)` assina,
  `Event::Notifications(NotifSnapshot)` entrega. O provedor usa `UserNotificationListener` (toasts do Centro de
  Notificações) **numa thread própria**, a cada 3 s, e só enquanto algum widget visível assina.
- **Permissão**: lê o estado com `GetAccessStatus` (não pergunta nada); só se o Windows disser "não
  especificada" chama `RequestAccessAsync` (a única chamada que mostra a pergunta). Estados tratados e
  mostrados no cartão, nunca escondidos: `Allowed`, `Denied` ("Permita em Configurações > Privacidade >
  Notificações") e `Unavailable` (o Windows não libera a API a este modo de execução, por exemplo app sem
  identidade de pacote).
- **Privacidade**: o widget só recebe **aplicativo, título (uma linha, 80 caracteres) e idade**; o corpo da
  mensagem não é lido. Nada é gravado em disco e, ao remover o widget, a lista é esquecida.
- **Widget `pod.notifications`** (1×1, 2×1, 2×2): 1×1 só a contagem; 2×1 contagem e 2 linhas "App: título"; 2×2
  cabeçalho, contagem, divisor e 3 notificações com "App · há 5 min" e o título. Categoria Arquivos, azul.
- Galeria: 21 imagens novas com notificações fictícias (a galeria nunca lê as do usuário).

## Verificado

- 220 testes no `pod-app` e 130 no `pod-model`: ordenação (mais novas primeiro, no máximo 5, total contado), texto
  em uma linha cortado por caracteres, mensagens de cada estado, widget (assina/cancela e **esquece o
  conteúdo**, só redesenha quando a lista muda, "—" em vez de zero sem acesso, dica de permissão no 2×2, todos os
  estados e formatos cabem) e o **provedor real** (devolve um estado e **para de ler depois do stop**).
- **Ao vivo, com notificações reais do Windows desta máquina**: o acesso estava `Allowed` mesmo para este
  executável sem pacote; os três cartões leram `4 notificações` e mostraram a estrutura esperada (2×2: cabeçalho,
  contagem e 3 itens de 2 linhas; 2×1: contagem e 2 itens; 1×1: `4` e `NOTIFICAÇÕES`). Os títulos reais não foram
  impressos nem fotografados.

## Não verificado / limites

- Os estados `Denied` e `Unavailable` só têm teste de regra e de layout; **não alterei a permissão do Windows**
  (configuração do sistema) para vê-los ao vivo. O estado `Unspecified` (pergunta de permissão) também não foi
  exercitado.
- Notificações novas chegam com até 3 s de atraso (intervalo da leitura); não há evento "notificação nova".
- Só leitura: não dá para dispensar nem abrir a notificação a partir do cartão.
- Um aplicativo que não publica título aparece como "(sem título)".
- Durante este trabalho o disco C: ficou 100% cheio e o build falhou; limpei `target/debug/incremental`
  (cache regenerável do projeto, ~3 GB) para liberar espaço. Restam ~3 GB livres: vale liberar mais.
