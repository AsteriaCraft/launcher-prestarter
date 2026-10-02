# ADR 0009. CI: матриця збирання, smoke на рідних раннерах, PR-збирання без публікації, активи і маніфест релізу

- Статус: запропоновано (етап Design, 2026-10-02); переглянуто після рецензії дизайну 2026-10-02 (див. «Ревізія»)
- Пов'язані: [0001](0001-artifact-matrix-and-jar-delivery.md), [0003](0003-macos-delivery-and-signing.md),
  [0007](0007-platform-stack-upgrade.md), [0008](0008-loopback-test-endpoints.md),
  [0010](0010-launchserver-variants-and-static-downloads.md), [0012](0012-launcher-changelog-source.md)

## Контекст

- `publish.yml` працює лише на push у `release` і збирає лише `Prestarter.exe` на `ubuntu-24.04` через cargo-xwin.
  CI для PR і гілок немає, тестів немає.
- Репозиторій **публічний**: раннери `ubuntu-22.04-arm`, `windows-11-arm` і macOS безкоштовні. Коментар у
  `publish.yml` про економію хвилин приватного репозиторію застарів.
- `release` не захищена (`branches/release/protection` → 404). Єдиний секрет - `RELEASE_SIGNING_KEY` (секрет
  репозиторію). Секретів `APPLE_*` немає.
- `release.json` (schema 1) читає модуль AsteriumReleases; `ReleaseManifest.parse` ігнорує невідомі поля і на
  верхньому рівні, і в активах (перевірено в коді `gravit-docker` `origin/main` 1e2ca7f), ліміт 64 КіБ.
- Злиття в `release` публікує одразу: власник читає зелений PR як «готово до релізу».

## Рішення

### Workflows

| Файл | Тригер | Що робить | Дозволи | Секрети |
|---|---|---|---|---|
| `ci.yml` | `pull_request` (усі), `push` усіх гілок крім `release`, `workflow_dispatch` | lint, тести, збирання 7 артефактів (`build.yml`), smoke (`smoke.yml`), самоперевірка підпису macOS, для PR у `release` - ще перевірка версії і changelog | `contents: read` (+ `pull-requests: read` для пропуску дублікатів) | жодних |
| `build.yml` | `workflow_call` | збирає всі цілі, вантажить артефакти; вхід `cache` | `contents: read` | жодних (macOS завжди ad hoc) |
| `smoke.yml` | `workflow_call` | ганяє артефакти на рідних раннерах ([0008](0008-loopback-test-endpoints.md)) | `contents: read` | жодних |
| `publish.yml` | `push` у `release` | версія → `build.yml` (без кешів) → `sign-macos` (environment `release`) → `smoke.yml` на тих самих (для macOS - підписаних) байтах → маніфест, підпис, атестація → draft → реліз | job publish: `contents: write`, `id-token: write`, `attestations: write` | `RELEASE_SIGNING_KEY`; `APPLE_*` лише в job `sign-macos` (environment `release`) |
| `jre-watch.yml` | щотижня (з гілки за замовчуванням `main`) і вручну | оновлює `src-tauri/src/jre/fallback.json` окремим PR у `main` і запускає на ньому `ci.yml` (`workflow_dispatch`); перевіряє `jfxwebkit.dll` у Windows aarch64 ([0004](0004-windows-arm64-uses-x64-jre.md)) | `contents: write`, `pull-requests: write`, `actions: write`, `issues: write` | жодних |

Публікація лишається лише на `release`. PR-збирання ніколи не створює тег, реліз чи draft і не бачить жодного
секрету.

### Модель гілок

- `main` - інтеграційна гілка і гілка за замовчуванням (лише з неї GitHub запускає `on: schedule`). Власник один раз
  перемотує її на `release` (`main` e519ba5 - предок `release`, тож це fast-forward).
- Гілки змін (`feat/*`, `fix/*`, PR від `jre-watch.yml`) → PR у `main` з зеленим `ci.yml`.
- Реліз: PR `main` → `release`, який зливає лише власник; злиття публікує (розділ «Версія і changelog» перевіряє на
  цьому PR, що версія піднята і changelog датований).
- PR, створений з `GITHUB_TOKEN`, не запускає `pull_request`-workflows; тому `jre-watch.yml` після створення PR сам
  запускає `ci.yml` через `workflow_dispatch` на його гілці (виняток GitHub: `workflow_dispatch` від `GITHUB_TOKEN`
  створює запуск), і перевірки з'являються на коміті PR. Налаштування «Allow GitHub Actions to create and approve
  pull requests» вмикає власник (команда - у звіті лейну).

### Дублікати і вартість

- `concurrency: ci-<подія>-<гілка>` з `cancel-in-progress` для всього, крім `main`: новий push скасовує застарілий
  запуск.
- Push у гілку з відкритим PR пропускає важкі jobs (перший job `gate` питає `gh pr list --head <гілка>`): ту саму
  роботу вже робить запуск `pull_request`.
- JRE в smoke завантажується зі справжнього Liberica (так, як у гравця); якщо API або github.com лежить, smoke
  червоний - це залежність, яку ми свідомо приймаємо, бо саме її проходить гравець.

### Матриця збирання (`build.yml`)

| Ціль | Раннер | Rust target | Спосіб | Актив |
|---|---|---|---|---|
| Windows x64 | `ubuntu-24.04` | `x86_64-pc-windows-msvc` | cargo-xwin 0.23.1, `--no-bundle`, CRT і SDK закріплені (`XWIN_CRT_VERSION`, `XWIN_SDK_VERSION`) | `Prestarter.exe` |
| Windows ARM64 | `ubuntu-24.04` | `aarch64-pc-windows-msvc` | те саме (виміряно, X1) | `Prestarter-windows-aarch64.exe` |
| Linux x64 | `ubuntu-24.04`, контейнер `ubuntu:22.04@sha256:…` | `x86_64-unknown-linux-gnu` | `--no-bundle`, потім `--bundles appimage` з того самого компілювання; крок AppImage - у контейнері з `--network none` | `Prestarter-linux-x86_64`, `Asterium-linux-x86_64.AppImage` |
| Linux ARM64 | `ubuntu-24.04-arm`, той самий контейнер (arm64) | `aarch64-unknown-linux-gnu` | те саме (linuxdeploy не збирає ARM AppImage крос-компіляцією) | `Prestarter-linux-aarch64`, `Asterium-linux-aarch64.AppImage` |
| macOS universal | `macos-15` | `universal-apple-darwin` | `--bundles app` + `scripts/ci/make-dmg.sh`, ad hoc | `Asterium-macos-universal.dmg` |

- Windows збирається на Linux, як сьогоднішній реліз (той самий скрипт локально і в CI); запуск перевіряється на
  справжній Windows у smoke. `scripts/ci/build-windows-exe.sh` отримує параметр цілі замість вшитого x86_64.
  Для ARM64 (етап Build): `ring` збирає свій C-код для `aarch64-pc-windows-msvc` звичайним `clang`, а cargo-xwin
  передає шляхи CRT/SDK у синтаксисі clang-cl (`/imsvc`); `scripts/ci/clang-msvc-wrapper.sh` перекладає їх у
  `-isystem`. Режим `clang` самого cargo-xwin не підходить: він завантажує незакріплений сторонній MSVC sysroot.
- Linux збирається в контейнері `ubuntu:22.04`, закріпленому дайджестом: стеля glibc 2.34 не залежить від образу
  раннера (GitHub тримає два LTS, і `ubuntu-22.04` зникне після виходу 26.04), збирання відтворюється локально
  тим самим `scripts/ci/build-linux.sh`, а крок AppImage справді без мережі (`docker run --network none`).
- Кожен скрипт збирання перевіряє результат: `file` (тип і архітектура), стеля glibc на Linux (`objdump -T`,
  не вище 2.34), `lipo -info` на macOS, відсутність `libssl` у `readelf -d`.
- Мітки раннерів закріплені явно (`ubuntu-24.04`, а не `ubuntu-latest`).

### Кеші

- `ci.yml`: кеш cargo (`Swatinem/rust-cache`, закріплений SHA, ключ за ціллю і ОС), кеш xwin (`XWIN_CACHE_DIR`),
  кеш yarn.
- `publish.yml`: жодних кешів, як сьогодні (кеш, записаний іншою гілкою, не має доходити до підписаного бінарника).

### Smoke (`smoke.yml`, той самий для PR і для релізу)

| Артефакт | Раннер | Сценарій |
|---|---|---|
| `Prestarter.exe` + FX-проба | `windows-2025` | дописати jar, запустити, маркер, знімок; запуск із шляху з кирилицею; без WebView2 ([0006](0006-launching-the-launcher.md)) |
| `Prestarter-windows-aarch64.exe` і `Prestarter.exe` + FX-проба | `windows-11-arm` | те саме; `os.arch=amd64`, WebView працює під емуляцією ([0004](0004-windows-arm64-uses-x64-jre.md)) |
| `Prestarter-linux-x86_64` + FX-проба | `ubuntu-24.04` (Xvfb); контейнер Ubuntu 24.04 без GTK | маркер; у «голому» контейнері очікуваний код 127 |
| `Prestarter-linux-aarch64` + FX-проба | `ubuntu-24.04-arm` (Xvfb) | маркер |
| AppImage x64 / ARM64 | `ubuntu-24.04`, `ubuntu-24.04-arm`; «голий» контейнер Ubuntu 24.04 для x64 | jar з loopback, маркер, чисте середовище, `--appimage-extract-and-run` без FUSE; у «голому» контейнері - підказка пакетів, `apt-get install` саме їх, потім маркер |
| DMG | `macos-15` (arm64 і `arch -x86_64` під Rosetta, яку smoke ставить явно) | змонтувати, карантин, `spctl`, запуск, `xattr -l`, назва процесу в Dock ([0003](0003-macos-delivery-and-signing.md)) |
| один файл на Wayland | `ubuntu-24.04` з weston headless | знімок вікна без патча tao |

- JRE в smoke завантажується зі справжнього API Liberica: так, як у гравця.
- **FX-проба замість простого `Hello.jar`** (рецензія: жоден тест не запускав JavaFX): `tests/fixtures/fxprobe`
  збирається окремим job (`javac --release 17`, Liberica JDK з FX) і повторює поведінку обгортки Gravit: головний
  клас без JavaFX перезапускає себе через `java --add-modules javafx.* -cp <код-джерело>`, чекає 3 с і виходить з
  кодом 0 (як `ClientLauncherWrapper`); дочірня JVM відкриває вікно `Asterium` з `WebView`, чекає завантаження
  сторінки (WebKit), робить знімок сцени, пише маркер (`os.arch`, `java.home`, код-джерело, аргументи, середовище,
  конвеєр Prism, user agent WebKit) і виходить. Режим `--fail-child` імітує падіння JVM лаунчера (перевірка повтору з
  `waitProcess`). Простий `tests/fixtures/hello` лишається для швидких перевірок шляху і середовища.
- Справжній лаунчер Gravit з loopback-LaunchServer у smoke престартера не запускається: jar продакшну звертався б до
  продакшну, а тестовий jar будує e2e `gravit-docker`. Його запуск - етап інтеграції (`docs/crossplatform.md`,
  розділ 10): Linux x64 у контейнері e2e і ручна перевірка власником на Windows перед релізом (rc, розділ 11).
- Кожен знімок вікна і журнал престартера - артефакт CI на 7 днів.

### Активи релізу і маніфест

| Актив | `os` | `arch` | `format` | `role` |
|---|---|---|---|---|
| `Prestarter.exe` (назва без змін, її чекає продакшн-конфіг) | windows | x86_64 | exe | prestarter |
| `Prestarter-windows-aarch64.exe` | windows | aarch64 | exe | prestarter |
| `Prestarter-linux-x86_64` | linux | x86_64 | elf | prestarter |
| `Prestarter-linux-aarch64` | linux | aarch64 | elf | prestarter |
| `Asterium-linux-x86_64.AppImage` | linux | x86_64 | appimage | download |
| `Asterium-linux-aarch64.AppImage` | linux | aarch64 | appimage | download |
| `Asterium-macos-universal.dmg` | macos | universal | dmg | download |
| `release-notes.json` ([0012](0012-launcher-changelog-source.md)) | - | - | json | notes |
| `prestarter-policy.json` ([0001](0001-artifact-matrix-and-jar-delivery.md)) | - | - | json | policy |

- `role: prestarter` - вхід для LaunchServer (без jar), `role: download` - готове завантаження для гравця,
  `role: notes` - нотатки для сайту, `role: policy` - підписана політика обгортки.
- `release.json` лишається **schema 1**; поля `os`, `arch`, `format`, `role` додаються до кожного активу як
  необов'язкові (старий модуль їх ігнорує, новий перевіряє проти свого конфігу, [0010](0010-launchserver-variants-and-static-downloads.md)).
  `scripts/ci/release-manifest.sh` приймає `<назва>[:os:arch:format:role]`; назва без метаданих працює як сьогодні
  (скрипт копіюється в інші репозиторії дослівно).
- Один підпис Ed25519 над `release.json` покриває всі активи (механізм і ключ без змін). `SHA256SUMS.txt` - для людей.
- **Атестація походження** (`actions/attest-build-provenance`, закріплений SHA) для кожного активу, поки
  репозиторій публічний (`github.event.repository.visibility == 'public'`); перевірка:
  `gh attestation verify <файл> -R AsteriaCraft/launcher-prestarter`.
- Authenticode у CI престартера не ставиться: підпис Windows має стояти на файлі, який завантажує гравець, тобто на
  `Asterium.exe` після дописування jar (модуль `OSSLSignCode` на LaunchServer, коли буде сертифікат).

### Версія і changelog

- Версії в `tauri.conf.json`, `src-tauri/Cargo.toml` і `package.json` мають збігатися - тепер це помилка, а не
  попередження.
- Новий `CHANGELOG.md` (Keep a Changelog) і `release-notes/<версія>.json`. Публікація відмовляє, якщо в
  `CHANGELOG.md` немає `## [X.Y.Z] - YYYY-MM-DD` з датою (не «Unreleased») або немає файла нотаток. `ci.yml` робить
  ту саму перевірку на PR у `release`, тож зелений PR справді готовий до злиття.

### Що робить власник (один раз, команди дає лейн)

- Перемотати `main` на `release` і лишити `main` гілкою за замовчуванням (модель гілок вище).
- Захист гілок `release` і `main`: злиття лише через PR, обов'язкові перевірки `ci.yml`.
- GitHub Environment `release` з правилом «лише гілка `release`»; секрети `APPLE_*` - туди, коли будуть; туди ж
  переноситься `RELEASE_SIGNING_KEY` (тоді job `publish` теж оголошує `environment: release`).
- Увімкнути «Allow GitHub Actions to create and approve pull requests» (для `jre-watch.yml`).

## Наслідки

- Кожен PR доводить, що всі сім артефактів збираються і запускаються на своїх ОС; реліз публікує саме ті байти, що
  пройшли smoke у тому самому запуску.
- Реліз триває довше (збирання на 5 раннерах, підпис macOS і smoke); фактичну тривалість першого прогону записує
  звіт лейну.
- Якщо репозиторій стане приватним, macOS-хвилини коштуватимуть x10, Windows x2, а ARM-раннери - як платні
  (відкрите питання; рекомендовано лишити публічним).

## Перевірка

- `actionlint` і `shellcheck` у `ci.yml`; тести `release-manifest.sh` (bats або bash-тести: метадані, старий формат,
  погані назви, зарезервовані назви); `tag-state.sh` без змін.
- Перший прогін на запушеній гілці: номер запуску, коди виходу кожного job, розміри всіх семи артефактів - у
  звіті лейну.

## Ревізія після рецензії (2026-10-02)

| Знахідка рецензії | Що змінено |
|---|---|
| `jre-watch.yml` не запуститься (гілка за замовчуванням `main` застаріла), його PR не отримають CI, моделі гілок немає | модель гілок: `main` - інтеграція і за замовчуванням, реліз - PR `main` → `release`; `jre-watch.yml` запускає `ci.yml` через `workflow_dispatch`; налаштування для власника |
| Жоден тест не запускає JavaFX/WebKit і обгортку Gravit на 6 з 7 цілей | FX-проба, що повторює обгортку Gravit, на всіх 7 артефактах; справжній лаунчер - на етапі інтеграції і в rc |
| Секрети середовища не доходять до повторно використовуваного `build.yml` | підпис macOS - окремий job `sign-macos` у `publish.yml` ([0003](0003-macos-delivery-and-signing.md)) |
| CRT/SDK xwin не закріплені, стеля glibc залежить від образу раннера | `XWIN_CRT_VERSION`/`XWIN_SDK_VERSION`; Linux у контейнері `ubuntu:22.04@sha256` на `ubuntu-24.04(-arm)`, AppImage з `--network none` |
| Кожен коміт двічі, без скасування, Rosetta припущено, оцінка часу оптимістична | `concurrency` зі скасуванням, job `gate` для push з відкритим PR, явна Rosetta, фактичні числа у звіті |
