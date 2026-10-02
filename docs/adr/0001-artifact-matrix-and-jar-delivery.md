# ADR 0001. Артефакт для кожної платформи і як до нього потрапляє jar лаунчера

- Статус: запропоновано (етап Design, 2026-10-02)
- Рішення власника, які це ADR виконує: цілі Windows x64 і ARM64, Linux x64 і ARM64, macOS universal; на Linux і
  один файл, і AppImage; macOS поки без підпису Apple.
- Пов'язані: [0002](0002-linux-formats-and-default.md), [0003](0003-macos-delivery-and-signing.md),
  [0006](0006-launching-the-launcher.md), [0010](0010-launchserver-variants-and-static-downloads.md)

## Контекст

Сьогодні гравець завантажує `Asterium.exe`. Його збирає LaunchServer: модуль Gravit `Prestarter_module` дописує
підписаний `Asterium.jar` у кінець `Prestarter.exe` (`PrestarterTask`: байти престартера, потім байти jar). Престартер
ставить JRE і запускає `java -jar <свій файл>`. Лаунчер вибирає варіант самооновлення за назвою свого файла і ОС/CPU
JVM (`LauncherRequest.java:24-25,48-103`): файл на `.jar` - це `JAR`, інакше `EXE_WINDOWS_*`, `LINUX_*` або
`MACOS_*`. Оновлення перезаписує той самий файл на місці (`LauncherBackendImpl.java:113-131`) і перезапускає
`java -jar <той самий шлях>` (`LauncherUpdater.java:71-83`).

Що виміряно на етапі Understand:

- PE і ELF з дописаним jar працюють: `java -jar` читає їх (W2, E2-E5), ELF лишається виконуваним.
- AppImage з дописаним jar не працює: `current_exe()` вказує в точку монтування `/tmp/.mount_*`, і дочірня JVM
  падає з `Invalid or corrupt jarfile` (A2, A3), а престартер виходить з кодом 0, тож гравець не бачить нічого.
  Точка монтування до того ж лише для читання і зникає, коли престартер завершується, тож самооновлення там
  неможливе.
- macOS `.app`: дописування до Mach-O або файл у `Contents/Resources` після підпису ламає печатку бандла; карантинний
  застосунок запускається з випадкової точки монтування лише для читання (App Translocation) або прямо з `.dmg`.
  Самооновлення на місці там теж неможливе.

## Рішення

Два способи доставки jar. Той самий бінарник престартера сам визначає свій спосіб під час запуску: якщо його власний
файл закінчується zip-архівом з `META-INF/MANIFEST.MF`, jar вбудований; інакше він працює з копією jar у сховищі.

| Ціль | Що завантажує гравець | Формат | Jar | Варіант оновлення Gravit |
|---|---|---|---|---|
| Windows x64 | `Asterium.exe` (LaunchServer) | PE x86-64 + jar | вбудований | `EXE_WINDOWS_X86_64` |
| Windows ARM64 | `Asterium_arm64.exe` (LaunchServer) | PE AArch64 + jar | вбудований | `EXE_WINDOWS_X86_64`: JVM x64, перше оновлення робить файл x64-збіркою ([0004](0004-windows-arm64-uses-x64-jre.md)) |
| Linux x64, один файл | `Asterium_linux` (LaunchServer) | ELF x86-64 + jar | вбудований | `LINUX_X86_64` |
| Linux ARM64, один файл | `Asterium_linux_arm64` (LaunchServer) | ELF AArch64 + jar | вбудований | `LINUX_ARM64` |
| Linux x64, AppImage | `downloads/Asterium-linux-x86_64.AppImage` (CI, статичний) | AppImage type 2 | копія в сховищі | `JAR` |
| Linux ARM64, AppImage | `downloads/Asterium-linux-aarch64.AppImage` (CI, статичний) | AppImage type 2 | копія в сховищі | `JAR` |
| macOS universal | `downloads/Asterium-macos-universal.dmg` (CI, статичний) | DMG з `Asterium.app` | копія в сховищі | `JAR` |
| будь-яка ОС з власною Java | `Asterium.jar` (LaunchServer) | jar | сам jar | `JAR` |

- **Вбудований jar.** Без змін у механізмі Gravit: LaunchServer дописує jar до сирого престартера потрібної
  платформи. Лаунчер оновлює свій файл на місці, як сьогодні `Asterium.exe`. Сирі престартери (без jar) - вхід для
  LaunchServer, не завантаження для гравця: `Prestarter.exe`, `Prestarter-windows-aarch64.exe`,
  `Prestarter-linux-x86_64`, `Prestarter-linux-aarch64`.
- **Копія jar у сховищі.** AppImage і `.app` нічого не містять від LaunchServer: їх збирає і (для macOS, коли є
  секрети) підписує CI, і вони однакові для всіх. На першому запуску престартер завантажує
  `https://launcher.asterium.pro/Asterium.jar` (адреса задана під час збирання) у
  `<сховище>/launcher/Asterium.jar` і запускає `java -jar` на цій копії. Файл закінчується на `.jar`, отже лаунчер -
  варіант `JAR`, і він оновлює цю копію сам, звичним механізмом Gravit. Престартер завантажує jar знову лише тоді,
  коли копії немає або вона не проходить перевірку (zip читається, `Main-Class` є).
- Довіра до завантаженого jar така сама, як до самооновлення Gravit і до самого завантаження на сайті: HTTPS до
  `launcher.asterium.pro`, перевірка сертифіката (rustls), заборона редиректів на інші хости, ліміт розміру.
  Підпис jar ключем LaunchServer престартер не перевіряє (див. «Безпека» в `docs/crossplatform.md`).
- Будь-який сирий престартер, запущений без jar, теж працює: він стане варіантом `JAR` з копією в сховищі. Це
  побічна властивість, не канал для гравців.

Розміри (виміряно або виведено з вимірів етапу Understand; CI запише точні в маніфест):

| Файл | Розмір |
|---|---|
| `Prestarter.exe` v0.2.0 | 4 811 776 B |
| `Asterium.exe` у продакшні (престартер + jar) | 13 315 677 B (jar 8 503 901 B) |
| ARM64 exe (X1, без jar) | 4 496 896 B |
| ELF x86-64 (без jar) | 6 125 632 B |
| AppImage x86-64 | 83 368 440 B |
| DMG universal | невідомо, вимірює CI |

## Наслідки

- На Linux два файли з різними варіантами оновлення (`LINUX_*` і `JAR`) більше не конфліктують: сервер віддає один
  файл на варіант, і AppImage цього варіанта не займає. Самооновлення AppImage - це лише jar (~8 МБ), а не 83 МБ.
- Варіанти `MACOS_*` на сервері не збираються зовсім.
- Нова версія AppImage або `.app` (сам престартер) доходить до гравця лише через сайт: лаунчер оновлює jar, а не
  обгортку. Тому обгортка має бути стабільною, а все, що змінюється часто, живе в jar. Для Windows і одного файла
  Linux новий престартер доходить разом з наступним збиранням лаунчера, як сьогодні.
- Престартер у варіанті з копією потребує мережі на першому запуску двічі: JRE і jar.

## Розглянуті варіанти

- Дописувати jar до AppImage і запускати `$APPIMAGE` (A4 працює). Відкинуто: самооновлення перезаписало б 83 МБ
  AppImage в місці, яке може бути лише для читання, а варіант `LINUX_*` збігся б з одним файлом.
- Jar усередині `.app` (`Contents/Resources`) або дописаний до Mach-O. Відкинуто: ламає підпис і нотаризацію,
  а з App Translocation самооновлення не може писати в бандл.
- Сервер перепідписує `.zip` для macOS при кожному збиранні (rcodesign ad hoc). Відкинуто: з Developer ID це
  означало б нотаризацію на кожне збирання LaunchServer.

## Перевірка

- Модульні тести: визначення вбудованого jar (PE, ELF, Mach-O без jar, файл з Authenticode-хвостом і полем коментаря
  EOCD, обрізаний zip).
- CI smoke на кожній платформі: вбудований jar (тестовий `Hello.jar` дописано так само, як це робить
  `PrestarterTask`) і копія jar (локальний сервер на loopback, [0008](0008-loopback-test-endpoints.md)): маркер від
  `Hello.jar`, шлях джерела коду, аргументи.
- e2e LaunchServer: кожен `Asterium_*` побайтово дорівнює `<сирий престартер> ∥ Asterium.jar`, а `LauncherSignCheck`
  для кожного варіанта отримує правильну URL.
