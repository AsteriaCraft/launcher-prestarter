import java.io.File;
import java.io.IOException;
import java.net.URISyntaxException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/**
 * The wrapper half of the FX probe (ADR 0009): behaves like Gravit's {@code ClientLauncherWrapper}. It starts a
 * second JVM with the JavaFX modules and {@code -cp <its own jar>}, then sleeps 3 s and exits with 0; if that JVM has
 * already died it logs {@code Process exit with error code: N} like Gravit. With {@code -Dlauncher.waitProcess=true}
 * it inherits the child's output and waits for it. No JavaFX class is touched here.
 */
public final class FxProbe {
    private FxProbe() {}

    public static void main(String[] args) throws IOException, InterruptedException, URISyntaxException {
        String self = new File(FxProbe.class.getProtectionDomain().getCodeSource().getLocation().toURI()).getPath();
        String java = Path.of(System.getProperty("java.home"), "bin", isWindows() ? "java.exe" : "java").toString();
        boolean waitProcess = Boolean.getBoolean("launcher.waitProcess");
        writeWrapperRecord(self, args, waitProcess);

        List<String> command = new ArrayList<>();
        command.add(java);
        command.add("--add-modules");
        command.add("javafx.base,javafx.graphics,javafx.controls,javafx.media,javafx.web,javafx.swing");
        command.add("-cp");
        command.add(self);
        command.add("FxProbeApp");
        command.addAll(List.of(args));
        ProcessBuilder builder = new ProcessBuilder(command);
        if (waitProcess) {
            builder.inheritIO();
        }
        Process process = builder.start();
        if (waitProcess) {
            process.waitFor();
            return;
        }
        Thread.sleep(3000);
        if (!process.isAlive()) {
            int code = process.exitValue();
            if (code != 0) {
                System.err.println("[main] ERROR pro.gravit.launcher.start.ClientLauncherWrapper - Process exit with error code: " + code);
            } else {
                System.err.println("[main] INFO pro.gravit.launcher.start.ClientLauncherWrapper - Process exit with code 0");
            }
        }
    }

    private static boolean isWindows() {
        return System.getProperty("os.name").toLowerCase().startsWith("windows");
    }

    private static void writeWrapperRecord(String self, String[] args, boolean waitProcess) throws IOException {
        String dir = System.getenv("ASTERIUM_SMOKE_DIR");
        if (dir == null || dir.isEmpty()) {
            return;
        }
        Path out = Path.of(dir);
        Files.createDirectories(out);
        int n = 0;
        while (Files.exists(out.resolve("wrapper-" + n + ".json"))) {
            n++;
        }
        String body = "{\"probe\":\"wrapper\",\"jar\":" + Json.string(self) + ",\"waitProcess\":" + waitProcess
            + ",\"osArch\":" + Json.string(System.getProperty("os.arch"))
            + ",\"javaHome\":" + Json.string(System.getProperty("java.home"))
            + ",\"args\":" + Json.array(args) + "}\n";
        Files.writeString(out.resolve("wrapper-" + n + ".json"), body, StandardCharsets.UTF_8);
    }
}
