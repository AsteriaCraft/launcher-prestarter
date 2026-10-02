# ADR 0003. macOS: universal `.app` у DMG, без підпису Apple зараз, Developer ID і нотаризація за секретами

- Статус: запропоновано (етап Design, 2026-10-02); переглянуто після рецензії дизайну 2026-10-02 (див. «Ревізія»)
- Рішення власника: macOS universal (Intel + Apple Silicon); поки без підпису, з чіткою інструкцією для гравців
  (System Settings → Privacy & Security → Open Anyway); підпис Developer ID і нотаризація підготовлені в CI за
  секретами `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD` і
  вмикаються, щойно секрети з'являються; до того macOS-job каже про це і все одно випускає збірку без підпису.
- Пов'язані: [0001](0001-artifact-matrix-and-jar-delivery.md), [0009](0009-ci-matrix-release-assets-and-manifest.md)

## Контекст

- Jar не може жити в бандлі ([0001](0001-artifact-matrix-and-jar-delivery.md)): `.app` статичний, збирається в CI,
  а jar лаунчера престартер завантажує в `~/Library/Application Support/Asterium/Prestarter/launcher/Asterium.jar`.
- JRE Liberica для macOS має `LC_BUILD_VERSION minos=11.0` (виміряно, L5); Tauri за замовчуванням дозволяє 10.13.
- `transparent: true` у `tauri.conf.json` на macOS потребує `app.macOSPrivateApi` (документація Tauri).
- Stapling квитка нотаризації працює для `.app` і `.dmg`, не для `.zip` (документація Tauri, Sign/macOS).
- Документація Tauri: позиції іконок і тло DMG не застосовуються, коли DMG збирається на CI (там пропускається
  AppleScript Finder).
- Apple Silicon не запускає arm64-код без підпису: потрібен щонайменше ad hoc (`signingIdentity: "-"`).
- macOS 15 прибрав обхід «Control-клік → Відкрити»: застосунок, який Gatekeeper не перевірив, відкривається лише
  через System Settings → Privacy & Security → «Open Anyway».

## Рішення

1. **Формат доставки - DMG**: `Asterium-macos-universal.dmg` з `Asterium.app` і посиланням на `/Applications`.
   Причини: звичний для macOS сценарій «перетягни в Програми» (після переміщення Finder'ом немає App Translocation),
   можливість staple квитка в майбутньому, вікно DMG з картинкою-інструкцією. ZIP відкинуто: Safari розпаковує його в
   «Завантаження», застосунок лишається там (транслокація при кожному запуску), staple неможливий.
2. **DMG збирає `scripts/ci/make-dmg.sh` через `dmgbuild`** (Python, версія і хеш закріплені в
   `scripts/ci/requirements-dmg.txt`, встановлюється з `--require-hashes`). `dmgbuild` пише `.DS_Store` без Finder, тож
   тло (стрілка в «Програми» і три кроки першого запуску українською та англійською) і позиції іконок працюють і на
   CI. `.app` збирає Tauri: `tauri build --target universal-apple-darwin --bundles app`.
3. **`tauri.conf.json`:** `productName: "Asterium"`, `identifier: "pro.asterium.prestarter"` (домен, яким володіє
   проєкт; сьогоднішній `com.asterium.prestarter` не наш домен), `bundle.macOS.minimumSystemVersion: "11.0"`,
   `bundle.category: "Game"`, `app.macOSPrivateApi: true` (не App Store, тож приватне API для прозорого вікна
   допустиме), `bundle.macOS.signingIdentity: "-"` за замовчуванням (ad hoc).
4. **Без секретів (зараз):** `.app` підписано ad hoc (`codesign -s -`), DMG не підписано. macOS-job пише
   `::notice::macOS build is NOT signed with a Developer ID: APPLE_CERTIFICATE is not set` і публікує збірку.
   Гравець бачить «“Asterium” Not Opened» і відкриває його через System Settings → Privacy & Security →
   «Open Anyway» (кнопка з'являється після першої спроби і доступна близько години) → пароль або Touch ID → «Open».
   На macOS 11-14 також працює Control-клік → «Відкрити». Інструкція - на сайті ([0011](0011-site-download-experience.md))
   і на картинці в DMG.
5. **З секретами (пізніше, без змін коду):** окремий job `sign-macos` у `publish.yml` з `environment: release`
   (лише він бачить секрети Apple) запускає `scripts/ci/macos-signing.sh` на `.app`, який зібрав `build.yml` (ad hoc,
   без секретів), і перезбирає DMG:
   - `APPLE_CERTIFICATE` (base64 `.p12`) і `APPLE_CERTIFICATE_PASSWORD` - скрипт імпортує сертифікат у тимчасовий
     keychain, визначає ідентичність `Developer ID Application: … (<TEAM_ID>)` і перевіряє, що команда в ній
     дорівнює `APPLE_TEAM_ID`;
   - `APPLE_APP_PASSWORD` (пароль застосунку Apple ID) разом з `APPLE_ID` і `APPLE_TEAM_ID` іде в
     `xcrun notarytool` (скрипт також експортує його як `APPLE_PASSWORD` - так його називає Tauri, якщо колись
     повернемо підпис у сам `tauri build`);
   - порядок: `codesign --force --options runtime --timestamp` для `.app` → `notarytool submit --wait` →
     `stapler staple` `.app` → `make-dmg.sh` → `codesign` DMG → `notarytool submit --wait` → `stapler staple` DMG →
     `spctl -a -vv` обох;
   - частина секретів без решти - помилка до підпису (а не тихий ad hoc), щоб напівналаштований підпис не
     опублікував непідписаний реліз непомітно; без жодного - `::notice::` і реліз з ad hoc;
   - секрети Apple живуть лише в GitHub Environment `release` (гілка `release`), не в репозиторії: PR і гілки їх не
     бачать. Окремий job потрібен тому, що job, який викликає повторно використовуваний workflow, не може мати
     `environment`, а секрети середовища не передаються через `workflow_call` (рецензія,
     [0009](0009-ci-matrix-release-assets-and-manifest.md)); smoke в `publish.yml` іде вже на підписаному DMG.
6. **Шлях підпису перевіряється без Apple.** CI на гілках генерує в тимчасовому keychain самопідписаний сертифікат
   для підпису коду і проганяє `macos-signing.sh` у режимі `--self-test`: імпорт, `codesign --verify --strict --deep`,
   підпис DMG. Нотаризацію без облікового запису Apple перевірити неможливо; її крок має окремий `--dry-run`, що
   друкує команди без секретів.
7. **Карантин.** Престартер після розпакування JRE і після завантаження jar знімає `com.apple.quarantine` з власних
   файлів, якщо атрибут є (чи ставить його система файлам, які створює застосунок з карантину, - перевіряє CI на
   macOS-раннері, `xattr -l`). Liberica JRE підписана `Developer ID Application: BELLSOFT (8LBATW8FZA)` з hardened
   runtime (L5).
8. **Транслокація не заважає**: престартер нічого не пише в бандл. Якщо він запущений з `/Volumes/…` або з
   `…/AppTranslocation/…`, вікно показує ненав'язливу підказку «Перетягніть Asterium у Програми» (один раз).
9. **Dock.** `-Xdock` обгортці Gravit нічого не дає (вона не створює вікна і виходить за 3 с). Престартер дописує
   `-Xdock:name=Asterium` і `-Xdock:icon=…` у `JDK_JAVA_OPTIONS` дочірнього середовища, яке успадковують і JVM
   лаунчера, і її перезапуск ([0006](0006-launching-the-launcher.md)); CI перевіряє назву процесу з вікном через
   System Events. Якщо це не спрацює, Dock для вікна лаунчера лишається завданням рантайму (відкрите питання 9).

## Наслідки

- Гравці на macOS проходять один зайвий крок при першому запуску, доки немає Developer ID ($99/рік Apple Developer
  Program). Підпис вмикається додаванням секретів, без коду і без нового дизайну.
- Підтримується macOS 11 Big Sur і новіші, обидві архітектури, з одного файла.
- Наступні версії обгортки доходять лише через сайт (лаунчер оновлює jar), тож макет DMG і текст інструкції мають
  бути правильними з першого релізу.

## Розглянуті варіанти

- ZIP з `.app` - див. пункт 1.
- Окремі збірки для Intel і Apple Silicon. Відкинуто рішенням власника (universal) і тим, що сайт не відрізняє
  Apple Silicon від Intel: Safari і Chrome на Apple Silicon повідомляють «Intel Mac OS X».
- DMG від Tauri (`--bundles dmg`) без тла. Працює, але на CI без інструкції у вікні; `dmgbuild` дає ту саму збірку з
  інструкцією.

## Перевірка (macOS-раннер, кожен PR)

- `lipo -info` (дві архітектури), `codesign -dv --verbose=4`, `codesign --verify --strict --deep -vv`.
- Змонтувати DMG, скопіювати `.app` у тимчасові «Програми», поставити карантин
  (`xattr -w com.apple.quarantine "0081;$(printf %x $(date +%s));Safari;" Asterium.app`), записати вердикт
  `spctl -a -vv -t exec` (для ad hoc очікується відмова - це те, що побачить гравець).
- Запуск `Contents/MacOS/Asterium` напряму (нативний зріз і `arch -x86_64` під Rosetta) з loopback-адресою jar
  ([0008](0008-loopback-test-endpoints.md)): справжнє завантаження JRE, маркер `Hello.jar`, `xattr -l` на JRE і jar.
- Експеримент з Understand §8 один раз, як доказ для [0001](0001-artifact-matrix-and-jar-delivery.md): дописати jar до
  Mach-O і в `Contents/Resources` після підпису, зафіксувати вихід `codesign --verify`.
- Знімок вікна престартера (`screencapture`) як артефакт CI.

## Ревізія після рецензії (2026-10-02)

| Знахідка рецензії | Що змінено |
|---|---|
| Секрети середовища не передаються в повторно використовуваний `build.yml` | підпис і нотаризація - окремий job `sign-macos` у `publish.yml` з `environment: release`; `build.yml` завжди ad hoc і без секретів |
| `-Xdock` на обгортці не має ефекту | `JDK_JAVA_OPTIONS` у середовищі дочірнього процесу, перевірка в CI |
| Rosetta на `macos-15` лише припущення | smoke ставить її явно (`softwareupdate --install-rosetta --agree-to-license`) і перевіряє `arch -x86_64 /usr/bin/true` |
| AppImage і DMG не мають каналу оновлення | підписана політика обгортки ([0001](0001-artifact-matrix-and-jar-delivery.md)) |
