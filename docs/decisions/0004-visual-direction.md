# ADR-0004 — Direção visual centrada no conteúdo

**Data:** 20 de outubro de 2026.  
**Status:** direção Editorial aceita pelo usuário em 20/10/2026 e implementada em Quick Notes; [validação e limites](../validation/editorial-quick-notes.md).  
**Origem:** referências de widgets enviadas pelo usuário, seguidas da autorização para detalhar a direção visual.

## Decisão

Adotar como direção superfícies discretas, conteúdo expressivo e controles integrados, com linguagem comum de tipografia, espaçamento e interação. O usuário escolheu **Quick Notes/Editorial** entre as três composições apresentadas. É o primeiro caso de aplicação: texto maior, mais respiro e cabeçalho mínimo sobre material Papel. A especificação está em [DESIGN.md](../DESIGN.md).

## Consequências

- Não impor o mesmo cabeçalho tingido a todos os widgets.
- Propor 20 DIP de raio para cartões maiores e conservar 14 DIP nos menores; validar por formato antes de alterar tokens globalmente.
- Preservar grade 88/12, resize livre de Notes, editor nativo, estado e persistência existentes.
- Priorizar superfícies opacas; vidro e sombra ampliada são trabalhos separados, com critérios de plataforma.
- Editorial é a direção principal; Papel e Margem ficam como referências de comparação, sem obrigação de construir um seletor de estilos.
- Nenhum efeito visual autoriza ampliar acesso a wallpaper, rede ou sistema.

## Relação com decisões anteriores

Complementa ADR-0001, ADR-0002 e ADR-0003; não revoga contratos do motor, capabilities, input ou política de janelas. A direção visual original da arquitetura histórica torna-se mais específica através de DESIGN.md. A implementação deverá registrar sua validação própria, sem tratar este ADR como teste concluído.
