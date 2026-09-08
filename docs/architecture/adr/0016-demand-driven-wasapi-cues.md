# 0016. WASAPI открывает поток только на время короткого сигнала

Дата: 2026-09-08. Статус: accepted. Заменяет выбор rodio/cpal и владение аудио
главным потоком в звуковой части ADR-0009; остальные решения ADR-0009 сохраняются.

## Контекст

[Замер](../../research/2026-09-07-idle-performance.md) подтвердил работу
cpal_wasapi_out при sound=false. rodio MixerDeviceSink постоянно генерирует
тишину и не предоставляет публичного управления pause/resume. Требуется
нулевая работа аудиопотока между сигналами, включая sound=true.

## Решение

Заменить rodio/cpal небольшим адаптером WASAPI в switcher-windows. Чистый код
switcher-app синтезирует прежние частоты 660/880/520 Гц в PCM16 mono 44100 Гц,
90 мс, с плавным началом и затуханием. SoundPlayer и конфиг не меняются.

Один STA-поток владеет всеми аудио COM-интерфейсами. Первый STA создаётся на main
до запуска audio/TSF и закрывается после их join. При фатальном таймауте shutdown
его guard намеренно сохраняется до завершения процесса: первый CoUninitialize
не должен опережать ещё работающие дочерние apartments.
В простое MsgWaitForMultipleObjectsEx
блокируется на оконных сообщениях без таймера и без открытого IAudioClient.
Плоские PCM-команды передаются через один заменяемый слот и числовой PumpWaker.
Во время текущего сигнала сохраняется только последняя ожидающая подсказка;
старые команды не образуют очередь устаревших языков. Stop имеет приоритет.
Изменение слота и неблокирующий PostThreadMessage выполняются под одним mutex:
stop не может завершить join и освободить числовой TID до последнего wake.
Перед логированием ошибки и join mutex освобождается.
Контракт жизни TID подтверждён [Microsoft](https://devblogs.microsoft.com/oldnewthing/20140822-00/?p=173):
живой JoinHandle удерживает thread object, но конкурентный sender должен завершить
свою отправку до закрытия этого handle.

При сигнале выбирается текущий default render endpoint, создаётся shared stream
с EVENTCALLBACK, AUTOCONVERTPCM и SRC_DEFAULT_QUALITY. Windows выполняет перевод
формата. Каждый GetBuffer/ReleaseBuffer выполняется на потоке-владельце.
Поток ждёт audio event вместе с оконной очередью STA, продолжает буферизацию и
освобождает stream после отправки всех сэмплов и GetCurrentPadding()==0. В конце
добавляется тишина по GetStreamLatency, чтобы teardown не обрезал аппаратный хвост.
Трёхсекундный watchdog существует только при активном сигнале; stop прерывает wait.

Ошибки дают Sound=Off, не меняя enabled. Следующий запрос заново выбирает endpoint
и может восстановить Sound=Ok. При старте worker проверяет endpoint и формат,
создав stream без Start и сразу освободив его. Это прогревает WASAPI, но не держит
устройство открытым и не обещает успех будущего воспроизведения.

## Альтернативы

- Отключать rodio только при enabled=false: не исправляет enabled idle.
- Патч rodio для pause: сохраняет лишний микшер и требует протокола конца буфера.
- PlaySound/WinMM: проще, но Microsoft рекомендует WASAPI для нового Windows-кода;
  явный поток также позволяет прерывать shutdown и наблюдать ошибки устройства.

## Контракты и проверка

Сверены windows-rs 0.62.2 и Microsoft:
[Initialize](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize),
[render buffer](https://learn.microsoft.com/en-us/windows/win32/coreaudio/rendering-a-stream),
[event handle](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-seteventhandle),
[message-aware wait](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-msgwaitformultipleobjectsex).

Регрессии: PCM-огибающая/частоты/громкость, bounded latest slot и stop, заполнение
частями с ожиданием опустошения, отказ/восстановление, shutdown с живым sender.
Native smoke проверяет старт/конец короткого буфера и простой до/после серии тонов.
Повторяются CPU/RSS/размер бинарника. Аудиосигнал требует отдельного прослушивания.
