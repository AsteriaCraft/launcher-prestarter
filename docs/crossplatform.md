# Престартер Asterium на всіх платформах: дизайн

- **Статус:** етап Design, 2026-10-02; переглянуто після рецензії дизайну того ж дня (розділ 16) перед етапом
  Build. Рішення, змінені рецензією, позначені в ADR розділом «Ревізія».
- **Задача власника (2026-10-02):** «я хочу щоб престартер був кросплатформою, він на таурі».
- **Рішення власника (обов'язкові):** цілі Windows x64 і ARM64, Linux x64 і ARM64, macOS universal; на Linux і один
  файл (бінарник з jar, потребує системного WebKitGTK), і AppImage; macOS поки без підпису, з інструкцією для гравців,
  а підпис Developer ID і нотаризація підготовлені в CI за секретами і вмикаються, коли секрети з'являться.
- **Репозиторії:** `launcher-prestarter` (цей), `gravit-docker` (LaunchServer), `asterium-platform` (сайт).
- **Рішення по суті - в ADR** ([`docs/adr/`](adr/)); цей документ зводить їх в одну картину і містить те, що не є
  окремим рішенням: потоки, контракти, відмови, безпеку, тести, rollout.

## Зміст

0. [Рішення коротко](#0-рішення-коротко)
1. [Вихідна точка](#1-вихідна-точка)
2. [Артефакти](#2-артефакти)
3. [Шлях гравця і самооновлення](#3-шлях-гравця-і-самооновлення)
4. [Престартер: зміни в коді](#4-престартер-зміни-в-коді)
5. [CI і релізи](#5-ci-і-релізи)
6. [LaunchServer](#6-launchserver)
7. [Сайт](#7-сайт)
8. [Відмови і що бачить гравець](#8-відмови-і-що-бачить-гравець)
9. [Безпека](#9-безпека)
10. [Тестування](#10-тестування)
11. [Розгортання і відкат](#11-розгортання-і-відкат)
12. [Відкриті питання власнику](#12-відкриті-питання-власнику)
13. [План робіт (лейни)](#13-план-робіт-лейни)
14. [Докази і джерела](#14-докази-і-джерела)
15. [Гра на кожній платформі](#15-гра-на-кожній-платформі)
16. [Рецензія дизайну: як враховано](#16-рецензія-дизайну-як-враховано)

## 0. Рішення коротко

| Питання | Рішення | ADR |
|---|---|---|
| Як jar лаунчера потрапляє в кожен формат | Windows і один файл Linux - LaunchServer дописує jar до сирого престартера (як сьогодні `Asterium.exe`); AppImage і macOS - статичні збірки CI без jar, престартер завантажує `Asterium.jar` у своє сховище і запускає копію | [0001](adr/0001-artifact-matrix-and-jar-delivery.md) |
| Linux | обидва формати для x86_64 і aarch64; сайт за замовчуванням пропонує AppImage; мінімум glibc 2.34 для одного файла і 2.35 для AppImage (виміряно на етапі Build) | [0002](adr/0002-linux-formats-and-default.md) |
| macOS | universal `.app` у DMG (`dmgbuild`, вікно з інструкцією); ad hoc зараз; Developer ID + нотаризація + staple за п'ятьма секретами в environment `release`; macOS 11+ | [0003](adr/0003-macos-delivery-and-signing.md) |
| Windows ARM64 | JRE x64 під емуляцією (у нативній jre-full немає WebKit JavaFX); тому гравцям на ARM - той самий `Asterium.exe`, нативна ARM-збірка лише будується і тестується, доки не з'явиться нативна JRE з WebKit | [0004](adr/0004-windows-arm64-uses-x64-jre.md) |
| JRE | правильна архітектура API (`arch=arm`), перевірка sha1 і розміру, аварійна таблиця з sha256, атомарне встановлення версіями поруч, перевірка оновлень раз на 7 днів з кнопкою «Грати зараз», власне сховище в `%LOCALAPPDATA%`/XDG/Application Support | [0005](adr/0005-jre-acquisition-and-local-store.md) |
| Запуск лаунчера | передача аргументів, чисте середовище (знімок до `set_var`, без змінних AppImage), від'єднаний процес; перевірки до запуску (jar, JRE, бібліотеки JavaFX на Linux); вбудований режим виходить одразу (самооновлення Gravit пише в той самий файл), режим копії читає журнал обгортки; робота без WebView2 | [0006](adr/0006-launching-the-launcher.md) |
| Стек | Tauri 2.12, без форку tao, rustls, закріплений тулчейн, без зайвих залежностей, бандлер AppImage без мережі | [0007](adr/0007-platform-stack-upgrade.md) |
| Як тестувати релізні байти без продакшну | чотири змінні середовища, адреси лише loopback | [0008](adr/0008-loopback-test-endpoints.md) |
| Що робити, коли обгортка AppImage/DMG застаріє | підписана політика `prestarter-policy.json` (адреса jar, версія Java, мінімальна і найновіша версії обгортки), дзеркало в `downloads/` | [0001](adr/0001-artifact-matrix-and-jar-delivery.md) |
| CI | модель гілок `main` → `release`; `ci.yml` на PR і гілках без секретів і без публікації; `publish.yml` лише на `release`, підпис macOS окремим job з environment `release`; спільні `build.yml` і `smoke.yml`; FX-проба на рідних раннерах; `release.json` schema 1 з метаданими активів; атестація походження | [0009](adr/0009-ci-matrix-release-assets-and-manifest.md) |
| LaunchServer | AsteriumReleases 2.3.0: поля `launcherVariant`/`download`/`since`, виправлення «того самого тегу», конфіг Prestarter, URL і хеші варіантів, `downloads/` і `downloads.json` | [0010](adr/0010-launchserver-variants-and-static-downloads.md) |
| Сайт | модалка з кожного «Завантажити», сторінка `/launcher` і `/launcher/changelog`, `/download` лишається редиректом, визначення ОС і CPU з Client Hints | [0011](adr/0011-site-download-experience.md) |
| Changelog на сайті | підписаний актив `release-notes.json` (uk, en); для старих релізів - розбір опису релізу | [0012](adr/0012-launcher-changelog-source.md) |

## 1. Вихідна точка

Усе нижче виміряно або прочитано в коді на етапі Understand (експерименти - [розділ 14](#14-докази-і-джерела)).

- **Престартер** (`origin/release` 8b9cd54 = v0.2.0, опубліковано 2026-09-29): Tauri 2.8.5 + Svelte. Ставить
  Liberica JRE 25 jre-full у `%APPDATA%\GravitLauncherStore\JRE-25` і запускає
  `javaw -Dlauncher.noJavaCheck=true -jar <свій файл>`. Збирається лише Windows x64, лише на `release`, на Linux
  через cargo-xwin. Тестів немає. Гілка `main` застаріла (e519ba5); нова робота починається від `origin/release`.
- **Репозиторій публічний** (коментарі кажуть «приватний»): ARM і macOS-раннери безкоштовні. `release` не захищена.
  Секрет один - `RELEASE_SIGNING_KEY`.
- **Продакшн:** `https://launcher.asterium.pro/Asterium.exe` = 13 315 677 B: перші 4 811 776 B - це `Prestarter.exe`
  v0.2.0 (sha256 `447ce4f9…ca59`), решта побайтово `Asterium.jar`. Authenticode немає. `Asterium_linux`,
  `Asterium_macos`, `Asterium_arm64.exe` - 404. Cloudflare не кешує файли лаунчера (`DYNAMIC`).
- **Що працює з дописаним jar:** PE (W2), ELF (E2-E5: справжній JRE з Liberica, `java -jar` сам себе). **Що ні:**
  AppImage (`current_exe()` у точці монтування, A2-A3), macOS `.app` (підпис, транслокація).
- **Дефекти престартера** (див. ADR 0004-0007): ARM64 завжди падає на старий аварійний JRE (`arch=aarch64` → 400);
  жодної перевірки JRE; розпакування tar без захисту від `../`; змінні AppImage і `__GL_THREADED_OPTIMIZATIONS=0`
  доходять до Minecraft; `expect` з `panic = "abort"` - мовчазне падіння (без WebView2, без дисплея); троттлінг
  прогресу не працює; вшите «v1.0.0 Alpha»; форк tao на рухомій гілці; залежність npm з назвою `"-"`.
- **Gravit 5.7.12** уже вміє варіанти `EXE_WINDOWS_ARM64`, `LINUX_X86_64`, `LINUX_ARM64` (імена файлів
  `_arm64.exe`, `_linux`, `_linux_arm64`), а `Prestarter_module` збирає будь-який варіант з конфігу. Але після
  рестарту хешуються лише `JAR` і `EXE_WINDOWS_X86_64`, а варіант з хешем без URL ламає лаунчер (NPE).
- **AsteriumReleases 2.2.0** знає один актив престартера і має три дефекти, що блокують кілька активів
  ([ADR 0010](adr/0010-launchserver-variants-and-static-downloads.md)).
- **Сайт** (`feat/web` f3a7d68): `/download?os=` → 302 на файл ОС; контракт без архітектури, sha256, нотаток і вимог;
  API для `GET /launcher` ще немає (сайт на моках).

## 2. Артефакти

| Ціль | Актив GitHub (вхід або готовий файл) | Що завантажує гравець | Jar | Розмір* | Варіант оновлення | Потрібно на машині гравця |
|---|---|---|---|---|---|---|
| Windows x64 | `Prestarter.exe` | `https://launcher.asterium.pro/Asterium.exe` | вбудований | ~17,9 МБ (9 370 624 B + jar) | `EXE_WINDOWS_X86_64` | Windows 10/11 64-bit, WebView2 (у Windows 11 є; без нього - режим без вікна) |
| Windows ARM64 | `Prestarter-windows-aarch64.exe` (лише будується і тестується) | поки `…/Asterium.exe` (x64 під емуляцією); `…/Asterium_arm64.exe` - після переходу на нативну JRE | вбудований | ~16,7 МБ (8 227 840 B + jar) | `EXE_WINDOWS_X86_64` | Windows 11 on ARM (емуляція x64); Windows 10 on ARM не підтримується |
| Linux x64, один файл | `Prestarter-linux-x86_64` | `…/Asterium_linux` | вбудований | ~17,0 МБ (8 443 200 B + jar) | `LINUX_X86_64` | glibc 2.34+, WebKitGTK 4.1, GTK 3, D-Bus, libXtst, ALSA |
| Linux ARM64, один файл | `Prestarter-linux-aarch64` | `…/Asterium_linux_arm64` | вбудований | ~16,3 МБ (7 764 464 B + jar) | `LINUX_ARM64` | те саме |
| Linux x64, AppImage | `Asterium-linux-x86_64.AppImage` | `…/downloads/Asterium-linux-x86_64.AppImage` | копія в сховищі | 82 487 800 B | `JAR` | glibc 2.35+ (бібліотеки WebKitGTK і cairo з Ubuntu 22.04), FUSE (або `--appimage-extract-and-run`), fontconfig, harfbuzz, fribidi, EGL, GLES2; для лаунчера GTK 3, libXtst, ALSA |
| Linux ARM64, AppImage | `Asterium-linux-aarch64.AppImage` | `…/downloads/Asterium-linux-aarch64.AppImage` | копія в сховищі | 80 468 488 B | `JAR` | те саме |
| macOS universal | `Asterium-macos-universal.dmg` | `…/downloads/Asterium-macos-universal.dmg` | копія в сховищі | 7 922 824 B | `JAR` | macOS 11+, Intel або Apple Silicon |
| будь-яка ОС | - | `…/Asterium.jar` | сам jar | ~8,5 МБ | `JAR` | своя Java 21+ з JavaFX |

\* Розміри виміряно в CI етапу Build (запуск 37046721983, коміт 869372e); файли з вбудованим jar = сирий
престартер + jar (~8,5 МБ). Сирий престартер 0.3.0 удвічі більший за 0.2.0 (4,8 МБ): rustls + ring + корені Mozilla,
Tauri 2.12, перевірка jar і розпакування, `panic = "unwind"`. Точні значення пише `release.json` і `downloads.json`.

Імена: `Prestarter-*` - вхід для LaunchServer (не для гравців), `Asterium-*` - готове завантаження. Архітектури в
іменах файлів - як у Rust і Liberica (`x86_64`, `aarch64`, `universal`); сайт показує гравцю `x64`, `ARM64`.

## 3. Шлях гравця і самооновлення

**Перший запуск, вбудований jar** (Windows, один файл Linux):
1. Гравець запускає `Asterium.exe` / `Asterium_linux` (Linux: після `chmod +x`).
2. Престартер не знаходить готового JRE у сховищі → показує вікно → API Liberica → завантаження з перевіркою →
   атомарне встановлення ([ADR 0005](adr/0005-jre-acquisition-and-local-store.md)).
3. `java -jar <свій файл> [аргументи]` з чистим середовищем, від'єднано; до 4 с чекає ранньої помилки
   ([ADR 0006](adr/0006-launching-the-launcher.md)); виходить.

**Перший запуск, копія jar** (AppImage, macOS): ті самі кроки, плюс між 2 і 3 - завантаження
`https://launcher.asterium.pro/Asterium.jar` у `<сховище>/launcher/Asterium.jar` (HTTPS, перевірка zip і
`Main-Class`, атомарний запис).

**Наступні запуски:** JRE готовий і перевірка оновлень JRE була менше 7 днів тому → одразу `java -jar`, без вікна і
без мережі. Перевірка прострочена → один запит до API (3 с): є новіша JRE → вікно, встановлення, запуск; немає мережі
→ запуск з наявним JRE.

**Самооновлення лаунчера** (без змін у Gravit):

| Формат | Що питає лаунчер | Що завантажує | Куди пише |
|---|---|---|---|
| `Asterium.exe` | `EXE_WINDOWS_X86_64` | `Asterium.exe` | той самий файл |
| `Asterium_arm64.exe` (поки не роздається) | `EXE_WINDOWS_X86_64` (JVM x64) | `Asterium.exe` | той самий файл уже на першому запуску; тому ARM-пристрої отримують `Asterium.exe` ([ADR 0004](adr/0004-windows-arm64-uses-x64-jre.md)) |
| `Asterium_linux`, `Asterium_linux_arm64` | `LINUX_X86_64`, `LINUX_ARM64` | той самий варіант | той самий файл (біт виконання лишається) |
| AppImage, DMG | `JAR` (файл закінчується на `.jar`) | `Asterium.jar` | копія в сховищі |
| `Asterium.jar` | `JAR` | `Asterium.jar` | той самий файл |

**Новий престартер доходить до гравця:** Windows і один файл Linux - разом з наступним збиранням лаунчера (як
сьогодні); AppImage і DMG - лише новим завантаженням із сайту (лаунчер оновлює jar, а не обгортку). Тому обгортка
має бути стабільною з першого релізу, а те, що може застаріти, вона бере з підписаної політики
(`prestarter-policy.json`, [ADR 0001](adr/0001-artifact-matrix-and-jar-delivery.md)): адресу jar, версію Java, і
дізнається, що є новіша обгортка (ненав'язливе повідомлення) або що її версія нижча за мінімальну (запуск
відмовлено з посиланням на сайт).

**Сумісність із сьогоднішніми установками.** Гравці з `Asterium.exe` отримають престартер 0.3.0 з наступним збиранням
лаунчера; він ставить JRE у нове сховище (одноразово ~120 МБ) і не чіпає `GravitLauncherStore`. `Asterium.exe` і
`Asterium.jar` лишаються за тими самими адресами (на них посилається старий сайт). Продакшн-конфіг AsteriumReleases з
одним `Prestarter.exe` працює і з новими релізами: зайві активи ігноруються, ім'я `Prestarter.exe` не змінюється.

## 4. Престартер: зміни в коді

### 4.1 Структура

Правило власника: кожен файл у каталозі своєї ролі, великий модуль - на підмодулі.

```
src-tauri/src/
  main.rs                 лише виклик lib::run і код виходу
  lib.rs                  знімок середовища, перевизначення, сховище і журнал, план -> швидкий шлях або вікно
  app/                    єдиний модуль, що знає Tauri: commands.rs, events.rs (троттлінг 100 мс), worker.rs,
                          headless.rs (режим без вікна), mod.rs (вікно з конфігу, create: false)
  flow/                   session.rs (один запуск: plan -> run -> launch), error.rs (коди виходу), report.rs
  jre/                    catalog.rs, api.rs, version.rs, fallback.rs + fallback.json, extract/{zip,tar,guard}.rs,
                          layout.rs (java -version), install.rs (staging, rename, GC)
  jar/                    inspect.rs (EOCD/CEN/MANIFEST, хвіст Authenticode), source.rs, fetch.rs
  launch/                 command.rs, environment.rs, process.rs, outcome.rs, libs.rs (ldd і пакети)
  policy/                 model.rs, verify.rs (Ed25519 release.json), mod.rs (вердикт, anti-rollback, fetch)
  store/                  paths.rs, state.rs (state.json schema 1), lock.rs, logs.rs, atomic.rs
  net/                    client.rs (rustls: сховище ОС для Liberica, корені Mozilla для хоста лаунчера),
                          download.rs (sha1 + sha256 потоком), overrides.rs
  platform/               host.rs, windows.rs (MessageBoxW, емуляція x64, вільне місце), unix.rs (setsid,
                          statvfs), macos.rs (карантин, транслокація, іконка для Dock)
  i18n/                   mod.rs + messages/{be,en,pl,ru,uk}.json (їх же імпортує фронтенд)
src-tauri/tests/          інтеграційні тести без GUI: flow.rs, https.rs (тестовий CA), binary.rs (справжній
                          бінарник); support/; examples/fake_java.rs - замінник java
src/lib/                  components/, config/, i18n/, types/, utils/ (фронтенд Svelte 5, редуктор стану з тестами)
tests/fixtures/           hello/ (Hello.java) і fxprobe/ (обгортка як у Gravit + вікно JavaFX/WebView) для smoke
```

Ядро (`flow`, `jre`, `jar`, `launch`, `policy`, `store`, `net`, `platform`, `i18n`) не залежить від Tauri (це перевіряє
`ci.yml`): його тестують інтеграційні тести і використовує і вікно, і швидкий шлях, і режим без WebView.

### 4.2 Алгоритм запуску

```
main:
  env_snapshot = знімок середовища (до будь-якого set_var)                     ADR 0006
  overrides = net::overrides::read()  (помилка → вихід з кодом 2)              ADR 0008
  store = store::open()  (шляхи, tmp/ чиститься, журнал запуску)               ADR 0005
  jar_source = jar::embedded::detect(current_exe) ? Embedded(path) : Copy(store/launcher/Asterium.jar)
  Copy: policy = збережена підписана політика; прострочена (24 год) → оновити (3 с)    ADR 0001
        версія нижче minWrapperVersion → вікно «завантажте нову версію», вихід 7
  якщо JRE готовий і перевірка JRE свіжа і (Embedded або копія jar валідна) і немає повідомлення політики:
      preflight (jar, JRE, бібліотеки JavaFX на Linux) → launch
        Embedded: вихід 0 одразу після spawn
        Copy: чекати обгортку (до 10 с), розібрати launcher-start.log → успіх / повтор / вікно помилки
  інакше:
      спробувати вікно Tauri (приховане до першого кадру)
        → не вдалося (немає WebView2 / WebKitGTK): режим без вікна (Windows: MessageBoxW)  ADR 0006
      у фоні: [перевірка або встановлення JRE, «Грати зараз»] → [копія jar, якщо треба] → preflight → launch → done/error
```

Події для фронтенду: `stage` (`jre-check`, `jre-download`, `jre-install`, `jre-verify`, `jar-download`, `launching`), `progress`
(байти, не частіше ніж раз на 100 мс - виправлений троттлінг), `error` (код помилки + параметри для i18n + чи
можливий повтор), `done`.

### 4.3 Особливості ОС

- **Windows (x64, ARM64):** `javaw.exe`; `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`; ARM64 бере JRE x64; без
  WebView2 - `MessageBoxW` і встановлення без вікна; ресурс версії exe з `tauri.conf.json`; сховище в
  `%LOCALAPPDATA%\Asterium\Prestarter`; шляхи з кирилицею перевіряються в CI.
- **Linux (один файл, AppImage):** `setsid`; під AppImage (`APPIMAGE` + `APPDIR`) - копія jar і чищення середовища;
  `__GL_THREADED_OPTIMIZATIONS`/`__NV_DISABLE_EXPLICIT_SYNC` лише для самого престартера; повідомлення про
  відсутні бібліотеки JavaFX з назвами пакетів; сховище `${XDG_DATA_HOME:-~/.local/share}/asterium/prestarter`.
- **macOS:** `JDK_JAVA_OPTIONS` з `-Xdock:name=Asterium -Xdock:icon=…` у середовищі дочірнього процесу (обгортці
  `-Xdock` нічого не дає, ADR 0006); копія jar; зняття `com.apple.quarantine` з власних
  файлів; підказка «перетягніть у Програми» з `/Volumes/…` або `…/AppTranslocation/…`; сховище
  `~/Library/Application Support/Asterium/Prestarter`; кожен зріз universal-бінарника бере JRE своєї архітектури.

### 4.4 Інтерфейс

- Розмір і стиль вікна без змін (512×300, без рамки, прозоре); вікно показується, коли сторінка відмалювала DOM і
  завантажила шрифти й тло першого кадру (не більше 1,5 с). Не `requestAnimationFrame`: WebKit (Linux, macOS) у
  прихованому вікні його не викликає - етап Build це показав (smoke: «the page did not report ready in 15 s»).
- Стани українською (і be, en, pl, ru за мовою ОС): «Перевіряємо Java», «Завантажуємо Java · 45 % · 12,3 МБ/с»,
  «Встановлюємо Java», «Завантажуємо лаунчер», «Запускаємо Asterium».
- Помилка - речення про причину, дія («Спробувати ще», «Відкрити журнали»), код помилки дрібно (для підтримки).
- Версія внизу - з `getVersion()`; мітка «тестовий режим», коли діє перевизначення ([ADR 0008](adr/0008-loopback-test-endpoints.md)).

### 4.5 `tauri.conf.json`

```jsonc
{
  "productName": "Asterium",                       // було "prestarter": ім'я .app, AppImage, заголовок
  "version": "0.3.0",
  "identifier": "pro.asterium.prestarter",         // було "com.asterium.prestarter"
  "app": {
    "macOSPrivateApi": true,                       // прозоре вікно на macOS (feature tauri/macos-private-api)
    "windows": [{ "label": "main", "create": false, "title": "Asterium", "visible": false, /* решта як була */ }],
    "security": { "csp": "default-src 'self'; connect-src ipc: http://ipc.localhost; img-src 'self' data:; style-src 'self' 'unsafe-inline'; font-src 'self' data:" }
  },
  "bundle": {
    "active": false,                               // збірка бандлів - лише явним --bundles у скриптах CI
    "targets": ["appimage", "app"],
    "category": "Game",
    "macOS": { "minimumSystemVersion": "11.0", "signingIdentity": "-" },
    "linux": { "appimage": { "bundleMediaFramework": false } }
  }
}
```

`create: false`: вікно створює код (`app/mod.rs`), тож помилку WebView2/WebKitGTK можна зловити і перейти в режим без
вікна, а не впасти в `expect`. Вікно приховане, доки фронтенд не відмалює DOM, не завантажить шрифти й тло і не
викличе `ready` (журнал: «the page is ready after N ms; showing the window»). `connect-src` дозволяє IPC Tauri
(`ipc://localhost`, на Windows `http://ipc.localhost`); без нього кожен виклик спершу падав і йшов через
`postMessage` (виявлено на етапі Build).

Ім'я виконуваного файла після зміни `productName` скрипти збирання беруть з `mainBinaryName` або імені bin cargo і
перевіряють; активи в `dist/` завжди отримують імена з [розділу 2](#2-артефакти).

## 5. CI і релізи

Повний опис і причини - [ADR 0009](adr/0009-ci-matrix-release-assets-and-manifest.md). Коротко:

- `ci.yml` (PR і push будь-якої гілки крім `release`, `contents: read`, без секретів): `cargo fmt --check`,
  `cargo clippy -D warnings` (хост і цілі Windows через `cargo xwin clippy`), `cargo test` на `ubuntu-22.04`,
  `ubuntu-22.04-arm`, `windows-2025`, `windows-11-arm`, `macos-15`; `svelte-check` і vitest; `cargo deny`;
  `shellcheck`, `actionlint`; збирання 7 артефактів; smoke; для PR у `release` - перевірка версії, дати в
  `CHANGELOG.md` і `release-notes/<версія>.json`.
- `publish.yml` (лише `release`): версія → збирання без кешів → підпис macOS окремим job з environment `release` (якщо є секрети) → smoke на тих
  самих байтах → `SHA256SUMS.txt`, `release.json` (schema 1, метадані активів), підпис Ed25519 → атестація
  походження → draft → публікація. Нічого не публікується, якщо будь-який крок упав.
- `jre-watch.yml`: щотижня оновлює аварійну таблицю JRE окремим PR і стежить за WebKit у Windows aarch64.

**Підписи кожного артефакту:**

| Артефакт | Що його захищає |
|---|---|
| усі активи релізу | sha256 у `release.json`, підписаному Ed25519 (`RELEASE_SIGNING_KEY`; публічний ключ `.github/release-signing.pub.pem` і `signingPublicKeys` AsteriumReleases); атестація походження GitHub, поки репозиторій публічний |
| `Asterium.app` / DMG | ad hoc зараз; Developer ID + нотаризація + staple, коли є `APPLE_*` ([ADR 0003](adr/0003-macos-delivery-and-signing.md)) |
| `Asterium*.exe` (з jar) | jar підписано ключем Gravit; Authenticode - модулем `OSSLSignCode` на LaunchServer, коли буде сертифікат (відкрите питання) |
| jar у будь-якому файлі | підпис Gravit (`SIGNUMO.RSA`), перевірка оновлення Gravit (SHA-512 + secure hash) |

## 6. LaunchServer

Повний опис - [ADR 0010](adr/0010-launchserver-variants-and-static-downloads.md). Коротко, що змінюється в
`gravit-docker`:

1. **AsteriumReleases 2.3.0** - поля активу `launcherVariant`, `download`, `since`; перевірка проти метаданих
   підписаного `release.json`; виправлення трьох дефектів (доповнення того самого тегу, `launcherBinaryMissing` для
   всіх варіантів, попередження про `Prestarter_module`).
2. **Конфіг `Prestarter_module`** пише AsteriumReleases у InitPhase (атомарно, лише встановлені й перевірені файли).
   Gravit сам дописує jar до кожного сирого престартера під час свого збирання.
3. **URL і хеші** кожного зібраного варіанта: URL виводиться з `urls.JAR` і зберігається в `LaunchServer.json`;
   варіант без URL не вмикається; після рестарту `sync(variant)` для кожного варіанта.
4. **`downloads/`** - новий каталог для AppImage і DMG; nginx віддає його другим томом лише для читання з
   `Cache-Control: no-cache`; ті самі заголовки для `Asterium*` у корені.
5. **`downloads/downloads.json`** - що віддає сервер зараз (файли, sha256, розміри, варіанти, теги рантайму і
   престартера, межа престартер/jar у складених файлах). Його читає API сайту.
6. **AsteriumCdn** - без змін (файли лаунчера і `downloads/` завжди з origin).
7. **README** (українською): розділ «Престартер для всіх платформ», виправити «репозиторій приватний» і
   «Authenticode» (у продакшні його немає), приклади конфігу.

## 7. Сайт

UX, URL і визначення системи - [ADR 0011](adr/0011-site-download-experience.md); джерело changelog -
[ADR 0012](adr/0012-launcher-changelog-source.md). Тут - контракт, вимоги і те, як API будує дані.

### 7.1 Контракт `packages/contracts/src/site/v1`

Контракт v1 ще не реалізований жодним API (сайт на моках), тож він змінюється на місці; `LauncherPackageKind`
прибирається, `LauncherPlatform` лишається.

```ts
// site-v1.enum.ts
export const LauncherArch = { X64: "x64", Arm64: "arm64", Universal: "universal" } as const;
export const LauncherFormat = { Exe: "exe", Binary: "binary", AppImage: "appimage", Dmg: "dmg" } as const;
export const LauncherJarDelivery = { Embedded: "embedded", Fetched: "fetched" } as const;
export const LauncherWebview = { WebView2: "webview2", WkWebView: "wkwebview", WebKitGtk: "webkitgtk-4.1", Bundled: "bundled" } as const;
export const LauncherMinimumOs = { Windows10: "windows-10", MacOs11: "macos-11", Glibc234: "glibc-2.34", Glibc235: "glibc-2.35" } as const;
export const LinuxDistro = { Debian: "debian", Fedora: "fedora", Arch: "arch" } as const;
export const LauncherComponent = { Launcher: "launcher", Installer: "installer" } as const;
export const GraphicsApi = { OpenGl: "opengl", Vulkan: "vulkan" } as const;

// launcher.schema.ts (скорочено; кожна схема з .openapi("Site…"))
LauncherDownloadSchema = z.object({
  id: z.string().regex(/^[a-z0-9]+(?:-[a-z0-9]+)*$/u).max(40),   // "windows-x64-exe", "linux-arm64-appimage"
  os: z.enum(LauncherPlatform),
  arch: z.enum(LauncherArch),
  format: z.enum(LauncherFormat),
  recommended: z.boolean(),                                       // типовий файл своєї групи (ОС, архітектура)
  fileName: z.string().min(1).max(128),
  url: MediaUrlSchema,                                            // https, origin лаунчера
  sizeBytes: z.number().int().positive().max(2_147_483_647),
  sha256: Sha256Schema,
  installerVersion: SemVerSchema,
  jar: z.enum(LauncherJarDelivery),
  webview: z.enum(LauncherWebview),
  signature: z.object({
    verified: z.boolean(),                                        // API перевірив байти (розділ 7.4)
    installerBytes: z.number().int().positive().nullable(),       // для вбудованого jar: межа престартер/jar
    installerSha256: Sha256Schema,                                // sha256 активу з підписаного release.json
  }),
  builtAt: TimestampSchema,
});

LauncherReleaseSchema = z.object({                                // GET /launcher
  launcher: z.object({ version: SemVerSchema, releasedAt: TimestampSchema }),   // Asterium Runtime
  installer: z.object({ version: SemVerSchema, releasedAt: TimestampSchema }),  // престартер
  gravit: SemVerSchema,                                           // "5.7.12"
  javaVersion: z.string().min(1).max(8),                          // JRE, яку ставить інсталятор: "25"
  downloads: z.array(LauncherDownloadSchema).min(1).max(16),
  jar: z.object({ url: MediaUrlSchema, sizeBytes: …, sha256: Sha256Schema }).nullable(),
  verification: z.object({
    manifestUrl: MediaUrlSchema, signatureUrl: MediaUrlSchema,
    publicKey: z.string().max(200),                               // base64 SPKI Ed25519
    keyFingerprint: z.string().regex(/^[0-9a-f]{16}$/u),
    checkedAt: TimestampSchema,
  }),
  requirements: LauncherRequirementsSchema,                       // розділ 7.2
  generatedAt: TimestampSchema,
}).superRefine(/* унікальні id; кожна ОС має файл; рівно один recommended на групу (os, arch); для linux recommended - appimage */);

LauncherChangelogEntrySchema = z.object({                         // GET /launcher/changelog?locale&component&limit&cursor
  component: z.enum(LauncherComponent),
  version: SemVerSchema,
  releasedAt: TimestampSchema,
  anchor: z.string().regex(/^(launcher|installer)-\d+\.\d+\.\d+$/u),
  notes: z.object({ added: NoteLines, changed: NoteLines, fixed: NoteLines }),  // до 20 рядків по 300 символів
  notesLocale: z.enum(Locale),                                    // мова нотаток (en для запасного шляху)
  sourceUrl: MediaUrlSchema.nullable(),
});
LauncherChangelogSchema = z.object({ items: z.array(LauncherChangelogEntrySchema).max(50), nextCursor: z.string().max(200).nullable() });
```

`SiteV1Route` отримує `LauncherChangelog: { method: "get", path: "/launcher/changelog" }`.

### 7.2 Системні вимоги: що фіксоване і що виводиться

**Фіксоване** (перевірено; значення - константи конфігу API):

| ОС | Мінімум | Звідки |
|---|---|---|
| Windows | Windows 10 або 11, 64-bit (x64 або ARM64); WebView2 (у Windows 10 з 1803 і Windows 11 - є) | документація Tauri (WebView2); платформи JRE 25 (лейн звіряє зі сторінкою підтримуваних конфігурацій BellSoft) |
| macOS | macOS 11 Big Sur, Intel або Apple Silicon | `LC_BUILD_VERSION minos=11.0` у `bin/java` Liberica (виміряно); Tauri за замовчуванням 10.13 |
| Linux | один файл: glibc 2.34+ (Ubuntu 22.04+, Debian 12+, Fedora 35+, RHEL 9+, Mint 21+, Arch); AppImage: glibc 2.35+ (Ubuntu 22.04+, Debian 12+, Fedora 36+, Mint 21+, Arch; не RHEL 9, виміряно на етапі Build); x86_64 або aarch64; X11 або Wayland (XWayland); GTK 3, libXtst, ALSA; один файл - WebKitGTK 4.1; AppImage - FUSE | `objdump -T` (виміряно `GLIBC_2.34`), M1-M3 |
| усі | інтернет для першого запуску (Java ~120-150 МБ, лаунчер ~8 МБ, клієнт сервера); Java ставиться автоматично | - |

Назви пакетів для Linux (`packages`): Debian/Ubuntu - `libwebkit2gtk-4.1-0 libgtk-3-0 libxtst6 libasound2`
(на Ubuntu 24.04 `libasound2t64`), Fedora - `webkit2gtk4.1 gtk3 libXtst alsa-lib`, Arch - `webkit2gtk-4.1 gtk3
libxtst alsa-lib`. Лейн site перевіряє кожну назву в репозиторіях дистрибутивів (packages.ubuntu.com,
packages.debian.org, Fedora, Arch) і не пише з пам'яті.

**Виводиться з серверів** (API, за даними профілів, які вже має платформа):

- **Пам'ять.** Для кожного сервера - мінімум і рекомендована пам'ять гри за правилом рантайму
  `MemoryRequirement` (`asteria.minRam`, `asteria.recommendedRam`, інакше `settings.ram` профілю, нижня межа 2 ГБ для
  Minecraft 1.17+ і модованих клієнтів). Рантайм попереджає, коли гра бере понад 75 % пам'яті комп'ютера
  (`SYSTEM_SHARE = 0.75`), тож пам'ять комп'ютера: мінімум = `max(мінімум гри) / 0,75`, рекомендовано =
  `max(рекомендована гри) / 0,75`, обидва округлені вгору до ряду 4, 6, 8, 12, 16, 24, 32 ГБ. Приклад: рекомендовані
  8 ГБ гри → 10,7 → 12 ГБ комп'ютера.
- **Диск.** JRE (розпакований розмір вимірює CI) + лаунчер + найбільший клієнт серед серверів + 10 %.
- **Графіка.** За версіями Minecraft серверів: 1.17 і новіші потребують OpenGL 3.2 core (гра не стартує нижче);
  рекомендовано - як в офіційних вимогах Mojang для цих версій. Версії, що переходять на Vulkan 1.3, дають
  `api: "vulkan"`. Пороги версій і рекомендоване значення лейн site бере з офіційної сторінки вимог Mojang
  (help.minecraft.net) і записує джерело поруч із константою.
- Сторінка показує, з чого це виведено: таблиця серверів (назва, версія Minecraft, пам'ять гри, розмір клієнта).
  Моки беруть ці числа з `canon.json` (вигадані дані), продакшн - з профілів.

### 7.3 Changelog

Спільна стрічка версій лаунчера (Asterium Runtime) і інсталятора (престартер), найновіші вгорі, лише стабільні.
Нотатки - з підписаного `release-notes.json`, інакше з розбору опису релізу (англійською, з міткою). Чистий розбирач
Markdown живе в спільному пакеті (його використовують і генерація фікстур, і API) з тестами на справжніх описах
релізів рантайму v3.0.0-v3.1.1 ([ADR 0012](adr/0012-launcher-changelog-source.md)).

### 7.4 Як API будує `GET /launcher` (документ для workflow API)

Лейн site переносить цей розділ у `asterium-platform/docs/architecture/launcher-downloads.md`; API реалізує його
в іншому workflow.

**Джерела:**
1. `D` - `downloads.json` LaunchServer (`LAUNCHER_DOWNLOADS_URL`, типово
   `https://launcher.asterium.pro/downloads/downloads.json`): до 256 КіБ, тайм-аут 5 с, схема перевіряється.
2. `P` - найновіший стабільний реліз `AsteriaCraft/launcher-prestarter`: `release.json` і `release.json.sig`,
   підпис перевіряється закріпленими ключами (`PRESTARTER_SIGNING_PUBLIC_KEYS`, ті самі, що в AsteriumReleases),
   прив'язка (репозиторій, компонент, тег, канал). `sha256(release.json)` має дорівнювати
   `D.prestarter.manifestSha256`: сервер справді працює саме з цим релізом.
3. `R` - стабільні релізи `AsteriaCraft/asterium-launcher` (тег, `publishedAt`, опис, необов'язковий
   `release-notes.json`, перевірений підписом `release.json` цього релізу ключем рантайму). Токен - лише якщо
   репозиторій приватний (fine-grained, read-only), у секретах API.
4. `S` - сервери і профілі з наявного джерела даних платформи (для вимог).

**Алгоритм:**
1. Для кожного файла `D.files`: `x86_64→x64`, `aarch64→arm64`, `elf→binary`; `id = <os>-<arch>-<format>`;
   `recommended` - exe для Windows, dmg для macOS, appimage для Linux.
2. Звірка з `P`: для файлів з копією jar `sha256` і розмір дорівнюють активу з `P`; для файлів з вбудованим jar
   `prestarterSize` дорівнює розміру активу з `P`. Розбіжність - файл не публікується, подія `level=error`.
3. **Перевірка байтів** - фонова задача черги (ідемпотентна за `sha256` файла): завантажити файл, порахувати
   sha256 і звірити з `D`; для вбудованого jar без Authenticode - `sha256(перші prestarterSize байтів)` = sha256
   активу `P`, а `sha256(решти)` = `D.jar.sha256`. Результат зберігається; лише тоді `signature.verified = true`.
4. `launcher` = тег рантайму з `D.runtime` і його `publishedAt` з `R`; `installer` = версія і дата `P`.
5. `requirements` - [розділ 7.2](#72-системні-вимоги-що-фіксоване-і-що-виводиться).
6. Кеш: Redis SWR (свіжий 60 с, застарілий до 1 год), оновлення під lease; при будь-якій помилці - останній добрий
   знімок з Postgres з його `generatedAt`; немає жодного - 503, проблема
   `https://asterium.pro/problems/launcher-unavailable`.
7. `GET /launcher/changelog`: злиття `R` і релізів престартера, найновіші вгорі, курсор за (дата, компонент, версія).

### 7.5 Структура коду сайту (FSD)

```
apps/web/app/[locale]/(site)/launcher/page.tsx              сторінка
apps/web/app/[locale]/(site)/launcher/changelog/page.tsx    уся історія
apps/web/app/download/route.ts                              редирект (лишається поза [locale])
apps/web/src/entities/launcher/api/                         порт: release(), changelog(); mock і http
apps/web/src/features/launcher-download/{ui,model,lib}/     DownloadTrigger, DownloadModal, pickDownload
apps/web/src/views/launcher/{ui,model}/                     розділи сторінки
apps/web/src/views/launcher-changelog/{ui,model}/
apps/web/src/shared/lib/detect-system*.ts                   визначення (замість detect-platform)
apps/web/src/shared/mock-canon/fixtures/launcher.fixture.ts фікстури з реальних артефактів і changelog
```

Усі наявні «Завантажити» (шапка, hero, секція головної, сторінка серверів, мобільне меню, підвал, кабінет) і діалог
входу на сервер переходять на `DownloadTrigger` і новий контракт.

## 8. Відмови і що бачить гравець

| Відмова | Що відбувається | Що бачить гравець |
|---|---|---|
| API Liberica недоступний | аварійна таблиця з sha256 ([ADR 0005](adr/0005-jre-acquisition-and-local-store.md)) | нічого незвичного |
| Обрив або підміна архіву JRE | sha1/розмір не збігаються → одна автоматична повторна спроба | після другої - «Не вдалося завантажити Java» + «Спробувати ще» |
| Немає місця | перевірка до завантаження | «Потрібно ще 420 МБ на диску C:» |
| Немає WebView2 (Windows) | режим без вікна | нативне повідомлення, далі лаунчер відкривається сам |
| Windows 7/8.1; Windows 10 on ARM | бінарник не запускається (Rust ≥ 1.78 і Liberica 25 - Windows 10+; Windows 10 on ARM без емуляції x64) | сайт показує «не підтримується» до завантаження |
| SmartScreen / Smart App Control (непідписаний `Asterium.exe`) | SmartScreen: «Докладніше» → «Усе одно запустити»; SAC у режимі enforcement блокує без обходу для окремої програми | FAQ сайту; підпис Authenticode - відкрите питання 5 |
| Один файл Linux без WebKitGTK | завантажувач виходить з кодом 127 до `main` | нічого; сайт за замовчуванням дає AppImage і показує вимоги |
| AppImage без FUSE | runtime AppImage пише в термінал | FAQ: `--appimage-extract-and-run` |
| Немає GTK 3 / libXtst для JavaFX | перевірка `ldd` до запуску (з очищеним середовищем), ADR 0006 | вікно з командою встановлення пакетів для свого дистрибутива; лаунчер не запускається (код 5) |
| JVM лаунчера падає в перші секунди (режим копії) | обгортка Gravit пише `Process exit with error code`; повтор з `-Dlauncher.waitProcess=true` | вікно з останніми рядками журналу (стек JVM лаунчера) |
| Обгортка AppImage/DMG нижча за `minWrapperVersion` з підписаної політики | лаунчер не запускається (код 7) | вікно «Завантажте нову версію Asterium» з посиланням на сайт |
| Є новіша обгортка AppImage/DMG | запуск як завжди | ненав'язливе повідомлення з посиланням (не частіше разу на добу) |
| Нова версія JRE під час запуску | вікно встановлення | кнопка «Грати зараз» запускає зі старою JRE, оновлення - наступного разу |
| LaunchServer недоступний на першому запуску AppImage/macOS | завантаження jar не вдається | «Сервер Asterium недоступний» + «Спробувати ще» |
| Пошкоджена копія jar | перевірка zip до запуску або `Invalid or corrupt jarfile` від обгортки → повторне завантаження jar → запуск | нічого, або помилка після другої спроби |
| Пошкоджений вбудований jar | перевірка zip до запуску | «Файл пошкоджено, завантажте Asterium знову» (код 4) |
| Офлайн, JRE вже є | перевірка оновлень JRE пропускається | гра запускається |
| macOS Gatekeeper | застосунок без Developer ID | діалог «не відкрито» → інструкція з сайту і з вікна DMG |
| Варіант без URL на сервері | неможливий: такий варіант не вмикається | - |
| Файл престартера відсутній на сервері | варіант не потрапляє в конфіг Prestarter | інші варіанти збираються |
| Новий актив на встановленому тегу | доповнення того самого тегу | - |
| Реліз з неправильною прив'язкою `arch` | модуль відмовляє в установці | - (лишається попередній реліз) |
| CDN віддав старий файл | `Cache-Control: no-cache`; задача перевірки API бачить розбіжність sha256 | сайт знімає позначку «перевірено» |
| `downloads.json` недоступний | API віддає останній добрий знімок | дані можуть бути старшими; без знімка - «тимчасово недоступно» |
| Нотаризація впала в `publish.yml` | реліз не публікується | - |
| Частина секретів Apple | помилка до збирання | - |

## 9. Безпека

**Ланцюжок довіри:**

| Що | Чому довіряємо | Що зустрічає підробка |
|---|---|---|
| Сирі престартери, AppImage, DMG | збирання GitHub Actions з коміту в `release` (злиття - лише власник; захист гілки - дія власника); `release.json` з sha256 кожного активу підписано Ed25519; атестація походження | AsteriumReleases: підпис, прив'язка до репозиторію/компонента/тегу/каналу, anti-rollback, розмір, digest GitHub, sha256 - інакше нічого не ставиться; узгодженість `os`/`arch`/`role` з конфігом |
| `release.json` | Ed25519, ключ лише в секреті | підпис не сходиться → відмова |
| Файли на диску LaunchServer | перевірка на старті і перед збиранням (наявний механізм) | карантин або ремонт з перевіреного сховища |
| Складені файли (`Asterium*.exe`, `Asterium_linux*`) | збирає Gravit на сервері; jar підписано ключем Gravit; оновлення перевіряється SHA-512 + secure hash | задача перевірки API: префікс ≠ підписаний престартер або хвіст ≠ `Asterium.jar` → без позначки «перевірено», подія `error` |
| Завантаження гравцем | TLS до `launcher.asterium.pro`; sha256 на сайті | гравець може звірити sha256 і (для досвідчених) підпис `release.json` |
| JRE | HTTPS до github.com + sha1 з api.bell-sw.com (два хости); аварійний - sha256 у коді | невідповідність → не встановлюється |
| Копія jar (AppImage, macOS) | HTTPS до закріпленого хоста лише з коренями Mozilla (`webpki-root-certs`, не сховище ОС), без редиректів на інші хости, ліміт 64 МіБ, перевірка zip/`Main-Class` | CA, доданий в ОС (корпоративний проксі або шкідливе ПЗ), не підмінить jar; JVM лаунчера так само не довіряє сертифікатам ОС (`cacerts` JRE) |
| Політика обгортки (`prestarter-policy.json`) | підпис Ed25519 `release.json` вшитим ключем, прив'язка до репозиторію/компонента/каналу, sha256 активу, anti-rollback за версією релізу | відхиляється, діє збережена або вшита |

**Ланцюг постачання:** дії GitHub закріплені SHA; `cargo --locked` і `cargo deny` (джерела - лише crates.io, без git
після прибирання форку tao); `yarn --frozen-lockfile`, прибрано пакет `"-"`; інструменти AppImage закріплені sha256,
бандлер працює без мережі; `publish.yml` без кешів; секрети Apple (і за бажанням `RELEASE_SIGNING_KEY`) - лише в
environment `release`; PR-збирання без секретів.

**Межі.** Компрометація origin `launcher.asterium.pro` або ключа Gravit дає зловмиснику лаунчер на всіх платформах -
так само, як сьогодні для `Asterium.exe`; цей дизайн цю межу не розширює, а задача перевірки API робить підміну
префікса престартера помітною. Локальний зловмисник з доступом до середовища процесу вже має доступ до файлів гравця;
перевизначення лише loopback ([ADR 0008](adr/0008-loopback-test-endpoints.md)).

**Приватність.** Престартер нічого не надсилає, крім запитів до API Liberica, github.com і `launcher.asterium.pro`;
журнали локальні, домашній каталог записано як `~`. Сайт використовує Client Hints лише для вибору файла (у браузері
і в редиректі); UA і IP не потрапляють у журнали додатку і мітки метрик (правило 11 `AGENTS.md` платформи).

## 10. Тестування

| Рівень | Де | Що доводить |
|---|---|---|
| Модульні Rust | `cargo test`, 5 раннерів | каталог JRE, вибір запису API, перевірки, розпакування зі зловмисними архівами, визначення вбудованого jar, команда і середовище запуску, перевизначення, стан і блокування, i18n-ключі |
| Інтеграційні Rust | `src-tauri/tests/`, 5 раннерів | повний шлях без GUI проти локального HTTPS (тестовий CA) з `fake-java`: встановлення, оновлення JRE, копія jar, рання помилка і повтор, `setsid`, від'єднання |
| Фронтенд | vitest + `svelte-check` | редуктор подій, прогрес, тексти станів |
| Скрипти CI | bats/bash + `shellcheck` + `actionlint` | `release-manifest.sh` (метадані, старий формат), `make-dmg.sh`, `macos-signing.sh --self-test` |
| Smoke релізних байтів | `smoke.yml` на рідних раннерах | кожен з 7 артефактів: GUI (знімок) → справжній Liberica → `java -jar` → FX-проба, що повторює обгортку Gravit (перезапуск з `-cp`, вихід через 3 с) і відкриває вікно з `WebView` (WebKit) → маркер і знімок; чисте середовище; Windows без WebView2; шлях не з ASCII, який Java читає, і відмова з кодом 6 для шляху, який кодова сторінка ANSI не записує; «голий» Linux з підказкою пакетів; macOS карантин, `spctl`, назва в Dock; Wayland |
| AsteriumReleases | JUnit + e2e (`run-e2e.sh`, розділ X) | [ADR 0010](adr/0010-launchserver-variants-and-static-downloads.md) «Перевірка»: побайтові складені файли, nginx, `LauncherSignCheck` до і після рестарту, доповнення тегу, відкат через `since` |
| Сайт | vitest, Playwright, axe, знімки | [ADR 0011](adr/0011-site-download-experience.md) «Перевірка» |
| Інтеграція (наступний етап workflow) | контейнер з e2e LaunchServer | справжній `Prestarter-linux-x86_64` з CI-запуску лейну як актив тестового релізу → LaunchServer збирає `Asterium_linux` → запуск під Xvfb у контейнері з GTK: лаунчер (jar) стартує і питає сервер `LINUX_X86_64`; самооновлення на першому запуску, поки престартер щойно вийшов; фікстури сайту оновлюються розмірами і sha256 з того самого запуску |
| Справжній лаунчер на Windows (rc) | ПК власника, перед злиттям релізу | 0.3.0-rc.1 на тестовому LaunchServer: `Asterium.exe` відкриває лаунчер, вхід, самооновлення (розділ 11) |

Правила звітів: кожна команда з кодом виходу (без пайпів) і кількістю тестів; номери запусків CI і коди виходу кожного
job; GUI на столі власника не запускається - лише Xvfb, контейнери і раннери.

## 11. Розгортання і відкат

| Крок | Хто | Що | Перевірка | Відкат |
|---|---|---|---|---|
| 1 | власник зливає PR `gravit-docker` у `main` (Dokploy розгортає сам) | образ з AsteriumReleases 2.3.0, entrypoint створює `downloads/`, compose з томом `downloads`, nginx | `mc-releases status` як раніше; конфіг Prestarter не переписано; `downloads.json` не з'явився (старий конфіг) | revert PR |
| 2a | власник | утримати компонент: `mc-releases update prestarter v0.2.0` (вебхук утримання не знімає) | `mc-releases status`: prestarter утримано на v0.2.0 | - |
| 2b | власник зливає PR `main` → `release` з версією `0.3.0-rc.1` | pre-release (продакшн з `allowPrerelease=false` його ігнорує) | тестовий LaunchServer (`allowPrerelease=true`) ставить rc і збирає `Asterium.exe`, `Asterium_linux*`; smoke справжнього лаунчера (розділ 10) і ручний запуск на справжньому Windows-ПК | rc не доходить до гравців |
| 2c | власник зливає PR з версією `0.3.0` | сім активів + нотатки + політика | CI зелений до злиття (дата в changelog); `mc-releases update prestarter` знімає утримання; модуль зі старим конфігом ставить лише `Prestarter.exe` v0.3.0, перезбирає `Asterium.exe`; перші 4,x МБ `Asterium.exe` = новий `Prestarter.exe`; журнали підтримки першу годину | `mc-releases rollback prestarter` (v0.2.0) - **але відкат доходить лише до гравців, чий престартер 0.3.0 ще запускає лаунчер** (оновлення йде через самооновлення лаунчера); хто застряг до запуску лаунчера, завантажує файл із сайту заново: готові банер на сайті і повідомлення в Discord з посиланням |
| 3 | власник оновлює `AsteriumReleases-Config.prod.json` → `ASTERIUM_RELEASES_CONFIG_B64` у Dokploy → redeploy | активи з `launcherVariant`/`download`/`since` | `mc-releases status` (варіанти, URL, хеші); `curl -sI` на `Asterium_arm64.exe`, `Asterium_linux`, `Asterium_linux_arm64`, `downloads/*`, `downloads/downloads.json`; `LauncherSignCheck` з e2e проти продакшну не запускається | попередній конфіг: варіанти зникають після наступного збирання, `downloads/` модуль чистить від файлів, яких немає в конфігу |
| 4 | власник зливає PR сайту в `feat/web` і розгортає Site у Dokploy вручну | модалка, сторінка, новий контракт на моках | Playwright на розгорнутому сайті | попередній образ Site |
| 5 | workflow API (окремо) | `GET /launcher`, `/launcher/changelog`, задача перевірки | контрактні тести; `ASTERIUM_DATA_SOURCE_LAUNCHER=api` | назад на mock |
| 6 | власник (необов'язково) | старий сайт Azuriom: Linux → AppImage, macOS → DMG (`Asterium.jar` лишається) | посилання відкриваються | старі посилання |
| пізніше | власник | секрети `APPLE_*` → наступний реліз підписаний; сертифікат Authenticode → модуль `OSSLSignCode` | `codesign`/`spctl`, `osslsigncode verify` | прибрати секрети |

Порядок 1 → 2 → 3 обов'язковий: конфіг з новими активами до модуля 2.3.0 вперся б у дефект «того самого тегу», а до
релізу 0.3.0 - у `MISSING_ASSET`.

## 12. Відкриті питання власнику

Кожне має рішення за замовчуванням; робота не чекає відповіді. Список з поясненнями - у звіті етапу; коротко:

1. Windows ARM64: JRE x64 під емуляцією (повний лаунчер) чи нативна (швидше, без вбудованих сторінок)? → x64; тоді
   ARM-пристрої отримують `Asterium.exe`, а `Asterium_arm64.exe` вмикається разом з нативною JRE (ADR 0004).
2. Власне сховище престартера (одноразово ~120 МБ для нинішніх гравців Windows) чи далі `GravitLauncherStore`? → власне.
3. Linux за замовчуванням на сайті → AppImage.
4. Лишити `launcher-prestarter` публічним (безкоштовні macOS/ARM-раннери, атестація)? → так.
5. Authenticode для Windows → поки без нього (як сьогодні; SmartScreen і Smart App Control - у FAQ). Пізніше:
   ключ сертифіката з 2023-06-01 має жити в HSM, тож варіанти - OV через хмарний HSM і `osslsigncode -pkcs11module`
   або jsign з хмарним KMS, або сертифікат Certum для відкритого коду (репозиторій публічний); підпис з таким ключем
   доводиться в e2e до купівлі.
6. `release-notes.json` у релізах рантайму → так, з наступного релізу.
7. Environment `release` і захист гілки `release`; переносити `RELEASE_SIGNING_KEY` у environment → так.
8. Правило кешу Cloudflare для `launcher.asterium.pro` (`/downloads/*` і `Asterium*` не кешувати довго) → перевірити
   і лишити bypass.
9. Dock на macOS показує «java» для вікна лаунчера, поки рантайм не додасть `-Xdock:name` у дочірню JVM → окреме
   завдання для `asterium-launcher`.
10. Apple Developer Program ($99/рік) → коли власник вирішить; до того ad hoc.
11. Знахідки етапу Integration у рантаймі (R1-R3, [ADR 0010](adr/0010-launchserver-variants-and-static-downloads.md),
    розділ «Виміряно на етапі Integration»): оновлення лаунчера губиться (callback ставиться після запиту оновлень) або
    не встановлюється (hook Gravit після початку виходу), і зрідка лаунчер висить без вікна (блокування ініціалізації
    класів JavaFX) → окреме завдання для `asterium-launcher`, бажано в 3.1.2. Престартер їх не має й обійти не може:
    вони однакові для всіх форматів і для престартера 0.2.0.

## 13. План робіт (лейни)

| Лейн | Репозиторій і worktree | Гілка | Головне |
|---|---|---|---|
| prestarter | `launcher-prestarter`, `C:/Users/Max/Desktop/Asterium/repos/launcher-prestarter-xplat` (вже створено, з цим дизайном) | `feat/prestarter-crossplatform` від `origin/release` 8b9cd54 | [розділ 4](#4-престартер-зміни-в-коді) і [розділ 5](#5-ci-і-релізи), ADR 0001-0009, 0012 |
| server | `gravit-docker`, `C:/Users/Max/Desktop/Asterium/repos/gravit-docker-xplat` (новий) | `feat/prestarter-crossplatform` від `origin/main` 1e2ca7f | [розділ 6](#6-launchserver), ADR 0010 |
| site | `asterium-platform`, `C:/Users/Max/Desktop/Asterium/repos/asterium-platform-download` (новий) | `feat/prestarter-download` від `origin/feat/web` f3a7d68 | [розділ 7](#7-сайт), ADR 0011, 0012 |

Детальні брифи лейнів - у звіті етапу Design (поле `lanes`).

## 14. Докази і джерела

**Експерименти Understand** (скрипти в scratchpad сесії: `exp/linux/`, `exp/minimal/`, `exp/hello/`, `exp/fx/`,
`liberica/*.py`; Docker-образи `prestarter-exp:jammy`, `prestarter-exp:minimal`, том `prestarter-exp-work` з
ELF, AppImage, ARM64 exe і JRE):

- W1-W4: реліз v0.2.0 і `SHA256SUMS.txt`; `java -jar` на PE з дописаним jar; хвіст Authenticode ламає jar, поле
  коментаря EOCD лікує.
- D0: збирання ELF і AppImage на ubuntu:22.04 (перша спроба впала на `xdg-open binary not found`).
- E1-E5: ELF з jar під Xvfb: GUI → Liberica → `java -jar` → `java.home=~/.local/share/GravitLauncherStore/JRE-25`,
  Java 25.0.4.1; другий запуск без вікна; змінні `__GL_*` на першому запуску.
- A0-A5: AppImage: зсув squashfs, `Invalid or corrupt jarfile` з `current_exe()`, `$APPIMAGE` працює, витік ~25
  змінних.
- M1-M3: «голий» Ubuntu 24.04: код 127; мінімальні бібліотеки для AppImage; JavaFX потребує GTK 3 і libXtst.
- X1: ARM64 exe через cargo-xwin на Linux.
- L1-L6: API Liberica для всіх ОС і CPU (400 на `arch=aarch64`), склад архівів, відсутність WebKit у Windows aarch64,
  `minos=11.0` на macOS. Повторно 2026-10-02: API повертає CSPU `25.0.4.1+1` і PSU `25.0.4+9`, поле `sha1`.
- T1: різниця форку tao з upstream - один рядок.
- Продакшн (лише GET): склад `Asterium.exe`, відсутність Authenticode, 404 на варіантах, `cf-cache-status: DYNAMIC`.
- Gson 2.14.0 з `/app/lib`: невідомий варіант → ключ `null`, два → `duplicate key: null`.

**Код:** `launcher-prestarter` `origin/release` 8b9cd54; `gravit-docker` `origin/main` 1e2ca7f
(`ReleasesConfig.java`, `ReleaseManifest.java` - невідомі поля ігноруються, `GravitLauncherHost.java`,
`AsteriumReleasesModule.java`, `nginx.conf`, `docker-compose.yml`); Gravit 5.7.12 fef9bae6 (`LaunchServer.java`,
`LocalUpdatesProvider.java`, `LauncherRequest.java`, `LauncherBackendImpl.java`, `LauncherUpdater.java`,
`ClientLauncherWrapper.java`, `EnvHelper.java`, `DirBridge.java`); `asterium-launcher` `origin/release`
(`CHANGELOG.md`, `MemoryRequirement.java`, мови `compat/lang/runtime_{be,en,pl,ru,uk}.properties`);
`asterium-platform` `origin/feat/web` f3a7d68 (`launcher.schema.ts`, `site-v1.enum.ts`, `download/route.ts`,
`detect-platform.ts`, `proxy.ts`, `site-shell.spec.ts`, `AGENTS.md`).

**Документація:** Tauri (context7 `/tauri-apps/tauri-docs`): prerequisites (WebView2 з Windows 10 1803),
windows-installer (без WebView2 застосунок «WILL NOT work»), webview-versions, macos-application-bundle
(мінімум 10.13 за замовчуванням), dmg (позиції не застосовуються на CI), app-store (universal), Sign/macOS (змінні
`APPLE_*`, stapling `.app`/`.dmg`), appimage; вихідний код бандлера AppImage Tauri (`linuxdeploy.rs`: кеш інструментів
`$XDG_CACHE_HOME/tauri`, `linuxdeploy-plugin-appimage` з `continuous`); GravitLauncher/LauncherModules
(Prestarter_module, OpenSSLSignCode_module); GravitLauncher/LauncherPrestarter `rust/5.7.x`; actions/runner-images;
PyInstaller feature notes і issue 4934 (дописані дані й codesign); eclecticlight.co і lapcatsoftware.com (App
Translocation).

## 15. Гра на кожній платформі

Лаунчер, що відкрився, ще не означає, що гра стартує: Gravit відмовляє в запуску клієнта без
`natives/<os>/<arch>` (`ClientLauncherProcess.java:98-104`, «Your operating system or architecture not supported»), а
Java клієнта фільтрується за ОС і архітектурою (`LauncherBackendImpl.java:297-323`, `customJavaDownload`). Каталоги
клієнтів продакшну збиралися для гравців Windows. Тому:

1. **Перевірка лише читанням** (лейн server): для кожного профілю продакшну - чи є `natives/linux/x86-64`,
   `natives/linux/arm64`, `natives/macosx/x86-64`, `natives/macosx/arm64` (і `natives/mustdie/x86-64`), і чи є
   придатна Java клієнта для кожної цілі. Результат - таблиця в звіті лейну і поле `clientSupport` у
   `downloads.json` ([ADR 0010](adr/0010-launchserver-variants-and-static-downloads.md), «Ревізія»).
2. **Сайт не рекламує ціль наосліп** ([ADR 0011](adr/0011-site-download-experience.md), «Ревізія»): для вибраної
   системи модалка і сторінка показують сервери, де гра ще не стартує.
3. **Приймання перед кроком 4 розгортання:** «увійти на один сервер» на кожній цілі, де `clientSupport` каже «так»
   (Windows - власник; інші - доступні власнику машини або спільнота), з записом результату.

Windows ARM64 тут окремий випадок: JVM x64 бере `mustdie/x86-64`, тож клієнти працюють так само, як на x64.

## 16. Рецензія дизайну: як враховано

Рецензія 2026-10-02 (вердикт «revise») дала 8 major і 9 minor знахідок. Кожна має рішення; лейни застосовують їх у
своїй частині.

| # | Рівень | Знахідка | Рішення | Де | Лейн |
|---|---|---|---|---|---|
| 1 | major | рання помилка за кодом обгортки не бачить JVM лаунчера | перевірки до запуску (jar, JRE, `ldd` JavaFX на Linux), розбір `launcher-start.log`, повтор з `waitProcess` | ADR 0006 | prestarter |
| 2 | major | 4 с очікування тримають образ і ламають самооновлення на місці | вбудований режим виходить одразу після `spawn()`; тест з `truncate+write` через 300 мс | ADR 0006 | prestarter, server (e2e) |
| 3 | major | `Asterium_arm64.exe` замінюється x64 на першому запуску | ARM-пристроям - `Asterium.exe`, `EXE_WINDOWS_ARM64` вимкнено до нативної JRE | ADR 0004, 0010, 0011 | усі |
| 4 | major | AppImage/DMG без каналу оновлення | підписана політика `prestarter-policy.json`, дзеркало в `downloads/`, повідомлення і мінімальна версія | ADR 0001, 0010 | prestarter, server |
| 5 | major | v0.3.0 без утримання і канарки; відкат не досягає зламаних | утримання, rc на тестовому LaunchServer, зняття утримання, чесний текст відкату, банер і Discord | розділ 11 | власник, server (README) |
| 6 | major | жоден тест не запускає JavaFX/обгортку на 6 з 7 цілей | FX-проба на всіх 7 артефактах; справжній лаунчер - інтеграція і rc | ADR 0009, розділ 10 | prestarter |
| 7 | major | ніхто не перевіряє, що гра стартує на macOS/Linux | розділ 15: перевірка natives, `clientSupport`, приймання | розділ 15, ADR 0010, 0011 | server, site |
| 8 | major | `jre-watch.yml` не запуститься, моделі гілок немає | `main` - інтеграція, реліз PR `main` → `release`, `workflow_dispatch` для CI на PR бота | ADR 0009 | prestarter, власник |
| 9 | minor | Smart App Control і HSM для Authenticode | FAQ, переписане відкрите питання 5 | розділ 8, 12, ADR 0010, 0011 | site, server |
| 10 | minor | Windows 7/8.1 і Windows 10 on ARM | сайт: «не підтримується»; перевірка емуляції x64 у нативному ARM-престартері | ADR 0004, 0011 | prestarter, site |
| 11 | minor | `canonicalize()` дає `\\?\` | `dunce`, тест | ADR 0006 | prestarter |
| 12 | minor | jar у режимі копії довіряє сховищу ОС | лише корені Mozilla для хоста лаунчера | ADR 0001, розділ 9 | prestarter |
| 13 | minor | оновлення JRE блокує гру, 15 с замало | «Грати зараз», 60 с, стадія перевірки | ADR 0005 | prestarter |
| 14 | minor | секрети середовища в `build.yml` | job `sign-macos` у `publish.yml` | ADR 0003, 0009 | prestarter |
| 15 | minor | xwin CRT/SDK і glibc від образу раннера | закріплені версії xwin; Linux у контейнері `ubuntu:22.04@sha256` | ADR 0009 | prestarter |
| 16 | minor | CI двічі, без скасування, Rosetta припущено | `concurrency`, job `gate`, явна Rosetta, фактичні числа | ADR 0009 | prestarter |
| 17 | minor | `-Xdock` на обгортці без ефекту | `JDK_JAVA_OPTIONS`, перевірка в CI | ADR 0003, 0006 | prestarter |
