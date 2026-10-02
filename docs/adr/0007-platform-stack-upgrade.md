# ADR 0007. Стек: Tauri 2.12, без форку tao, rustls, закріплений тулчейн, прибрані зайві залежності

- Статус: запропоновано (етап Design, 2026-10-02)
- Пов'язані: [0002](0002-linux-formats-and-default.md), [0009](0009-ci-matrix-release-assets-and-manifest.md)

## Контекст

- Сьогодні: tauri 2.8.5, tauri-runtime-wry 2.8.1, wry 0.53.3, tao 0.34.3 з патчем, @tauri-apps/cli 2.8.4,
  reqwest 0.12 з native-tls. Найновіші: tauri 2.12.1 (потребує Rust 1.90) і cli 2.12.1.
- `Cargo.toml:34-35` підміняє tao форком `GravitLauncher6/tao`, гілка `dev` (у `Cargo.lock` коміт 26c5c79).
  Єдина зміна від upstream 0.34.3 - закоментований `WlHeader::setup(...)` у `src/platform_impl/linux/window.rs`
  (T1: 2 коміти, 1 файл, 1 рядок): це стосується лише Wayland. Upstream виправив це в tao 07f3742b18 (#1218,
  0.36.0+); tauri 2.12 бере tao ^0.37. Гілка форку рухається (6 комітів попереду, 45 позаду upstream dev), її
  закріплює лише `Cargo.lock`.
- native-tls робить ELF залежним від `libssl.so.3`/`libcrypto.so.3`.
- Бандлер AppImage у cli 2.8.4 завантажував linuxdeploy і скрипти плагінів з гілок `master` без закріплення
  (Understand A0). Новіші бандлери тримають скрипти gtk/gstreamer у собі і закріплюють linuxdeploy комітом; плагін
  `linuxdeploy-plugin-appimage` досі береться з `continuous`.
- `package.json` має залежність з назвою `"-"` (`"-": "^0.0.1"`): випадковий пакет з npm у збиранні підписаного
  бінарника. `tauri` має feature `tray-icon`, яке не використовується.
- Інтерфейс показує вшите «build v1.0.0 Alpha» (`src/lib/config/app.ts:3`), справжня версія 0.2.0.
- Upstream 2.1.0 стартує вікно прихованим (`visible: false`) і показує його в `setup` - без білого спалаху.

## Рішення

1. **Оновити до tauri 2.12.x і @tauri-apps/cli 2.12.x** (точні версії в `Cargo.lock` і `yarn.lock`, обидва
   комітяться; CI збирає з `--locked` і `--frozen-lockfile`).
2. **Прибрати `[patch.crates-io] tao`.** Поведінку на Wayland підтверджує знімок під weston headless у CI
   ([0002](0002-linux-formats-and-default.md)). Якщо знімок покаже заголовок GTK, повертаємо патч у вигляді
   закріпленого коміту (`rev = "…"`, не `branch`), а не гілки.
3. **reqwest з rustls** і системним сховищем сертифікатів (rustls-platform-verifier: на Windows і macOS - сховище
   ОС, на Linux - системні сертифікати), як в upstream 2.1.0 (reqwest 0.13). Точний набір feature перевіряє лейн
   за документацією reqwest. Наслідки: ELF без OpenSSL; корпоративний TLS-проксі з власним CA, встановленим в ОС,
   працює.
4. **Тулчейн закріплено** в `rust-toolchain.toml` (точна стабільна версія ≥ 1.90, з `targets` для всіх семи
   цілей і `components = ["clippy", "rustfmt"]`). Однакова версія в CI і локально.
5. **Прибрати** залежність `"-"`, feature `tray-icon`, `dirs-next` (на `dirs`), `thiserror` (якщо не
   використовується після рефакторингу).
6. **Бандлер не ходить у мережу за неперевіреним.** `linuxdeploy-plugin-appimage` і все, що бандлер інакше
   завантажив би, кладеться в його кеш інструментів (`$XDG_CACHE_HOME/tauri`) заздалегідь з закріпленими URL і
   sha256 (`scripts/ci/appimage-tools.lock`), а сам крок `tauri build --bundles appimage` виконується без мережі
   (`unshare -n` або контейнер `--network none`): спроба щось завантажити ламає збирання, а не проходить тихо.
7. **Вікно стартує прихованим** і показується після першого кадру; версія в інтерфейсі - з `getVersion()`.
8. **Мови інтерфейсу престартера** - ті самі, що в рантаймі: be, en, pl, ru, uk; вибір за мовою ОС
   (`sys-locale`), запасна - en. Тексти коротких станів («Завантажуємо Java», «Встановлюємо», «Запускаємо лаунчер»,
   помилки з діями) замість сьогоднішніх «Loading modules...».
9. **Rust-код за ролями** (правило власника про структуру): `src-tauri/src/` містить лише `main.rs`, `lib.rs` і
   каталоги модулів - `app/` (Tauri: команди, події, вікно), `jre/` (каталог, API, завантаження, перевірка,
   розпакування, сховище JRE), `jar/` (визначення вбудованого jar, завантаження копії, перевірка), `launch/`
   (команда, середовище, процес, рання помилка), `store/` (шляхи, `state.json`, блокування, журнали), `net/`
   (HTTP-клієнт, політика хостів, loopback-перевизначення), `platform/` (windows, linux, macos), `i18n/`; фронтенд -
   `src/lib/{components,config,i18n,types,utils}`.

## Наслідки

- Менше залежностей ОС у одного файла Linux (без OpenSSL), усі залежності закріплені й перевірені.
- Оновлення Tauri зачіпає API (події, `Emitter`, конфіг); лейн проганяє повний набір тестів і smoke на всіх
  платформах до злиття.

## Перевірка

- `cargo tree -i tao` - лише crates.io, без git; `cargo tree -i openssl-sys` - порожньо.
- `cargo deny check` (ліцензії, advisories, джерела: лише crates.io), `yarn audit --groups dependencies`
  (або еквівалент) у CI.
- ELF: `readelf -d` не містить `libssl`/`libcrypto`.
- Крок AppImage без мережі зелений.
