# 0013. TSF использует InputProcessorProfileActivationSink на собственном STA-потоке

Дата: 2026-09-07. Статус: accepted.

## Контекст

В ADR-0009 предварительно назван `ITfActiveLanguageProfileNotifySink`. Для M1
нужны обычные RU/EN-раскладки. Этот интерфейс не передаёт ни LANGID, ни HKL,
а контракт `ITfInputProcessorProfileActivationSink::OnActivated` явно описывает
`TF_PROFILETYPE_KEYBOARDLAYOUT`, LANGID/HKL и отдельный флаг активации.

## Решение

Используем `ITfInputProcessorProfileActivationSink`, получая `ITfSource` через
QueryInterface у `ITfThreadMgr`, как предписывает [документация интерфейса](https://learn.microsoft.com/en-us/windows/win32/api/msctf/nn-msctf-itfinputprocessorprofileactivationsink).
Поток сначала успешно выполняет `CoInitializeEx(COINIT_APARTMENTTHREADED)`,
создаёт менеджер, вызывает Activate, подписывает sink и качает сообщения.
Возвраты S_OK и S_FALSE требуют симметричного CoUninitialize;
RPC_E_CHANGED_MODE считается ошибкой и не создаёт RAII-владельца инициализации.

Порядок освобождения: UnadviseSink → Deactivate → освобождение интерфейсов →
CoUninitialize. Ресурсы не покидают STA-поток. Частичный отказ setup освобождает
уже созданные ресурсы в том же порядке. Супервизия — по ADR-0012.
`windows-core` становится прямой зависимостью той же версии, что у `windows`,
поскольку макрос implement генерирует абсолютные ссылки на этот крейт.

У макроса `#[implement]` явно задаётся `Agile = false`: по умолчанию
windows-implement 0.60.2 добавляет IAgileObject и свободнопоточный маршалер,
что несовместимо с Rc/Cell и thread-local обработкой ошибок нашего STA-sink.
Регрессионный QueryInterface-тест проверяет отсутствие IAgileObject.

Деактивации игнорируются. При активации обычной раскладки payload содержит её
HKL; при активации input processor берётся снимок читателя. Payload служит
уведомлением, рантайм всё равно перечитывает foreground по ADR-0011.

Установленная подписка публикует `Degraded/tsf_delivery_unverified`, первый
полезный callback — `Ok/tsf_activation_observed`. Это подтверждает вызов sink,
но не заменяет smoke десяти переключений в **другом** процессе. Документация
не гарантирует в явном виде глобальную доставку всех переключений; риск остаётся
открытым до наблюдения. Постоянный опрос обычных приложений не добавляется.

Контракты: [OnActivated](https://learn.microsoft.com/en-us/windows/win32/api/msctf/nf-msctf-itfinputprocessorprofileactivationsink-onactivated),
[Activate](https://learn.microsoft.com/en-us/windows/win32/api/msctf/nf-msctf-itfthreadmgr-activate),
[Deactivate](https://learn.microsoft.com/en-us/windows/win32/api/msctf/nf-msctf-itfthreadmgr-deactivate),
[CoInitializeEx](https://learn.microsoft.com/en-us/windows/win32/api/combaseapi/nf-combaseapi-coinitializeex).

## Рассмотренные альтернативы

- Оставить ActiveLanguageProfileNotifySink: его контракт не описывает HKL
  обычной раскладки, соответствие M1 потребовало бы дополнительного эксперимента.
- Считать успешный AdviseSink доказательством глобального наблюдения: это разные
  факты, и смешение скрывает основной технический риск.

## Последствия

Выбор sink в карте ADR-0009 уточнён этим решением. Поддержка IME-профилей с
одинаковым HKL остаётся вне RU/EN-контракта M1. Нативный lifecycle тест проверяет
подписку и освобождение, а доставку переключений проверяет отдельный smoke.
