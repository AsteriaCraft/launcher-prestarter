# ADR 0009. CI: матриця збирання, smoke на рідних раннерах, PR-збирання без публікації, активи і маніфест релізу

- Статус: запропоновано (етап Design, 2026-10-02)
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
| `ci.yml` | `pull_request` (усі), `push` усіх гілок крім `release` | lint, тести, збирання 7 артефактів (`build.yml`), smoke (`smoke.yml`), для PR у `release` - ще перевірка версії і changelog | `contents: read` | жодних |
| `build.yml` | `workflow_call` | збирає всі цілі, вантажить артефакти; вхід `cache` і `macos-signing` (`adhoc`, `self-test`, `auto`) | `contents: read` | лише з `publish.yml` через environment |
| `smoke.yml` | `workflow_call` | ганяє артефакти на рідних раннерах ([0008](0008-loopback-test-endpoints.md)) | `contents: read` | жодних |
| `publish.yml` | `push` у `release` | версія → `build.yml` (без кешів, `macos-signing: auto`) → `smoke.yml` на тих самих байтах → маніфест, підпис, атестація → draft → реліз | job publish: `contents: write`, `id-token: write`, `attestations: write` | `RELEASE_SIGNING_KEY`, `APPLE_*` (environment `release`) |
| `jre-watch.yml` | щотижня і вручну | оновлює `src-tauri/src/jre/fallback.json` окремим PR; перевіряє `jfxwebkit.dll` у Windows aarch64 ([0004](0004-windows-arm64-uses-x64-jre.md)) | `contents: write`, `pull-requests: write` | жодних |

Публікація лишається лише на `release`. PR-збирання ніколи не створює тег, реліз чи draft і не бачить жодного
секрету.

### Матриця збирання (`build.yml`)

| Ціль | Раннер | Rust target | Спосіб | Актив |
|---|---|---|---|---|
| Windows x64 | `ubuntu-24.04` | `x86_64-pc-windows-msvc` | cargo-xwin 0.23.1, `--no-bundle` | `Prestarter.exe` |
| Windows ARM64 | `ubuntu-24.04` | `aarch64-pc-windows-msvc` | cargo-xwin, `--no-bundle` (виміряно, X1) | `Prestarter-windows-aarch64.exe` |
| Linux x64 | `ubuntu-22.04` | `x86_64-unknown-linux-gnu` | `--no-bundle` і `--bundles appimage` з одного компілювання | `Prestarter-linux-x86_64`, `Asterium-linux-x86_64.AppImage` |
| Linux ARM64 | `ubuntu-22.04-arm` | `aarch64-unknown-linux-gnu` | те саме (linuxdeploy не збирає ARM AppImage крос-компіляцією) | `Prestarter-linux-aarch64`, `Asterium-linux-aarch64.AppImage` |
| macOS universal | `macos-15` | `universal-apple-darwin` | `--bundles app` + `scripts/ci/make-dmg.sh` | `Asterium-macos-universal.dmg` |

- Windows збирається на Linux, як сьогоднішній реліз (той самий скрипт локально і в CI); запуск перевіряється на
  справжній Windows у smoke. `scripts/ci/build-windows-exe.sh` отримує параметр цілі замість вшитого x86_64.
- Кожен скрипт збирання перевіряє результат: `file` (тип і архітектура), стеля glibc на Linux (`objdump -T`,
  не вище 2.34), `lipo -info` на macOS, відсутність `libssl` у `readelf -d`.
- Мітки раннерів закріплені явно (`ubuntu-22.04`, а не `ubuntu-latest`).

### Кеші

- `ci.yml`: кеш cargo (`Swatinem/rust-cache`, закріплений SHA, ключ за ціллю і ОС), кеш xwin (`XWIN_CACHE_DIR`),
  кеш yarn.
- `publish.yml`: жодних кешів, як сьогодні (кеш, записаний іншою гілкою, не має доходити до підписаного бінарника).

### Smoke (`smoke.yml`, той самий для PR і для релізу)

| Артефакт | Раннер | Сценарій |
|---|---|---|
| `Prestarter.exe` + `Hello.jar` | `windows-2025` | дописати jar, запустити, маркер, знімок; запуск із шляху з кирилицею; без WebView2 ([0006](0006-launching-the-launcher.md)) |
| `Prestarter-windows-aarch64.exe` + `Hello.jar` | `windows-11-arm` | те саме; `os.arch=amd64` ([0004](0004-windows-arm64-uses-x64-jre.md)) |
| `Prestarter-linux-x86_64` + `Hello.jar` | `ubuntu-22.04` (Xvfb); контейнер Ubuntu 24.04 без GTK | маркер; у «голому» контейнері очікуваний код 127 |
| `Prestarter-linux-aarch64` + `Hello.jar` | `ubuntu-22.04-arm` (Xvfb) | маркер |
| AppImage x64 / ARM64 | `ubuntu-22.04`, `ubuntu-22.04-arm`; мінімальний контейнер для x64 | jar з loopback, маркер, чисте середовище, `--appimage-extract-and-run` без FUSE |
| DMG | `macos-15` (arm64 і `arch -x86_64` під Rosetta) | змонтувати, карантин, `spctl`, запуск, `xattr -l` ([0003](0003-macos-delivery-and-signing.md)) |
| один файл на Wayland | `ubuntu-24.04` з weston headless | знімок вікна без патча tao |

- JRE в smoke завантажується зі справжнього API Liberica: так, як у гравця.
- `Hello.jar` збирається з `tests/fixtures/hello/` окремим job (javac `--release 17`): друкує джерело коду,
  аргументи, `os.arch`, `java.home`, середовище; пише маркер-файл; завершується.
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

- `role: prestarter` - вхід для LaunchServer (без jar), `role: download` - готове завантаження для гравця,
  `role: notes` - нотатки для сайту.
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

- Захист гілки `release`: злиття лише через PR, обов'язкові перевірки `ci.yml`.
- GitHub Environment `release` з правилом «лише гілка `release`»; секрети `APPLE_*` - туди, коли будуть; за бажанням
  туди ж переноситься `RELEASE_SIGNING_KEY` (відкрите питання).

## Наслідки

- Кожен PR доводить, що всі сім артефактів збираються і запускаються на своїх ОС; реліз публікує саме ті байти, що
  пройшли smoke у тому самому запуску.
- Реліз триває довше (збирання на 5 раннерах і smoke), орієнтовно 25-35 хв.
- Якщо репозиторій стане приватним, macOS-хвилини коштуватимуть x10, Windows x2, а ARM-раннери - як платні
  (відкрите питання; рекомендовано лишити публічним).

## Перевірка

- `actionlint` і `shellcheck` у `ci.yml`; тести `release-manifest.sh` (bats або bash-тести: метадані, старий формат,
  погані назви, зарезервовані назви); `tag-state.sh` без змін.
- Перший прогін на запушеній гілці: номер запуску, коди виходу кожного job, розміри всіх семи артефактів - у
  звіті лейну.
