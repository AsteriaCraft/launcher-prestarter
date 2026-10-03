# Престартер Asterium

Маленька програма, яку гравець запускає першою: ставить Java (BellSoft Liberica 25 jre-full з JavaFX) для своєї ОС
і процесора і запускає лаунчер Asterium (GravitLauncher 5.7.12 з рантаймом Asterium). Tauri 2 (Rust у `src-tauri`,
Svelte у `src`). Працює на Windows x64 і ARM64, Linux x64 і ARM64 (один файл і AppImage), macOS universal.

Дизайн і рішення: [`docs/crossplatform.md`](docs/crossplatform.md) і [`docs/adr/`](docs/adr/). Зміни:
[`CHANGELOG.md`](CHANGELOG.md).

## Як це працює

- **Вбудований jar** (Windows `Asterium*.exe`, Linux `Asterium_linux*`): LaunchServer дописує `Asterium.jar` у кінець
  сирого престартера; престартер ставить Java і запускає `java -jar <свій файл>`, після чого одразу виходить, щоб Gravit
  міг оновити цей файл на місці.
- **Копія jar** (AppImage, macOS `.app`): престартер завантажує `Asterium.jar` з `launcher.asterium.pro` у своє
  сховище, запускає його і стежить за обгорткою Gravit кілька секунд; оновлює копію сам Gravit. Що в обгортці може
  застаріти (адреса jar, версія Java, мінімальна версія обгортки), вона бере з підписаної політики
  `prestarter-policy.json` ([ADR 0001](docs/adr/0001-artifact-matrix-and-jar-delivery.md)).
- **Сховище:** Windows `%LOCALAPPDATA%\Asterium\Prestarter`, Linux `${XDG_DATA_HOME:-~/.local/share}/asterium/prestarter`,
  macOS `~/Library/Application Support/Asterium/Prestarter`. Журнали - `logs/prestarter-1.log` (останні 5 запусків) і
  `logs/launcher-start.log` (вивід обгортки Gravit).
- **Коди виходу:** 0 - лаунчер запущено; 2 - неправильне перевизначення; 3 - Java; 4 - jar; 5 - бракує бібліотек
  Linux (вікно називає пакети); 6 - лаунчер не стартував; 7 - обгортка старша за мінімальну; 10 - інше
  ([ADR 0006](docs/adr/0006-launching-the-launcher.md)).

## Розробка

Потрібно на будь-якій ОС: Node.js 24 з corepack (yarn 1.22.22 береться з `package.json`), rustup (тулчейн
`rust-toolchain.toml` - Rust 1.98.1 - ставиться сам при першому `cargo`), Git.

| ОС | Додатково |
|---|---|
| Windows | Visual Studio Build Tools 2022 з «Desktop development with C++»; WebView2 (є у Windows 10 1803+ і 11) |
| Linux (Debian/Ubuntu) | `sudo apt install build-essential curl file libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev xdg-utils` |
| Linux (Fedora) | `sudo dnf install webkit2gtk4.1-devel gtk3-devel librsvg2-devel @development-tools` |
| macOS | Xcode Command Line Tools (`xcode-select --install`) |

```bash
corepack enable
yarn install --frozen-lockfile
yarn tauri dev                         # вікно з гарячим перезавантаженням фронтенду
cd src-tauri && cargo test             # модульні та інтеграційні тести (fake-java, локальний HTTPS-сервер)
cargo clippy --all-targets -- -D warnings && cargo fmt --check
cd .. && yarn run check && yarn run test   # svelte-check і vitest
```

`yarn tauri dev` і будь-який запуск на столі розробника відкривають справжнє вікно і завантажують справжню Java; щоб
не чіпати своє сховище, задайте `ASTERIUM_PRESTARTER_STORE` (нижче).

### Тестові перевизначення

Релізний бінарник читає рівно чотири змінні середовища ([ADR 0008](docs/adr/0008-loopback-test-endpoints.md)); інших
перемикачів немає. Неправильне значення - помилка з кодом виходу 2, а не тихий перехід на справжню адресу.

| Змінна | Що робить | Допустимі значення |
|---|---|---|
| `ASTERIUM_PRESTARTER_JRE_API` | базова адреса API Liberica | лише `http(s)://127.0.0.1:<порт>/…` або `http(s)://[::1]:<порт>/…` |
| `ASTERIUM_PRESTARTER_LAUNCHER_URL` | адреса jar для режиму копії | те саме |
| `ASTERIUM_PRESTARTER_STORE` | каталог сховища замість типового | абсолютний шлях |
| `ASTERIUM_PRESTARTER_NONINTERACTIVE` | `1`: системні діалоги пишуться в журнал замість вікон | `1` або `0` |

Перші три вмикають у вікні мітку «тестовий режим»; кожне перевизначення пишеться в журнал як `WARN override …`.

### Структура

```
src-tauri/src/
  app/       єдине місце, що знає Tauri: вікно, команди, події, режим без вікна
  flow/      один запуск: що потрібно -> зробити -> запустити лаунчер (без Tauri)
  jre/       каталог цілей, API Liberica, аварійна таблиця, розпакування, атомарне встановлення
  jar/       визначення вбудованого jar, перевірка zip, копія jar
  launch/    команда, чисте середовище, від'єднаний процес, бібліотеки Linux, розбір виходу обгортки
  policy/    підписана політика обгортки (Ed25519, той самий ключ, що й release.json)
  store/     шляхи, state.json, блокування, журнали
  net/       HTTP-клієнти (rustls), завантаження, перевизначення
  platform/  Windows, Linux/macOS, macOS
  i18n/      be, en, pl, ru, uk (ті самі JSON читає фронтенд)
src-tauri/tests/      інтеграційні тести; examples/fake_java.rs - замінник java для них
src/                  фронтенд (Svelte 5): App.svelte, lib/{components,config,i18n,types,utils}
tests/fixtures/       Hello.java і FX-проба для smoke; tests/scripts/*.bats - тести скриптів CI
scripts/ci/           збирання (Windows через cargo-xwin, Linux у контейнері, macOS), DMG, підпис, маніфест
scripts/smoke/        smoke на рідних раннерах
```

## CI і релізи (Asterium)

Модель гілок ([ADR 0009](docs/adr/0009-ci-matrix-release-assets-and-manifest.md)): `main` - інтеграційна гілка за
замовчуванням; зміни - через PR у `main`; реліз - PR `main` → `release`, який зливає лише власник.

- **`ci.yml`** - PR, push у будь-яку гілку крім `release`, ручний запуск: `cargo fmt`, clippy (також для Windows через
  `cargo xwin clippy`), `cargo test` на `ubuntu-22.04`, `ubuntu-22.04-arm`, `windows-2025`, `windows-11-arm`,
  `macos-15`; svelte-check, vitest, `cargo deny`, shellcheck, actionlint, bats; збирання всіх семи артефактів
  (`build.yml`) і smoke на рідних раннерах (`smoke.yml`); для PR у `release` - ще перевірка версії, дати в
  `CHANGELOG.md` і `release-notes/<версія>.json` і те, що ключ підпису не є секретом репозиторію. Значень секретів не
  читає і нічого не публікує. Останній job **«CI result»** чекає на всі інші - це єдина обов'язкова перевірка захисту
  гілок (`.github/protect.json`); push, який gate пропускає, звітує як «CI result (push)» і PR не підміняє.
- **`publish.yml`** - лише push у `release` (злиття PR власником **публікує одразу**):
  1. версія однакова в `tauri.conf.json`, `package.json`, `src-tauri/Cargo.toml` (і `Cargo.lock`), заголовок
     `## [X.Y.Z] - YYYY-MM-DD` у `CHANGELOG.md` і та сама дата в `release-notes/X.Y.Z.json`; тег `v<версія>` ще не
     існує, а версія вища за найновіший стабільний реліз (`scripts/ci/release-checks.sh`, `scripts/ci/tag-state.sh`);
  2. збирання без кешів (`build.yml`); підпис macOS окремим job з environment `release` (`scripts/ci/macos-signing.sh`:
     Developer ID, нотаризація і staple `.app` і DMG, коли є всі п'ять секретів `APPLE_*`; без них - ad hoc і
     `::notice::`; частина секретів - помилка);
  3. smoke на тих самих байтах (для macOS - на підписаному DMG);
  4. `SHA256SUMS.txt`, `release.json` (schema 1, кожен актив з `os`, `arch`, `format`, `role`), підписана політика,
     підпис Ed25519 (`scripts/ci/sign-release.sh`, секрет `RELEASE_SIGNING_KEY` з environment `release`, публічний ключ
     `.github/release-signing.pub.pem` - він же вшитий у престартер для перевірки політики), атестація походження
     (поки репозиторій публічний), draft → публікація як Latest (версія з `-rc.1` - pre-release).
- **`jre-watch.yml`** - щотижня з `main`: оновлює аварійну таблицю JRE окремим PR і відкриває issue, коли в Windows
  ARM64 jre-full з'явиться WebKit.

Активи релізу: `Prestarter.exe`, `Prestarter-windows-aarch64.exe`, `Prestarter-linux-x86_64`,
`Prestarter-linux-aarch64` (вхід для LaunchServer), `Asterium-linux-x86_64.AppImage`, `Asterium-linux-aarch64.AppImage`,
`Asterium-macos-universal.dmg` (готові завантаження), `release-notes.json`, `prestarter-policy.json`, `SHA256SUMS.txt`,
`release.json`, `release.json.sig`.

### Налаштування репозиторію (власник, один раз)

Ключ підпису вже існує: ним підписано 0.2.0, його публічна половина - `.github/release-signing.pub.pem`, вона вшита в
престартер і стоїть у `signingPublicKeys` LaunchServer. Секрет `RELEASE_SIGNING_KEY` має жити **лише** в GitHub
Environment `release` (з нього може розгортатися тільки гілка `release`): секрет репозиторію читає workflow з будь-якої
гілки. Поки він є секретом репозиторію, `publish.yml` нічого не публікує, а PR у `release` червоний (job «Release key
only in environment release»). Усе робить один скрипт з офлайн-копії **наявного** ключа (не нового):

```bash
scripts/setup-repository.sh AsteriaCraft/launcher-prestarter --release-key <офлайн-копія>.pem --dry-run   # подивитися
scripts/setup-repository.sh AsteriaCraft/launcher-prestarter --release-key <офлайн-копія>.pem             # зробити
```

Він звіряє ключ із `.github/release-signing.pub.pem` (чужий або новий ключ - відмова до будь-яких змін), створює
environment `release` лише для гілки `release`, кладе туди ключ, **лише потім** видаляє секрет репозиторію, ставить
захист `main` і `release` з `.github/protect.json` (злиття лише через PR із зеленим «CI result» від GitHub Actions,
актуальним щодо гілки, і для адміністраторів теж; без обов'язкового схвалення, бо автор PR не може схвалити свій;
без force push і видалення) і дозволяє Actions відкривати PR (`jre-watch.yml`). Наприкінці друкує назви секретів (не
значення) і обов'язкові перевірки. Потім - «Re-run» перевірки ключа у відкритому PR у `release`. Після перейменування
job у `ci.yml` назва «CI result» лишається; `tests/scripts/repository-setup.bats` тримає разом цю назву,
`protect.json` і список `needs`.

`scripts/make-release-signing-key.sh` - лише для **ротації** (новий ключ у environment `release` + новий `.pub.pem`
у тому самому релізному PR + новий ключ у `signingPublicKeys` поруч зі старим до цього релізу). Новий ключ без решти
кроків провалює кожен реліз (`sign-release.sh` звіряє підпис із закоміченим публічним ключем). Офлайн-підпис ключем
власника: `RELEASE_SIGNING_KEY_FILE=key.pem RELEASE_SIGNING_PUBKEY=release-signing.pub.pem scripts/ci/sign-release.sh <каталог>`.

Перевірити реліз вручну:

```bash
sha256sum -c SHA256SUMS.txt
openssl pkeyutl -verify -rawin -pubin -inkey .github/release-signing.pub.pem -in release.json -sigfile release.json.sig
gh attestation verify Asterium-macos-universal.dmg -R AsteriaCraft/launcher-prestarter
```

### Збирання локально

```bash
bash scripts/ci/build-windows-exe.sh x86_64      # або aarch64; Linux з clang, lld, llvm (cargo-xwin)
bash scripts/ci/build-linux.sh docker            # Linux: обидва формати в закріпленому контейнері ubuntu:22.04
bash scripts/ci/build-macos.sh                   # macOS: universal .app і DMG (ad hoc)
bash scripts/update-jre-fallback.sh              # оновити src-tauri/src/jre/fallback.json
```

## Ліцензія

MIT, див. [`LICENSE.txt`](LICENSE.txt). Основано на престартері GravitLauncher (`rust/5.7.x`).
