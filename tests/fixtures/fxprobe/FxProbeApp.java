import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

import javafx.animation.PauseTransition;
import javafx.application.Application;
import javafx.application.ConditionalFeature;
import javafx.application.Platform;
import javafx.concurrent.Worker;
import javafx.embed.swing.SwingFXUtils;
import javafx.scene.Scene;
import javafx.scene.image.WritableImage;
import javafx.scene.web.WebView;
import javafx.stage.Stage;
import javafx.util.Duration;

import javax.imageio.ImageIO;

/**
 * The launcher half of the FX probe (ADR 0009): what the real Gravit launcher JVM needs on every target. Opens a
 * window titled "Asterium" with a WebView (jfxwebkit), waits for the page to load, saves a snapshot of the scene and
 * writes {@code $ASTERIUM_SMOKE_DIR/fx-<n>.json} (os.arch, java.home, the JavaFX pipeline, the WebKit user agent,
 * the environment), keeps the window open for {@code ASTERIUM_SMOKE_HOLD_MS} (default 5000) and exits.
 * {@code --fail-child} makes it die at once (exit 3) to imitate a launcher crash; with Gravit's waitProcess retry
 * it throws instead, so the stack trace reaches the log.
 */
public final class FxProbeApp extends Application {
    private static List<String> arguments = List.of();

    public static void main(String[] args) {
        arguments = Arrays.asList(args);
        if (arguments.contains("--fail-child")) {
            if (Boolean.getBoolean("launcher.waitProcess") || System.getenv("ASTERIUM_SMOKE_THROW") != null) {
                throw new IllegalStateException("FX probe was asked to fail (--fail-child)");
            }
            System.exit(3);
        }
        launch(args);
    }

    @Override
    public void start(Stage stage) {
        WebView web = new WebView();
        Scene scene = new Scene(web, 640, 400);
        stage.setTitle("Asterium");
        stage.setScene(scene);
        stage.show();
        web.getEngine().getLoadWorker().stateProperty().addListener((obs, old, state) -> {
            if (state == Worker.State.SUCCEEDED) {
                PauseTransition settle = new PauseTransition(Duration.millis(500));
                settle.setOnFinished(e -> recordAndClose(stage, scene, web));
                settle.play();
            } else if (state == Worker.State.FAILED) {
                System.err.println("FX probe: the WebView page failed to load");
                Platform.exit();
            }
        });
        web.getEngine().loadContent(
            "<html><body style='background:#0f1424;color:#f3f4fb;font-family:sans-serif'>"
                + "<h1>Asterium FX probe</h1><p id='ua'></p>"
                + "<script>document.getElementById('ua').textContent = navigator.userAgent;</script></body></html>");
    }

    private void recordAndClose(Stage stage, Scene scene, WebView web) {
        String dir = System.getenv("ASTERIUM_SMOKE_DIR");
        try {
            if (dir != null && !dir.isEmpty()) {
                Path out = Path.of(dir);
                Files.createDirectories(out);
                int n = 0;
                while (Files.exists(out.resolve("fx-" + n + ".json"))) {
                    n++;
                }
                WritableImage image = scene.snapshot(null);
                Path png = out.resolve("fx-" + n + ".png");
                ImageIO.write(SwingFXUtils.fromFXImage(image, null), "png", png.toFile());
                Map<String, String> fields = new LinkedHashMap<>();
                fields.put("probe", "fx");
                fields.put("osArch", System.getProperty("os.arch"));
                fields.put("osName", System.getProperty("os.name"));
                fields.put("javaHome", System.getProperty("java.home"));
                fields.put("javaVersion", System.getProperty("java.version"));
                fields.put("javafxVersion", System.getProperty("javafx.runtime.version"));
                fields.put("scene3d", String.valueOf(Platform.isSupported(ConditionalFeature.SCENE3D)));
                fields.put("userAgent", String.valueOf(web.getEngine().getUserAgent()));
                fields.put("title", stage.getTitle());
                fields.put("cwd", new File("").getAbsolutePath());
                fields.put("snapshot", png.toString());
                fields.put("snapshotSize", (int) image.getWidth() + "x" + (int) image.getHeight());
                String body = "{" + Json.string("record") + ":" + Json.object(fields) + ","
                    + Json.string("args") + ":" + Json.array(arguments.toArray(new String[0])) + ","
                    + Json.string("env") + ":" + Json.object(System.getenv()) + "}\n";
                Files.writeString(out.resolve("fx-" + n + ".json"), body, StandardCharsets.UTF_8);
            }
        } catch (IOException e) {
            e.printStackTrace();
        }
        long hold = 5000;
        String holdText = System.getenv("ASTERIUM_SMOKE_HOLD_MS");
        if (holdText != null && !holdText.isEmpty()) {
            hold = Long.parseLong(holdText);
        }
        PauseTransition close = new PauseTransition(Duration.millis(hold));
        close.setOnFinished(e -> Platform.exit());
        close.play();
    }
}
