import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Map;
import java.util.TreeMap;

/**
 * Smoke fixture (ADR 0009): what a launcher jar sees when the prestarter starts it with {@code java -jar}. Prints and
 * writes {@code $ASTERIUM_SMOKE_DIR/hello-<n>.json}: code source, arguments, os.arch, java.home, java.version and the
 * whole environment. Built with {@code javac --release 17} (Java SE only).
 */
public final class Hello {
    private Hello() {}

    private static String json(String value) {
        StringBuilder out = new StringBuilder("\"");
        for (char c : value.toCharArray()) {
            switch (c) {
                case '"' -> out.append("\\\"");
                case '\\' -> out.append("\\\\");
                case '\n' -> out.append("\\n");
                case '\r' -> out.append("\\r");
                case '\t' -> out.append("\\t");
                default -> {
                    if (c < 0x20) {
                        out.append(String.format("\\u%04x", (int) c));
                    } else {
                        out.append(c);
                    }
                }
            }
        }
        return out.append('"').toString();
    }

    public static void main(String[] args) throws IOException {
        String codeSource = Hello.class.getProtectionDomain().getCodeSource().getLocation().toString();
        StringBuilder body = new StringBuilder("{\n");
        body.append("  \"probe\": \"hello\",\n");
        body.append("  \"codeSource\": ").append(json(codeSource)).append(",\n");
        body.append("  \"osArch\": ").append(json(System.getProperty("os.arch"))).append(",\n");
        body.append("  \"osName\": ").append(json(System.getProperty("os.name"))).append(",\n");
        body.append("  \"javaHome\": ").append(json(System.getProperty("java.home"))).append(",\n");
        body.append("  \"javaVersion\": ").append(json(System.getProperty("java.version"))).append(",\n");
        body.append("  \"cwd\": ").append(json(System.getProperty("user.dir"))).append(",\n");
        body.append("  \"args\": [");
        for (int i = 0; i < args.length; i++) {
            body.append(i == 0 ? "" : ", ").append(json(args[i]));
        }
        body.append("],\n  \"env\": {");
        Map<String, String> env = new TreeMap<>(System.getenv());
        boolean first = true;
        for (Map.Entry<String, String> entry : env.entrySet()) {
            body.append(first ? "\n    " : ",\n    ").append(json(entry.getKey())).append(": ").append(json(entry.getValue()));
            first = false;
        }
        body.append("\n  }\n}\n");
        System.out.print(body);
        String dir = System.getenv("ASTERIUM_SMOKE_DIR");
        if (dir != null && !dir.isEmpty()) {
            Path out = Path.of(dir);
            Files.createDirectories(out);
            int n = 0;
            while (Files.exists(out.resolve("hello-" + n + ".json"))) {
                n++;
            }
            Files.writeString(out.resolve("hello-" + n + ".json"), body, StandardCharsets.UTF_8);
        }
    }
}
