# ADR 0010. LaunchServer: варіанти лаунчера через Prestarter_module, статичні завантаження в `downloads/`, `downloads.json`

- Статус: запропоновано (етап Design, 2026-10-02); переглянуто після рецензії дизайну 2026-10-02 (див. «Ревізія»)
- Репозиторій: `gravit-docker` (модуль AsteriumReleases, entrypoint, compose, nginx, README)
- Пов'язані: [0001](0001-artifact-matrix-and-jar-delivery.md), [0004](0004-windows-arm64-uses-x64-jre.md),
  [0009](0009-ci-matrix-release-assets-and-manifest.md), [0011](0011-site-download-experience.md)

## Контекст (gravit-docker `origin/main` 1e2ca7f, Gravit 5.7.12 fef9bae6, LauncherModules master)

- **Збирання.** `LaunchServer` один раз на старті викликає `collectBinary()` (`LaunchServer.java:304-312`): `JAR`,
  потім подія `LaunchServerLauncherBinaryInit`. `Prestarter_module` читає `config/Prestarter/Config.json`
  (`paths: Map<UpdateVariant,String>`, за замовчуванням `{EXE_WINDOWS_X86_64: "Prestarter.exe"}`, шляхи відносно
  `/app/data`) і додає збирач для кожного варіанта. `PrestarterTask` пише байти престартера, потім jar;
  відсутній файл - `FileNotFoundException`. `buildLauncherBinaries()` збирає все або нічого
  (`LaunchServer.java:314-342`).
- **Оновлення.** `LocalUpdatesProvider` кладе файли в `updates/` під іменами `<binaryName>.jar`, `.exe`,
  `_arm64.exe`, `_linux`, `_linux_arm64`, `_macos`, `_macos_arm64`; `LINUX_X86` і `LINUX_ARM32` - голе
  `<binaryName>` (`:70-102`). На старті хешуються лише `JAR` і `EXE_WINDOWS_X86_64` (`:42-47`); варіант без хешу
  отримує «оновлено» (`:106-108`), з хешем, але без URL - `UpdateInfo(null)`, і лаунчер падає з NPE на `URI.create`
  (`LauncherBackendImpl.java:115-118`). Секрети збирання зберігаються в `build-secrets.json` між рестартами.
- **Gson і помилки в конфігу Prestarter:** одна невідома назва варіанта стає ключем `null` (збирання падає з NPE), дві -
  `JsonSyntaxException: duplicate key: null`, і Gravit перезаписує файл своїм типовим (лише Windows x64)
  (`JsonConfigurableInterface.java:50-53`).
- **AsteriumReleases 2.2.0:** компонент `prestarter` ставить один актив `Prestarter.exe` у `/app/data/Prestarter.exe`.
  Ціль активу не може бути в `updates/`, `config/` та інших каталогах LaunchServer (`ReleasesConfig.targetProblem`).
  Відомі дефекти, що блокують кілька активів:
  - **новий актив на вже встановленому тегу** вважається «підробкою» (`ReleaseInstaller.java:705-725`), ремонт з
    того самого бандла не вдається (`:646-662`), компонент іде в карантин, `Prestarter.exe` зникає, збирання
    стоїть до нового тегу (`ReleaseService.java:261-285`, `:809-820`, `ReleaseInstaller.java:221-233`);
  - **«чи немає збірки лаунчера»** дивиться лише на `JAR` (`GravitLauncherHost.launcherBinaryMissing`), тож нові
    варіанти без зміни компонента не зберуться ніколи;
  - попередження про відсутній `Prestarter_module` знає лише `.exe` (`ReleaseService.java:196-200`).
- **nginx** віддає корінь `updates/` як є (`root /volume/updates`), том підключено лише підшляхом `updates`, лише
  для читання. Файли лаунчера йдуть з origin через Cloudflare (`cf-cache-status: DYNAMIC`).
- **AsteriumCdn** дзеркалить лише підкаталоги `updates/`; файли в корені (і будь-що поза `updates/`) - ні.
- Продакшн: `binaryName` = `Asterium`, `urls.JAR` = `https://launcher.asterium.pro/Asterium.jar`, Authenticode немає
  (`osslsigncode verify` → «No signature found»), `Asterium_linux` та інші - 404.

## Рішення

### 1. Конфіг активів (AsteriumReleases 2.3.0, зворотно сумісний)

Нові необов'язкові поля `ReleasesConfig.Asset`:

| Поле | Значення | Правила |
|---|---|---|
| `launcherVariant` | назва `UpdateVariant`: `EXE_WINDOWS_X86_64`, `EXE_WINDOWS_X86`, `EXE_WINDOWS_ARM64`, `LINUX_X86_64`, `LINUX_ARM64`, `MACOS_X86_64`, `MACOS_ARM64` | лише `kind: FILE`; унікальна в усьому конфігу; `JAR`, `LINUX_X86`, `LINUX_ARM32` (ім'я файла збігається з голим `<binaryName>`) - відмова; ціль не в `downloads/` |
| `download` | `true`: готове завантаження для гравця | лише `FILE`; ціль - `downloads/<назва активу>`; не разом з `launcherVariant` |
| `since` | тег, напр. `v0.3.0` | актив обов'язковий для релізів ≥ `since`; у старіших його немає, і це не помилка |

Приклад для продакшну (оновлені `Config.prod.example.json` і README):

```json
"prestarter": {
  "repo": "AsteriaCraft/launcher-prestarter",
  "assets": [
    { "name": "Prestarter.exe", "target": "Prestarter.exe", "launcherVariant": "EXE_WINDOWS_X86_64" },
    { "name": "Prestarter-windows-aarch64.exe", "target": "prestarter/Prestarter-windows-aarch64.exe", "launcherVariant": "EXE_WINDOWS_ARM64", "since": "v0.3.0" },
    { "name": "Prestarter-linux-x86_64", "target": "prestarter/Prestarter-linux-x86_64", "launcherVariant": "LINUX_X86_64", "since": "v0.3.0" },
    { "name": "Prestarter-linux-aarch64", "target": "prestarter/Prestarter-linux-aarch64", "launcherVariant": "LINUX_ARM64", "since": "v0.3.0" },
    { "name": "Asterium-linux-x86_64.AppImage", "target": "downloads/Asterium-linux-x86_64.AppImage", "download": true, "since": "v0.3.0" },
    { "name": "Asterium-linux-aarch64.AppImage", "target": "downloads/Asterium-linux-aarch64.AppImage", "download": true, "since": "v0.3.0" },
    { "name": "Asterium-macos-universal.dmg", "target": "downloads/Asterium-macos-universal.dmg", "download": true, "since": "v0.3.0" }
  ]
}
```

- Конфіг, у якому жоден актив не має `launcherVariant` чи `download` (сьогоднішній продакшн), працює точно як 2.2.0:
  модуль не пише конфіг Prestarter, не чіпає URL і не пише `downloads.json`.
- **Узгодженість з підписаним маніфестом.** Якщо актив у `release.json` має `os`/`arch`/`role`
  ([0009](0009-ci-matrix-release-assets-and-manifest.md)), модуль звіряє їх з конфігом: `launcherVariant`
  `LINUX_ARM64` вимагає `os: linux`, `arch: aarch64`, `role: prestarter`; `download: true` - `role: download`.
  Розбіжність - відмова встановлення (`BINDING`): помилкова прив'язка arm64 до файла x86_64 не доходить до гравців.

### 2. Дефекти, які виправляються перед усім іншим

- **Доповнення того самого тегу.** Якщо застосовні (за `since`) активи є в конфігу, але їх немає в збереженому
  бандлі або в `state.json`, модуль завантажує саме їх для того самого тегу: `release.json` перевіряється знову
  (підпис, прив'язка), його sha256 має дорівнювати встановленому, кожен актив - розмір, digest GitHub, sha256.
  Це «зміна» (після неї збирання), а не «підробка». Підробкою лишається лише змінений на диску файл, що вже є в стані.
- **`launcherBinaryMissing()`** повертає true, якщо немає файла будь-якого варіанта з `server.launcherBinaries`.
- **Попередження про `Prestarter_module`** перелічує всі активи з `launcherVariant`, яким він потрібен.

### 3. Конфіг Prestarter пише AsteriumReleases

- У `LaunchServerInitPhase` (після `config.init`, до `collectBinary`) модуль пише `config/Prestarter/Config.json`
  атомарно (тимчасовий файл і rename), лише якщо вміст змінився: `paths` - варіант → ціль для кожного встановленого
  і перевіреного активу з `launcherVariant`. Імена варіантів беруться з enum, тож ключ `null` неможливий.
- Набір варіантів фіксується на старті (Gravit збирає `launcherBinaries` один раз). Якщо реліз, встановлений
  вебхуком, додав актив з `launcherVariant`, модуль пише `restart-required` (WARN) і показує це в `mc-releases status`;
  наявні варіанти збираються далі.

### 4. URL і хеш для кожного зібраного варіанта

- Там само в InitPhase: для кожного варіанта з конфігу Prestarter модуль гарантує запис у
  `updatesProvider.urls`. База - `urls.JAR` без `<binaryName>.jar` (продакшн: `https://launcher.asterium.pro/`),
  ім'я - `LocalUpdatesProvider.getUpdate(variant)`: `EXE_WINDOWS_ARM64` → `…/Asterium_arm64.exe`,
  `LINUX_X86_64` → `…/Asterium_linux`, `LINUX_ARM64` → `…/Asterium_linux_arm64`. Наявний запис власника не
  перезаписується; якщо його ім'я файла не збігається з `getUpdate`, це WARN. Зміни зберігаються в
  `LaunchServer.json` засобами Gravit, тож `reload` їх не губить.
- Якщо базу вивести неможливо (`urls.JAR` не закінчується на `<binaryName>.jar`), варіант **не вмикається**
  (ERROR, його немає в конфігу Prestarter): краще без файла, ніж з `UpdateInfo(null)`.
- Після `config.init` модуль викликає `LocalUpdatesProvider.sync(variant)` для кожного варіанта, що має файл в
  `updates/` і запис у `build-secrets.json`. Без цього після кожного рестарту лаунчери не на Windows x64 не
  отримували б оновлень до наступного збирання.
- Файли варіантів, які модуль збирав раніше, а тепер не збирає (відкат, зміна конфігу), видаляються з `updates/`
  після наступного успішного збирання.

### 5. Статичні завантаження і `downloads.json`

- **Каталог `downloads/`** у `/app/data`. Entrypoint створює його (до старту LaunchServer і, через `depends_on`,
  до nginx). nginx отримує другий том лише для читання (підшлях `downloads` → `/volume/downloads`) і
  `location /downloads/` з `alias`, `Cache-Control: no-cache`, `X-Content-Type-Options: nosniff`,
  `Content-Disposition: attachment`, типами `application/x-apple-diskimage` (`.dmg`), `application/vnd.appimage`
  (`.AppImage`), `application/json` (`.json`). Файли лаунчера в корені (`Asterium*.exe`, `Asterium*.jar`,
  `Asterium_linux*`) теж отримують `Cache-Control: no-cache`: з незмінними іменами кеш CDN віддав би старий файл
  і зламав би і sha256 на сайті, і самооновлення.
- `downloads/` належить модулю: файл, якого немає серед поточних активів з `download: true` (і не
  `downloads.json`), видаляється після успішного встановлення або відкату, тож відкат конфігу не лишає на сайті
  застарілих AppImage чи DMG.
- **`downloads/downloads.json`** (schema 1) - що зараз віддає цей LaunchServer. Модуль пише його атомарно після
  кожного успішного збирання, після встановлення або відкату престартера, на старті (після синхронізації хешів) і
  за командою `releases downloads`. sha256 великих файлів кешується за (розмір, mtime).

```json
{
  "schema": 1,
  "generatedAt": "2026-10-02T12:00:00Z",
  "baseUrl": "https://launcher.asterium.pro/",
  "gravit": "5.7.12",
  "runtime": { "tag": "v3.1.1", "manifestSha256": "<64 hex>" },
  "prestarter": { "tag": "v0.3.0", "manifestSha256": "<64 hex>" },
  "jar": { "file": "Asterium.jar", "url": "https://launcher.asterium.pro/Asterium.jar", "size": 8503901, "sha256": "<64 hex>", "builtAt": "…" },
  "files": [
    { "file": "Asterium.exe", "url": "https://launcher.asterium.pro/Asterium.exe", "size": 13315677, "sha256": "<64 hex>",
      "os": "windows", "arch": "x86_64", "format": "exe", "jar": "embedded", "variant": "EXE_WINDOWS_X86_64",
      "prestarterAsset": "Prestarter.exe", "prestarterSize": 4811776, "authenticode": false, "builtAt": "…" },
    { "file": "Asterium-linux-x86_64.AppImage", "url": "https://launcher.asterium.pro/downloads/Asterium-linux-x86_64.AppImage",
      "size": 83368440, "sha256": "<64 hex>", "os": "linux", "arch": "x86_64", "format": "appimage", "jar": "fetched",
      "prestarterAsset": "Asterium-linux-x86_64.AppImage" }
  ]
}
```

- У списку лише файли, що існують зараз; `os`/`arch`/`format` беруться з підписаного `release.json`, а для файлів
  з вбудованим jar - з варіанта. `authenticode: true`, коли завантажено `OSSLSignCode` (тоді хвіст файла не дорівнює
  `Asterium.jar` побайтово, і сайт не робить складену перевірку, [0011](0011-site-download-experience.md)).
- **AsteriumCdn** не змінюється: `downloads/` і корінь `updates/` він не дзеркалить, ці файли завжди йдуть з origin.
  Публікація статичних файлів на R2 - можливе покращення пізніше.

### 6. Що робить самооновлення лаунчера для кожного формату

| Формат | Варіант | Що завантажує оновлення | Куди пише |
|---|---|---|---|
| `Asterium.exe` | `EXE_WINDOWS_X86_64` | `Asterium.exe` | той самий файл |
| `Asterium_arm64.exe` | `EXE_WINDOWS_X86_64` (JRE x64, [0004](0004-windows-arm64-uses-x64-jre.md)) | `Asterium.exe` | той самий файл (стає x64-збіркою) |
| `Asterium_linux` / `_linux_arm64` | `LINUX_X86_64` / `LINUX_ARM64` | той самий варіант | той самий файл, біт виконання лишається |
| AppImage, DMG | `JAR` | `Asterium.jar` | копія в сховищі престартера |
| `Asterium.jar` (своя Java) | `JAR` | `Asterium.jar` | той самий файл |

## Наслідки

- Rollout має порядок: спершу образ з модулем 2.3.0 (зі старим конфігом нічого не змінюється), потім реліз
  престартера 0.3.0 (старий конфіг ставить лише `Prestarter.exe`), потім конфіг і compose, потім сайт.
- `keepVersions: 3` тримає до 4 бандлів престартера по ~200 МБ (два AppImage, DMG, три сирі престартери): ~800 МБ
  на диску з ~700 ГБ вільних; хешування на старті - кілька секунд.
- Відкат престартера на тег, старший за `since` (наприклад v0.2.0), законно вимикає нові варіанти й завантаження
  (WARN), а не ламає встановлення через `MISSING_ASSET`.

## Перевірка (лейн server)

- Модульні тести: валідація `launcherVariant`/`download`/`since` (дублікати, заборонені варіанти, цілі, взаємне
  виключення); узгодженість з метаданими `release.json`; доповнення того самого тегу (новий актив → завантажено
  лише його, без карантину, з повторною перевіркою підпису); запис конфігу Prestarter (атомарність, незмінний
  вміст не переписується, лише встановлені активи); виведення URL (продакшн-форма, власний URL власника, база, яку
  не вивести); синхронізація хешів після «рестарту»; `launcherBinaryMissing` для кожного варіанта; видалення
  застарілих файлів; `downloads.json` (схема, лише наявні файли, кеш sha256); сумісність: старий конфіг → нуль змін.
- e2e (`modules/asterium-releases/e2e/run-e2e.sh`, новий розділ X «cross-platform»): підписаний тестовим ключем
  реліз престартера з сімома активами; кожен `Asterium_*` дорівнює `<сирий престартер> ∥ Asterium.jar` побайтово;
  nginx віддає кожен файл (і `downloads/*` з правильними заголовками); `LauncherSignCheck request` для кожного
  варіанта до і **після рестарту**: актуальний → без оновлення, старіший → точна URL, ніколи `null`; реліз з
  розбіжністю `arch` - відмова; додати актив до конфігу на встановленому тегу → доповнення і збирання; відкат на
  тег до `since` → варіанти вимкнено, збирання зелене; повний образ без кешу.

## Ревізія після рецензії (2026-10-02)

Лейн server реалізує рішення вище з такими змінами (вони мають пріоритет над текстом вище там, де розходяться):

1. **`EXE_WINDOWS_ARM64` не вмикається** ([0004](0004-windows-arm64-uses-x64-jre.md)). Модуль уміє цей варіант
   (валідація, URL, хеш), але `Config.prod.example.json`, README і крок 3 розгортання **не** додають
   `launcherVariant: EXE_WINDOWS_ARM64` для `Prestarter-windows-aarch64.exe`: файл можна встановлювати як звичайний
   актив без варіанта або не встановлювати. Причина: з JRE x64 лаунчер питає `EXE_WINDOWS_X86_64`, дайджест ARM-файла
   ніколи не збігається з `Asterium.exe`, і файл перезаписується x64-збіркою на першому ж запуску
   (`LauncherRequest.java:25-39`, `LocalUpdatesProvider.java:110-113`). Рядок таблиці «`Asterium_arm64.exe`» у
   пункті 6 діє лише після переходу на нативну JRE. e2e фіксує поведінку: `LauncherSignCheck` як
   `EXE_WINDOWS_X86_64` з дайджестом ARM64-файла → точна URL `Asterium.exe`.
2. **Дзеркало підписаної політики обгортки** ([0001](0001-artifact-matrix-and-jar-delivery.md)). Модуль кладе в
   `downloads/` три файли встановленого релізу компонента `prestarter`: `release.json` → `prestarter-release.json`,
   `release.json.sig` → `prestarter-release.json.sig`, актив `prestarter-policy.json` → `prestarter-policy.json`
   (байт у байт, атомарно, після встановлення, відкату і на старті; без активу в релізі - файли видаляються).
   Конфіг: необов'язкове поле компонента `mirrorManifest: true` (або еквівалент, який обере лейн) для `prestarter`;
   nginx віддає їх з тими самими заголовками, що й решту `downloads/` (`application/json`, `no-cache`). AppImage і
   DMG читають `https://launcher.asterium.pro/downloads/prestarter-policy.json` і поруч маніфест з підписом.
3. **Гра на кожній платформі** (`docs/crossplatform.md`, розділ 15). `downloads.json` отримує
   `clientSupport`: для кожного профілю - список `(os, arch)`, для яких є `natives/<os>/<arch>` у каталозі клієнта
   і придатна Java клієнта (`customJavaDownload` або JRE не нижче `minJavaVersion` профілю). Модуль виводить це з
   профілів і каталогу `updates/` під час запису `downloads.json`, лише читаючи файли.
4. **Розгортання з утриманням** (розділ 11): README описує утримання компонента перед злиттям релізу
   (`mc-releases update prestarter v0.2.0`), rc на тестовому LaunchServer з `allowPrerelease=true` і зняття утримання.
5. **Authenticode** (відкрите питання 5): README не обіцяє «увімкнути `OSSLSignCode`, коли буде сертифікат»: з
   2023-06-01 ключ сертифіката підпису коду має бути в HSM, тож шлях - PKCS#11 до хмарного HSM
   (`osslsigncode -pkcs11module`) або jsign з хмарним KMS; підпис з таким ключем доводиться в e2e до купівлі.
