# W2 (fechamento) — Drink water, Converter, Favorites e Parcels

## O que entrou

- **Drink water** (`pod.water`): botão "+copo" e "−", anel de progresso, marcas de copos (4 a 10). Meta
  (1,5 a 3,5 L) e tamanho do copo (150 a 500 ml) em submenus. O contador zera sozinho quando o dia muda.
- **Converter** (`pod.converter`): comprimento, massa, temperatura, volume e velocidade; unidades em
  submenus "De"/"Para", "Inverter". O valor é digitado em uma **janelinha de entrada** (um campo, OK/Cancelar);
  o widget valida e, se recusar, a janelinha continua aberta com o motivo. Números com vírgula ou ponto,
  resultado com até 6 algarismos significativos e formatação pt-BR. Sem moedas (exigiriam rede).
- **Favorites** (`pod.favorites`): botões que abrem links salvos no navegador padrão. Painel "Favoritos"
  (título + endereço). **Política de URL:** só `http`/`https` (sem esquema vira `https://`), sem usuário/senha,
  sem espaços, host com ponto; `javascript:`, `file:`, `mailto:`, caminhos do Windows etc. são recusados na
  entrada **e** ao ler o arquivo. O widget só conhece o id do link: o host busca o endereço nos dados
  compartilhados e o valida de novo antes de chamar o sistema.
- **Parcels** (`pod.parcels`): lista de encomendas com caixa "entregue"; painel "Encomendas" (descrição +
  código de rastreio, 8–30 letras/números, normalizado em maiúsculas). **Não consulta nada**: só guarda.
- **Plataforma:** `SharedData` ganhou links e encomendas (arquivos antigos continuam válidos); efeitos
  `OpenUri`, `OpenInput`, `InputError`; o painel de edição virou **guiado por tabela** (4 tipos); quem assina a
  hora agora recebe a hora atual logo ao iniciar (`Command::TimeNow`).
- **Correção:** os comandos devolvidos por `start` eram descartados na abertura da janela; agora são tratados
  (sem isso a assinatura inicial de hora não chegava).
- Bandeja → "Novo widget" lista 14 cartões. Galeria: 204 imagens (68 casos × 3 temas).

## Verificado em janela real (debug; links em modo simulado)

| Ação | Resultado |
|---|---|
| Água: dois cliques em "+250" | `25%`, "500 ml de 2 L"; disco com `drunk_ml:500` e o dia gravado |
| Favoritos: clique em "Docs" | log `link (simulado) id=2 url=https://docs.exemplo.org/a` (o navegador **não** foi aberto no teste) |
| Encomendas: clicar na caixa | "Fone novo, concluída"; disco `delivered:true` |
| Conversor: "Valor…" → "abc" + Enter | janelinha continua aberta: "Número inválido. Exemplos: 12 ou 12,5" |
| Conversor: "25,5" + Enter | fecha; "25,5 km = 15,845 milha"; disco `value:25.5` |
| Painel Favoritos: `javascript:alert(1)` | "Endereço inválido. Use http:// ou https://…"; nada adicionado; os campos ficam |
| Painel Favoritos: `exemplo.net/pagina` | adicionado como `https://exemplo.net/pagina`; widget e disco atualizam |

Testes: 173 no `pod-app` e 99 no `pod-model` (conversões e roundtrips, formatação/leitura de números, URLs
perigosas, códigos, limites, cada widget em todos os formatos e estados).

## Limites

- O abrir de verdade (`ShellExecuteW`) não foi exercitado em teste automático para não abrir o navegador do
  usuário; só o caminho até a decisão de abrir (modo simulado, apenas em builds com asserts de debug).
- O 1×1 do Converter não tem botão (menu apenas); Água e Favoritos têm um botão só no 1×1.
- Conversor: sem moedas, sem área/tempo/dados; unidades fixas.
- Parcels não rastreia nada e Favorites não busca ícone/título do site.
- Teclado: Espaço/Enter aciona o primeiro botão da view (no Favoritos, o primeiro link).
- Não medi custo de CPU dos novos widgets (a água assina a hora por minuto, como o relógio).
- Com isto o W2 do ADR está completo no escopo combinado; Timer/Stopwatch/Pomodoro, Countdown/Calendar/Reminders
  e estes quatro. Itens maiores do ADR (painel de ajustes com campos livres para todos os widgets, relógio
  de parede nos timers) continuam como limites registrados.
