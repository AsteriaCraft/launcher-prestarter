# ADR 0011. Сайт: модалка завантаження, сторінка `/launcher`, `/download` як редирект, визначення системи

- Статус: запропоновано (етап Design, 2026-10-02); переглянуто після рецензії дизайну 2026-10-02 (див. «Ревізія»)
- Репозиторій: `asterium-platform` (гілка `feat/web` → нова `feat/prestarter-download`)
- Вимога власника (2026-10-02): «на завантаження має бути модалка та окрема сторінка з детальною інформацією:
  ченжлоги лаунчера, версія, дата апдейтів, системні вимоги, і автоматом під користувача підбирати тип».
- Пов'язані: [0002](0002-linux-formats-and-default.md), [0010](0010-launchserver-variants-and-static-downloads.md),
  [0012](0012-launcher-changelog-source.md)

## Контекст (`feat/web` f3a7d68)

- Кожне «Завантажити» - посилання `href="/download"` (`native`) у шапці, hero, секції головної, на сторінці серверів
  (`launcher-cta.tsx`), у мобільному меню, у підвалі, у кабінеті (`overview-view.tsx`, `new-player.tsx`), а діалог
  входу на сервер (`server-join.tsx`) веде на `/download?os=<platform>`.
- `apps/web/app/download/route.ts` - route handler поза `[locale]` (proxy його не чіпає: matcher виключає
  `download`): `?os=` або User-Agent → 302 на файл першого завантаження цієї ОС, без релізу - `/#start`.
- Контракт `site/v1/launcher.schema.ts`: одна платформа × `kind` (`installer`/`jar`), без архітектури, sha256,
  нотаток чи вимог; refine «кожна платформа має завантаження». Mock: `Asterium-Setup-<v>.exe` і `.jar` на
  `downloads.example`. HTTP-адаптер кличе `GET /api/site/v1/launcher`, якого API ще не має.
- `detect-platform.ts`: ОС з Client Hints або UA, без архітектури; Windows 11 за `platformVersion ≥ 13`.
- Chrome на будь-якому Linux повідомляє в скороченому UA `X11; Linux x86_64`; Safari і Chrome на Apple Silicon -
  `Intel Mac OS X`; iPadOS Safari - як macOS. Високоентропійні Client Hints (`Sec-CH-UA-Arch`, `-Bitness`) сервер
  отримує лише після `Accept-CH`, а `Critical-CH` змушує Chromium одразу повторити запит з ними.

## Рішення

### URL

- **Сторінка `/launcher`** (uk; `/en/launcher` - en): усе про лаунчер. **`/launcher/changelog`** - уся історія
  версій; постійне посилання на версію - `/launcher/changelog#launcher-3.1.1` (лаунчер) і `#installer-0.3.0`
  (інсталятор).
- **`/download` лишається route handler-ом-редиректом** (302, `Cache-Control: private, no-store`):
  - `?id=<id>` - саме цей файл;
  - `?os=…[&arch=…][&format=…]` - рекомендований файл цієї групи. Кожне наявне посилання `/download?os=windows|macos|linux`
    і далі веде на файл: Windows - x64 exe (ARM64 - за підказками), macOS - DMG, Linux - AppImage для визначеної
    архітектури (x64, якщо не визначено);
  - без параметрів - файл, коли визначення впевнене (`exact` або `compatible`, див. нижче), інакше
    `/launcher#downloads`; телефони і ChromeOS - `/launcher`;
  - відповідь має `Accept-CH: Sec-CH-UA-Arch, Sec-CH-UA-Bitness, Sec-CH-UA-Platform-Version`,
    `Critical-CH: Sec-CH-UA-Arch, Sec-CH-UA-Bitness`, `Vary: Sec-CH-UA-Arch, Sec-CH-UA-Bitness, Sec-CH-UA-Platform, User-Agent`;
  - без релізу - `/launcher` (сторінка пояснює стан), а не `/#start`.
- Кожна кнопка «Завантажити» стає `DownloadTrigger`: посилання на `/launcher` (без JS, середня кнопка миші і
  Ctrl/Cmd-клік відкривають сторінку), а звичайний клік відкриває модалку.

### Визначення системи (`shared/lib/detect-system`, чиста функція + адаптери для браузера і для запиту)

Результат: `{ kind: "desktop", os, arch: "x64" | "arm64" | null, confidence: "exact" | "compatible" | "unsure", label }`
або `{ kind: "mobile" }` / `{ kind: "chromeos" }` / `{ kind: "unsupported", reason: "windows-32bit" | "linux-32bit" | "other-os" }`.

| Що бачимо | Результат |
|---|---|
| macOS (UA або `platform: "macOS"`), не iPad | `macos`, `universal`, `exact` |
| «Mac» і `navigator.maxTouchPoints > 1` (лише в браузері) | `mobile` (iPad) |
| Windows, `architecture: "arm"`, `bitness: "64"` | `windows`, `arm64`, `exact` |
| Windows, `architecture: "x86"`, `bitness: "64"` | `windows`, `x64`, `exact` |
| Windows, `bitness: "32"` | `unsupported: windows-32bit` |
| Windows без підказок (Firefox, Safari) | `windows`, `x64`, `compatible` (x64 працює і на ARM під емуляцією; під кнопкою - «Є збірка для Windows на ARM») |
| Linux з підказками або з нескороченим UA (`x86_64`, `aarch64`/`arm64`) | `linux`, ця архітектура, `exact` |
| Linux `i686`/`armv7l` | `unsupported: linux-32bit` |
| Linux з UA Chrome `Linux x86_64` і без підказок | `linux`, `null`, `unsure` → вибір замість здогадки |
| Android, iOS, `Sec-CH-UA-Mobile: ?1` | `mobile` |
| `CrOS` | `chromeos` |
| інше | `unsupported: other-os` → повний список |

У браузері - `navigator.userAgentData.getHighEntropyValues(["architecture", "bitness", "platformVersion"])`, на
сервері - заголовки `Sec-CH-UA-*`, запасний варіант - UA. Вибір файла - окрема чиста функція
`pickDownload(release, system)` → `{ primary, alternatives, note }`: рекомендований файл групи (ОС, архітектура), для
Linux - AppImage ([0002](0002-linux-formats-and-default.md)).

### Модалка (`features/launcher-download`)

HeroUI Modal на десктопі, нижній Sheet на телефоні; фокус-пастка, Esc, повернення фокусу на тригер.

1. Заголовок «Завантажити Asterium» і рядок «Лаунчер 3.1.1 · оновлено 1 жовтня 2026».
2. **Один головний файл** для визначеної системи: значок ОС, «Windows · x64» / «macOS · Intel і Apple Silicon» /
   «Linux · x64 · AppImage», кнопка «Завантажити для Windows» (`href="/download?id=<id>"`), під нею розмір
   («13,3 МБ»), версія інсталятора і SHA-256 (скорочено `447ce4f9…ca59`, кнопка «Копіювати», повне значення в
   підказці; після копіювання - «Скопійовано»).
3. **Коротке встановлення для цієї ОС** (3 кроки): Windows - SmartScreen («Докладніше → Усе одно запустити»);
   macOS - перетягти в «Програми», перший запуск через «Системні параметри → Приватність і безпека → Усе одно
   відкрити»; Linux AppImage - «Дозволити виконання» або `chmod +x`, один файл - пакети WebKitGTK 4.1. Точні написи
   діалогів Windows і macOS (українська та англійська локалізації ОС) лейн звіряє зі справжніми системами або
   офіційною документацією Microsoft і Apple, а не пише з пам'яті.
4. **«Інша система»** - розгортання зі списком усіх файлів, згрупованих за ОС (радіо-список: ОС, архітектура, формат,
   розмір); вибір змінює головний файл і кроки.
5. Після кліку «Завантажити» модалка переходить у стан «Завантаження почалося»: ті самі кроки крупніше, посилання
   «Не почалося? Завантажити ще раз».
6. Посилання «Усе про лаунчер: вимоги, зміни, перевірка файла →» на `/launcher`.
7. Стани: `unsure` - одразу список вибору з поясненням «Не вдалося визначити архітектуру»; `mobile`/`chromeos` -
   «Лаунчер працює на комп'ютерах з Windows, macOS і Linux» + кнопка «Копіювати посилання» (`https://<сайт>/launcher`);
   `unsupported` - причина і список; немає даних - «Завантаження тимчасово недоступне» з посиланням на сторінку.

### Сторінка `/launcher` (`views/launcher`)

1. **Hero**: назва, версія лаунчера і дата, той самий головний файл, що в модалці, і «Інша система».
2. **Усі завантаження** (`#downloads`): таблиця (ОС, процесор, формат, розмір, SHA-256 з копіюванням, підпис,
   кнопка); на телефоні - картки. Рядок `Asterium.jar` - окремо, «для тих, хто має свою Java 21+ з JavaFX».
3. **Як перевірити файл** (`#verify`): `Get-FileHash -Algorithm SHA256` (PowerShell), `shasum -a 256` (macOS),
   `sha256sum` (Linux); для досвідчених - перевірка `release.json` командою `openssl pkeyutl -verify` з публічним
   ключем; для файлів з вбудованим jar - що перші N байтів дорівнюють підписаному інсталятору (N і sha256 показано).
4. **Системні вимоги** (`#requirements`): вкладки Windows / macOS / Linux - версія ОС, процесор, пам'ять (мінімум і
   рекомендовано), диск, графіка, вебрушій (WebView2 / вбудований / WebKitGTK 4.1 або AppImage), інтернет,
   «Java ставиться автоматично». Значення приходять з API ([розділ 7 дизайну](../crossplatform.md#7-сайт)) і
   показують, з чого виведені (таблиця серверів: версія Minecraft, рекомендована пам'ять, розмір клієнта).
5. **Встановлення** (`#install`): вкладки за ОС, кроки з поясненнями (і для обох форматів Linux).
6. **Що нового** (`#changelog`): три останні версії (лаунчер і інсталятор у спільній стрічці, найновіші вгорі), у
   кожної - дата, мітка «Лаунчер»/«Інсталятор», «Додано» / «Змінено» / «Виправлено», кнопка «Копіювати посилання»;
   «Уся історія →» на `/launcher/changelog`. Повторно використовуються `PageHeader`, `UnderlineTabs`, `ListGroup`,
   `Container`, `LoadedSection` і патерн S06 «поточна версія повністю, ранні - рядками».
7. **Питання** (`#faq`): попередження антивірусів і SmartScreen/Gatekeeper; де лаунчер тримає дані (сховище
   престартера з [0005](0005-jre-acquisition-and-local-store.md); дані лаунчера Gravit: Windows `%APPDATA%\Asterium`,
   Linux `~/.minecraftlauncher/Asterium`, macOS `~/minecraft/Asterium` - за `DirBridge` Gravit, лейн звіряє з
   рантаймом); як видалити повністю; чому завантажується Java; Windows на ARM; AppImage чи один файл; старий
   `GravitLauncherStore`.

### Дані

Контракт `site/v1` розширюється ([розділ 7 дизайну](../crossplatform.md#7-сайт)): матриця файлів з `os`, `arch`,
`format`, `sha256`, розміром у байтах, версією інсталятора і підписом; версії лаунчера й інсталятора з датами;
вимоги; окремий маршрут `GET /launcher/changelog`. API (інший workflow) будує їх з `downloads.json`, підписаного
`release.json` престартера і релізів рантайму; документ для API пише лейн site у `docs/architecture/launcher-downloads.md`.

## Наслідки

- Одна URL `/download` і далі означає «живий реліз», старі посилання працюють, а кнопки на сайті відкривають модалку
  з усім потрібним.
- Сторінка - одне місце для вимог, змін і інструкцій; на неї можна посилатися з Discord і з лаунчера.

## Перевірка (лейн site)

- vitest: таблиця визначення (реальні UA і набори Client Hints для кожного рядка вище), `pickDownload`, контракт
  (схеми, refine: унікальні `id`, один рекомендований на групу, кожна ОС має файл), route handler (усі старі форми
  `?os=`, нові параметри, заголовки `Accept-CH`/`Critical-CH`/`Vary`, телефон → сторінка, немає релізу).
- Playwright: модалка з кожного тригера (шапка, hero, сервери, мобільне меню, підвал, кабінет), клавіатура і фокус,
  axe без порушень, копіювання SHA-256, «Інша система», стан після кліку, телефонний UA, сторінка з усіма розділами,
  якорі змін, `/download?os=windows` → 302 на x64 exe, знімки світлої і темної теми на десктопі й телефоні.

## Ревізія після рецензії (2026-10-02)

Лейн site реалізує рішення вище з такими змінами (вони мають пріоритет над текстом вище там, де розходяться):

1. **Windows на ARM отримує `Asterium.exe` (x64)** ([0004](0004-windows-arm64-uses-x64-jre.md)): рядок «Windows,
   `architecture: "arm"`» дає `windows`, `x64`, `compatible` з підписом «працює на Windows 11 на ARM через емуляцію
   x64»; кнопки «збірка для ARM» і рядка `windows-arm64-exe` у матриці немає, доки `downloads.json` його не містить
   (його не буде до переходу на нативну JRE). `superRefine` «кожна ОС має файл» не вимагає ARM для Windows.
2. **Непідтримувані Windows:** UA `Windows NT 6.` (7, 8, 8.1) і Windows на ARM з `platformVersion` < 13 (Windows 10 on
   ARM, без емуляції x64) - стан `unsupported` з поясненням (Rust ≥ 1.78 і Liberica 25 вимагають Windows 10+; Windows
   10 on ARM не запускає x64-програми).
3. **SmartScreen і Smart App Control** у FAQ і в кроках встановлення Windows: що бачить гравець у SmartScreen
   («Докладніше» → «Усе одно запустити»), і що Smart App Control у режимі enforcement блокує непідписаний файл без
   обходу для окремої програми (лише вимкнути SAC). Точні рядки діалогів - з джерел Microsoft, з посиланням поруч.
4. **Підтримка гри по серверах:** коли API віддає `clientSupport` (з `downloads.json`, [0010](0010-launchserver-variants-and-static-downloads.md)),
   модалка і сторінка позначають для вибраної системи сервери, де гра ще не запускається («лаунчер працює; сервер X
   ще ні»), замість мовчки рекламувати ціль. Контракт v1 отримує необов'язкове поле `clientSupport`; моки - дані з
   `canon.json`.
5. **Linux:** у «голій» системі без GTK 3/libXtst престартер сам називає пакети ([0006](0006-launching-the-launcher.md));
   FAQ показує ту саму команду встановлення, що й вікно престартера (одне джерело: таблиця пакетів у
   `docs/crossplatform.md` розділ 7.2).
