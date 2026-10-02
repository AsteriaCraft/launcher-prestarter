# ADR 0006. Запуск лаунчера: шлях jar, аргументи, чисте середовище, від'єднаний процес, рання помилка

- Статус: запропоновано (етап Design, 2026-10-02)
- Пов'язані: [0001](0001-artifact-matrix-and-jar-delivery.md), [0002](0002-linux-formats-and-default.md),
  [0005](0005-jre-acquisition-and-local-store.md)

## Контекст (код `origin/release` 8b9cd54 і виміри Understand)

- `runner.rs:18-24`: `java -Dlauncher.noJavaCheck=true -jar <current_exe()>`, голий `spawn()`: середовище
  успадковується як є, на Unix немає `setsid` і stdio не закрито (закриття термінала шле SIGHUP), аргументи
  командного рядка не передаються (upstream 2.1.0 передає), рання помилка JVM губиться.
- Під AppImage `current_exe()` - файл у точці монтування (A2, A3); у дочірню JVM потрапляють ~25 змінних AppRun:
  `LD_LIBRARY_PATH` і `PATH` у точку монтування, `PYTHONHOME`, `PYTHONPATH`, `PERLLIB`, `QT_PLUGIN_PATH`,
  `GST_PLUGIN_SYSTEM_PATH*`, `GTK_PATH`, `GTK_IM_MODULE_FILE`, `GDK_PIXBUF_MODULE_FILE`, `GIO_EXTRA_MODULES`,
  `GSETTINGS_SCHEMA_DIR`, `GI_TYPELIB_PATH`, `XDG_DATA_DIRS`, `GTK_THEME`, `GDK_BACKEND=x11`, `APPDIR`, `APPIMAGE`,
  `ARGV0`, `OWD` (A5). JVM лаунчера і Minecraft успадкували б їх.
- `lib.rs:186-190` ставить `__GL_THREADED_OPTIMIZATIONS=0` і `__NV_DISABLE_EXPLICIT_SYNC=1` для себе (WebKitGTK на
  NVIDIA), і на першому запуску вони доходять до Minecraft (E3 їх показує, E4 ні).
- Обгортка Gravit (`ClientLauncherWrapper`) запускає справжню JVM лаунчера з `-cp <код-джерело>`, чекає 3 с і
  завершується з кодом 0; з битим jar вона падає одразу з кодом 1 (`Invalid or corrupt jarfile`).

## Рішення

**Команда** (`launch::command`, чиста функція, повністю покрита тестами):

```
<jre>/bin/javaw.exe | <jre>/bin/java
  -Dlauncher.noJavaCheck=true
  [macOS] -Xdock:name=Asterium -Xdock:icon=<бандл>/Contents/Resources/icon.icns
  -jar <jar>
  <аргументи престартера, крім -psn_*>
```

- `<jar>`: вбудований режим - канонічний шлях власного файла (`current_exe()` → `canonicalize`); режим копії -
  `<сховище>/launcher/Asterium.jar` ([0001](0001-artifact-matrix-and-jar-delivery.md)).
- Аргументи престартера передаються як є, як в upstream 2.1.0 (наприклад `--debug` лаунчера Gravit).

**Середовище дочірнього процесу** (`launch::environment`):

1. На початку `main()`, до будь-якого `set_var` і до ініціалізації GTK, престартер знімає копію свого середовища.
2. Дочірній процес отримує цю копію, а не поточне середовище. Змінні, які престартер ставить для себе
   (`__GL_THREADED_OPTIMIZATIONS`, `__NV_DISABLE_EXPLICIT_SYNC`, `WEBKIT_DISABLE_DMABUF_RENDERER`), не доходять
   до лаунчера, якщо гравець не мав їх сам.
3. Під AppImage (є і `APPIMAGE`, і `APPDIR`) копія вже забруднена хуками AppRun, тому:
   - видаляються `APPDIR`, `APPIMAGE`, `ARGV0`, `OWD`, `APPIMAGE_*`, `GDK_BACKEND`, `GTK_THEME`, `GTK_PATH`,
     `GTK_IM_MODULE_FILE`, `GTK_DATA_PREFIX`, `GDK_PIXBUF_MODULE_FILE`, `GIO_EXTRA_MODULES`,
     `GSETTINGS_SCHEMA_DIR`, `PYTHONHOME`;
   - зі списків шляхів (`PATH`, `LD_LIBRARY_PATH`, `XDG_DATA_DIRS`, `XDG_CONFIG_DIRS`, `GI_TYPELIB_PATH`,
     `GST_PLUGIN_PATH*`, `GST_PLUGIN_SYSTEM_PATH*`, `GST_PLUGIN_SCANNER*`, `PYTHONPATH`, `PERLLIB`,
     `QT_PLUGIN_PATH`) прибираються лише елементи всередині `$APPDIR`; порожня змінна видаляється;
   - будь-яка інша змінна, значення якої містить шлях `$APPDIR`, видаляється (запобіжник для майбутніх хуків);
   - `PATH` без жодного елемента отримує `/usr/local/bin:/usr/bin:/bin`.
4. Решта (`HOME`, `DISPLAY`, `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR`, `LANG`, проксі тощо) лишається.

**Процес від'єднаний:**
- Windows: `javaw.exe`, `CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS`, stdin з null.
- Unix: `setsid()` у дочірньому процесі (`CommandExt::pre_exec`, один `unsafe` з поясненням), stdin з `/dev/null`.
- stdout і stderr - у `<сховище>/logs/launcher-start.log` (перезаписується при кожному запуску), щоб рання помилка
  JVM («Invalid or corrupt jarfile», `UnsupportedClassVersionError`, відсутня бібліотека GTK для JavaFX) не губилася.

**Рання помилка видима:**
- Після запуску престартер чекає до 4 с. Обгортка Gravit, що вийшла з кодом 0 або ще працює, - успіх, престартер
  завершується з кодом 0. Ненульовий код за ці 4 с - помилка.
- У режимі копії престартер один раз завантажує jar знову і повторює запуск.
- Інакше він показує вікно з помилкою: зрозуміле речення, останні рядки `launcher-start.log`, кнопки «Спробувати ще»
  і «Відкрити журнали». На швидкому шляху (без вікна) вікно Tauri створюється саме для цього.
- Престартер завершується з ненульовим кодом, коли лаунчер не запустився (для тестів і скриптів).
- На Linux без бібліотек JavaFX (`libgtk-3`, `libXtst`) текст помилки називає пакети для Debian/Ubuntu, Fedora і
  Arch: це найчастіша причина на «голих» системах (M3).

**Вікно без WebView.** Якщо вікно створити неможливо (на Windows немає WebView2 Runtime; помилка ініціалізації
WebKitGTK), престартер не падає мовчки (`expect` з `panic = "abort"` прибирається):
- на Windows він показує нативне `MessageBoxW` («Готуємо Asterium: завантажимо Java, близько 120 МБ. Лаунчер
  відкриється сам.») і продовжує без вікна: лаунчеру (JavaFX) WebView2 не потрібен;
- помилка встановлення - теж `MessageBoxW` з текстом і шляхом до журналів;
- на Linux і macOS - текст у журнал і в stderr, код виходу ненульовий (на Linux один файл без WebKitGTK взагалі
  не доходить до `main`, [0002](0002-linux-formats-and-default.md)).

## Наслідки

- Minecraft більше не отримує `__GL_THREADED_OPTIMIZATIONS=0` на першому запуску і жодної змінної AppImage.
- Закриття термінала, з якого запустили престартер, не вбиває лаунчер.
- Замість «нічого не сталося» гравець бачить причину і дію, а підтримка - журнал.
- Престартер живе до 4 с довше на швидкому шляху (без вікна, непомітно для гравця).

## Перевірка

- Модульні тести: команда для кожної ОС і режиму; фільтр `-psn_*`; середовище - знімок до `set_var`, видалення
  змінних престартера, повний список A5 як фікстура (до і після), шляхи з `$APPDIR` усередині списків, порожній
  `PATH`.
- Інтеграційні тести з `fake-java`: аргументи, середовище, `setsid` (новий session id на Unix), рання помилка
  (fake-java виходить з кодом 1) → повторне завантаження jar → помилка з кодом виходу престартера.
- CI smoke: `Hello.jar` друкує своє середовище; під AppImage у ньому немає жодної змінної зі списку; на Windows
  запуск з шляху з кирилицею (`…\Ігри\Asterium.exe`).
- Windows без WebView2: `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` на порожній каталог → нативне повідомлення → маркер
  `Hello.jar`. У CI повідомлення не чекає клацання: `ASTERIUM_PRESTARTER_NONINTERACTIVE=1` пише текст повідомлення в
  журнал замість вікна. Цей перемикач безпечний і в релізній збірці (він лише прибирає діалоги, нічого не
  перенаправляє), тож тестується той самий бінарник, що отримує гравець ([0008](0008-loopback-test-endpoints.md)).
