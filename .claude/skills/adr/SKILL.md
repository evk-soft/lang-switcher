---
name: adr
description: Use when making or changing a decision about architecture, dependencies, platform techniques, or process — records it as an ADR in docs/architecture/adr
---

# Architecture Decision Records

Location: `docs/architecture/adr/NNNN-slug.md`, where `NNNN` is the next number (zero-padded, look at the directory).

Template (docs are in Russian in this repo):

```markdown
# NNNN. <Заголовок — само решение, сформулированное как факт>

Дата: YYYY-MM-DD. Статус: accepted | superseded by NNNN.

## Контекст
<требование, ограничение или проблема, породившие решение>

## Решение
<что именно выбрано, точно и проверяемо>

## Рассмотренные альтернативы
<варианты и почему отклонены — по 1-2 строки>

## Последствия
<что становится проще/сложнее; какие обязательства появились>
```

Rules:

- One decision per ADR.
- Superseding: write a new ADR and mark the old one `superseded by NNNN` — never rewrite an accepted ADR's substance.
- If the decision changes the big picture, update `docs/architecture/overview.md` in the same commit.
- Reference ADRs from code where the code would otherwise look arbitrary: `// see ADR-0004`.
