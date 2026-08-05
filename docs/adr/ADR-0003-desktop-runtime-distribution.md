# ADR-0003: Desktop Runtime Distribution

**Статус:** accepted
**Дата:** 2026-08-05

## Контекст

Профиль требует локального filesystem, отдельного browser process, OS locks,
GPU/fonts/network окружения и безопасного хранения ключей. Обычное web
application не может предоставить эти свойства или управлять Camoufox без
локального/удаленного worker.

## Решение

Основной продукт является web application с небольшим устанавливаемым локальным
Profile Bridge:

```text
React web UI -> Cloudflare Rust Worker API
  -> one-time launch intent
    -> profilebridge://claim/<opaque-code>
      -> native Rust Profile Bridge/supervisor
        -> authenticated Cloudflare HTTPS control plane
        -> local cache, materialization, sync and updater
        -> managed IPC to embedded Python 3.12 Camouhost
          -> separate visible Camoufox window
```

Web application показывает каталог, клиентов, пользователей и права, cloud
status, generations, сертификацию и mailbox operations. Browser session
открывается отдельным обычным окном. Встраивание browser surface внутрь WebView
запрещено: оно усложняет изоляцию, lifecycle и fingerprint coherence.

Bridge не поднимает доступный web page localhost HTTP/WebSocket API. Custom URI
содержит только случайный single-use code с TTL 30-60 секунд. Profile ID, JWT,
email, R2 credentials и keys в URI запрещены. Bridge погашает code по HTTPS с
device-bound identity; web UI получает session status от cloud control plane, а
не от local process. Identity perimeter выполняет Cloudflare Access, но Bridge
использует device-bound application protocol и не зависит от browser cookie.

## Runtime Bundle

Runtime поставляется как signed content-addressed artifact и включает:

- embedded Python 3.12, не system Python;
- exact locked Camoufox/BrowserForge/Playwright wheels;
- exact Camoufox browser binary;
- Playwright driver;
- fonts, addons и GeoIP databases;
- protobuf contract descriptor;
- SBOM, licenses, hashes и signature;
- runtime capability manifest.

Bundle устанавливается side-by-side. Новая версия сначала проходит verify и
canary; active profiles не мигрируют автоматически. Rollback сохраняет
предыдущий bundle.

## Packaging

Первичный target: Windows native MSIX/App Installer или signed bootstrap
installer. Регистрация в Microsoft не обязательна; Microsoft Store является
дополнительным distribution channel.
Bootstrap скачивает bundle в application data, проверяет signature/hash и
атомарно активирует его. Для offline deployment существует larger full bundle.

macOS и Linux являются отдельными certified lanes с собственными browser/fonts
artifacts. Один cross-platform archive не считается допустимым runtime.

WSLg остается development/test lane. Для максимальной coherence Windows profile
на Windows должен исполняться Windows-native worker, а не Linux process,
маскирующий Windows fingerprint.

## Multi-Device

Cloud profile доступен на другом компьютере только при выполнении всех условий:

1. устройство авторизовано и зарегистрировано;
2. установлен совместимый signed runtime bundle;
3. получен single-writer lease и fencing token;
4. generation скачан и проверен локально;
5. per-generation DEK разрешено unwrap для этого устройства;
6. network/proxy policy совместима с fingerprint;
7. предыдущий dirty writer отсутствует или разрешен recovery workflow.

Production client не получает постоянный R2 bucket token. Cloud control plane
выдает short-lived scoped credentials или presigned operations. Per-generation
DEK wrap выполняется tenant KEK через `KeyProviderPort` по ADR-0006; device
revocation запрещает новые unwrap.

Текущий smoke использует локальный Secret Vault key и поэтому доказан только на
одном компьютере.

## Forgotten Window

- session heartbeat и activity monitor;
- idle warning с кнопками Continue и Save & Close;
- configurable idle timeout;
- hard maximum session TTL;
- typed drain и graceful browser close;
- OS process ownership и bounded shutdown deadline;
- dirty-local retention при offline/error;
- retry/outbox после восстановления сети;
- crash recovery на clone, без удаления locks вслепую.

Live profile не копируется как обычный каталог. Optional crash-consistent
checkpoint требует atomic filesystem snapshot technology и отдельной policy;
authoritative sync по умолчанию выполняется после graceful close.

## Альтернативный Remote Mode

Позднее web UI может управлять удаленным browser worker и показывать окно через
WebRTC streaming. Это дает доступ без локальной установки, но требует browser
farm, controlled egress IP, GPU capacity, stronger isolation и больших затрат.
Remote mode не заменяет desktop MVP и сертифицируется отдельно.
