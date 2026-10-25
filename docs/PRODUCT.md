# Product Definition

**Статус:** normative product boundary  
**Дата:** 2026-08-27

## 1. Назначение

Browser Profile Platform — самостоятельное приложение для управляемого жизненного
цикла browser profiles: создание и безопасный импорт, назначение клиенту, выдача
доступа пользователю, локальный запуск через Windows Profile Bridge, сохранение
immutable generations, восстановление, сертификация и audit.

Платформа развивается как одно модульное приложение с одним защищённым `main`,
одной архитектурой, одной schema/compatibility lineage и независимым поэтапным
production enablement capabilities:

```text
source_present != production_enabled
```

Наличие mailbox/outbound/другого capability-кода в репозитории не делает его частью
первого production release и не разрешает его backend execution.

Платформа не является CRM и не становится владельцем канонических коммуникаций.
Она проектируется как будущий внешний bounded context Universal CRM со стабильными
contracts/events и заменяемыми adapters.

## 2. Основная Ценность

Owner организации получает контролируемый каталог профилей и может доказуемо
ответить:

- кому разрешено видеть и запускать профиль;
- на каком устройстве и runtime он использовался;
- какое generation является активным;
- был ли profile закрыт и синхронизирован корректно;
- почему профиль quarantined или требует recovery;
- какое evidence подтверждает certification;
- кто изменил assignment, grant, generation или policy.

Member получает только явно выданные profiles и минимально необходимую связанную
client projection.

## 2.1. Принятый продуктовый принцип: Firefox local-first resilience

**Локальная аварийная устойчивость Firefox — базовое свойство приложения;
зашифрованная облачная синхронизация — надстройка, а не предварительное
условие сохранности рабочего локального профиля.**

При сбое Windows, браузера, Bridge или сети исходные файлы локального
`user_data`, включая SQLite/WAL и session recovery, сохраняются без
автоматического удаления, подмены устаревшей облачной копией или
ложного статуса `Saved`. Когда новый запуск допустим по текущим ACL,
устройству, single-writer/fencing и актуальной cloud generation, Bridge
должен дать Firefox восстановить те же рабочие локальные файлы.
Если право на запуск не доказано, данные сохраняются для контролируемого
recovery/conflict, а не уничтожаются и не открываются обходом авторизации.

Обычный обрыв связи не равен доказанному отзыву fence. Приложение
разделяет локальную доступность и подтверждённую облачную синхронизацию;
восстановление после crash/выключения Windows должно быть не хуже нативного
Firefox **при сравнимых разрешённых условиях работы**.
Обязательная отрицательная приёмка — конфликт двух компьютеров,
отзыв ACL, stale fence и сохранение несинхронизированных локальных веток.
Нормативный технический контракт и Windows proof matrix:
[LOCAL_PROFILE_LIFECYCLE.md](LOCAL_PROFILE_LIFECYCLE.md).
Никакое это правило не выдаёт offline-право на запись или дополнительное
Production/staging полномочие.


**Принятое разделение heartbeat и сохранения (2026-10-25).** Heartbeat нужен
только для продления серверного single-writer lease, обнаружения утраты
полномочий и предотвращения записи двумя устройствами. Heartbeat **не**
инициирует snapshot/upload и **не** доказывает сохранность данных. Bridge
наблюдает завершение управляемого процесса Firefox независимо от очередного
heartbeat: после подтверждённого закрытия, освобождения локальных writer locks
и проверки актуальных полномочий он автоматически пытается зашифровать,
загрузить и подтвердить новую cloud generation. `Saved` допустим только
после authoritative verify/CAS/commit; пока этого нет, локальные изменения
остаются `Unsynced`/recovery, а не стираются.

При аварийном завершении Firefox/Bridge/Windows гарантируется сохранение
оригинального рабочего каталога; немедленная облачная отправка при power loss
не обещается. После перезапуска Bridge восстанавливает незавершённый intent
и выполняет **безопасное** продолжение только при свежей авторизации и
отсутствии несовместимого другого writer. Таймаут heartbeat не является
положительным revoke; confirmed revoke/expiry запрещает дальнейшие
привилегированные операции. Упрощение heartbeat не отменяет ACL, fencing,
epoch, CAS, device trust и запрет небезопасной offline-публикации.

## 3. Приоритет Возможностей

1. Governed browser profile lifecycle.
2. Device trust, single-writer coordination и recoverable local state.
3. Encrypted cloud generations и multi-device materialization.
4. Fingerprint/runtime certification.
5. Provider-neutral mailbox operations.
6. CRM integration.

Mailbox Operations является вторичной capability. Предпочтительны Gmail API/OAuth,
Microsoft Graph/OAuth и IMAP/application-password adapters там, где они соответствуют
accepted security/product contracts; browser-assisted mailbox flow допускается только
как управляемый fallback. Платформа не дублирует полные письма без отдельного product,
privacy и retention решения.

## 4. Первый production release — принятый CAP-12 vertical slice

Первый release не обязан ждать завершения всех capability, код которых присутствует
в `main`. Его пользовательская capability конечна:

```text
managed login
-> create/find/view/edit Client
-> create Browser Profile
-> attach Profile to Client
-> see independently authorized attached Profiles in Client card
-> launch through local Windows Profile Bridge and pinned real Camoufox
-> controlled close and authoritative confirmed save
-> reopen the last confirmed state from Client card
-> detach or atomically reassign without deleting either entity
-> logout
```

Принятые product semantics:

- multi-user архитектура: один tenant owner и invited members;
- для acceptance первого release достаточно одной product organization, без special-case
  single-user архитектуры;
- Cloudflare Access / managed external identity владеет credentials и account recovery;
- приложение владеет membership lifecycle, resource authorization и revoke;
- `Client 1 -> 0..N Profiles`, `Profile -> 0..1 Client`;
- assignment не выдаёт ACL; видимость и launch требуют отдельной server authorization;
- любой active member может создать независимый Profile и получает явный creator PROFILE_OPERATOR grant;
- право открытия Profile определяется текущим Profile Operate ACL пользователя, независимо от создателя,
  assignment, автора последнего сохранения и компьютера, на котором Profile использовался раньше;
- зарегистрированный и неотозванный компьютер служит средой исполнения и локальным кэшем, а не владельцем
  Profile; у пользователя может быть несколько одновременно зарегистрированных компьютеров;
- перед Camoufox launch сервер выбирает active VERIFIED generation: соответствующая проверенная чистая
  локальная копия используется без повторного скачивания; отсутствующая или устаревшая материализуется
  из точной серверной версии; локальное время изменения не определяет актуальность;
- несохранённое локальное состояние принадлежит recovery и не перезаписывается молча облачной копией;
- подключение другого компьютера не отзывает предыдущий; отзыв конкретного устройства не отзывает остальные;
- первый вход через браузер и подключение компьютера выполняются один раз; обычные последующие запуски
  используют CNG proof и возобновляемую application session без повторного интерактивного входа;
- обычный путь — создать Profile, нажать Open, работать и закрыть; подготовка первого поколения,
  проверка локальной копии, загрузка и подтверждённое сохранение выполняются приложением;
- один active writer на Profile;
- обычный Profile по умолчанию использует подключение компьютера без обязательных proxy,
  ограничений страны/ASN/типа сети и ручных файлов настройки; дополнительные сетевые ограничения
  принадлежат конкретному Profile и не могут исчезнуть при ошибке чтения или отсутствии измерения;
- локальный workspace не является canonical backup;
- `Saved` существует только после exact verification successor generation и authoritative commit;
- hard profile purge не входит в первый release без отдельного product/legal решения.

### 4.1 Обязательные supporting guarantees

Supporting guarantee является release blocker только если он универсален либо достижим из
выбранного effective capability set. Для принятого slice обязательны:

- tenant membership, grants и fail-closed backend admission до side effects;
- immutable encrypted generations, fencing/CAS и сохранение последней подтверждённой generation
  при partial failure;
- device trust, claim, lease/fence, workspace lock и один shipping Bridge/runtime path;
- immutable Windows/Bridge/runtime identity, trusted distribution, rollback и recovery;
- точные D1/R2/Durable Object/Bridge migration, integrity, backup и recovery evidence для
  реально достижимых данных;
- hosted login/logout/recovery и membership-revocation evidence;
- health/error/incident observation, достаточная для принятого сценария;
- один exact Release Candidate и target-specific Production Authorization envelope.

Certification, audit, updater, notification или recovery infrastructure может быть supporting
implementation detail только в объёме, необходимом перечисленным гарантиям. Это не превращает
соответствующий полный пользовательский feature в scope первого release.

Обычное открытие Profile не требует полной fingerprint-сертификации. Camoufox исполняет браузер
и применяет сохранённую конфигурацию; приложение отвечает за её точность, актуальную generation,
доступ, единственного writer и подтверждённое сохранение. Наблюдаемые различия hardware, экрана/DPR,
WebGL, fonts, locale и speech voices диагностируются без запрета обычного запуска и без обещания
полного совпадения fingerprint. Отсутствие WebGL само по себе допустимо. Неверная browser/OS identity,
повреждённые данные или runtime, отсутствие доступа и потеря writer ownership остаются блокерами.
Приложение не переписывает сохранённый fingerprint под компьютер и не исправляет ограничения Camoufox
собственными подменами браузерных API.

Добавление второй независимой organization/tenant в product UX требует отдельного isolation/product
решения; существующие tenant-safe boundaries при этом не ослабляются.

## 5. Source-present, но production-disabled в первом release

Следующие capability могут существовать, компилироваться и тестироваться в том же `main`, но не входят
в выбранный effective set и не блокируют первый release своей независимой незавершённостью:

- Mailboxes, OAuth mailbox administration и client↔mailbox bindings;
- mailbox jobs, Notifications, Automation и outbound side effects;
- bulk profile operations как отдельный product feature;
- tenant-wide Audit UI, global Sessions UI и Certification UI;
- complex roles, mobile parity, generic export и hard Profile purge;
- new providers и будущая CRM/communications integration.

Изменение принятой product boundary само по себе не изменяет runtime admission. До зависящей
feature/candidate стадии оно обязано пройти explicit profile-impact reconciliation по §13.1
[`APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`](APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md)
и immutable-profile правилам
[`CAP01_CAPABILITY_POLICY_IMPLEMENTATION_CONTRACT.md`](CAP01_CAPABILITY_POLICY_IMPLEMENTATION_CONTRACT.md).
Product wording, наличие исходного кода, UI или binding сами по себе не изменяют effective set.

Никакой `production-lite` ветки, mailbox fork, второй schema lineage или отдельной архитектуры для
будущих capabilities не создаётся. Capability включается только через canonical Capability Policy,
accepted Release Candidate и target-specific backend admission.

## 6. Не Цели

- обещание абсолютной невидимости или гарантированного обхода anti-fraud;
- запуск Camoufox внутри Cloudflare Worker;
- использование R2 как live Firefox filesystem;
- удалённый generic `exec` на устройстве;
- хранение открытых mailbox/proxy credentials;
- автоматическое изменение original legacy profiles;
- скрытая выдача profile access через client assignment;
- прямой доступ к таблицам или profile payload из CRM;
- ожидание завершения mailbox/outbound capability перед первым Production Core release.

## 7. Definition Of Product Success

### 7.1 Первый release

Первый release успешен, когда все принятые CAP-12 B1–B10 доказаны на одном неизменном Release
Candidate, Production target envelope содержит свежие universal/reachable evidence, named authority
выдала GO/PILOT, а deployed candidate совпадает с авторизованным. Пользователь проходит весь сценарий
без CLI; второй writer и unauthorized/replayed launch отвергаются; любая save failure сохраняет
последнюю подтверждённую generation; UI не сообщает ложный `Saved`.

### 7.2 Full platform success

Полный продукт развивается отдельными product decisions: последующие accepted capability profiles
могут добавлять mailbox administration, mailbox jobs/automation, outbound и CRM integration без
переписывания принятой domain/runtime architecture или создания второй истории данных/production-enable
authority.

## 8. Status And Execution Authority

Этот документ владеет стабильным product scope, но не текущим статусом. Binding program живёт в
[`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md), единственный live transaction
pointer — fresh [Issue #266](https://github.com/iamaman11/part-crm-emai-profile/issues/266), а
Production state — в exact candidate decision/evidence его natural owners. `status.json`, Markdown,
старый roadmap или green CI сами по себе не могут повысить readiness или выдать Production Authorization.

## 9. Product/Legal Input Before Distribution

До публичной/коммерческой дистрибуции product owner должен выбрать и добавить совместимую repository
и product license либо явно документировать иной legal режим. Автоматизированный агент не выбирает
license по догадке. Отсутствие решения не расширяет source/runtime scope, но блокирует утверждение о
разрешённой дистрибуции.
