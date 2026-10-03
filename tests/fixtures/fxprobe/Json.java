import java.util.Map;
import java.util.TreeMap;

/** Minimal JSON writing for the probe records (no JavaFX, so the wrapper JVM can load it). */
final class Json {
    private Json() {}

    static String string(String value) {
        if (value == null) {
            return "null";
        }
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

    static String array(String[] values) {
        StringBuilder out = new StringBuilder("[");
        for (int i = 0; i < values.length; i++) {
            out.append(i == 0 ? "" : ",").append(string(values[i]));
        }
        return out.append(']').toString();
    }

    static String object(Map<String, String> values) {
        StringBuilder out = new StringBuilder("{");
        boolean first = true;
        for (Map.Entry<String, String> entry : new TreeMap<>(values).entrySet()) {
            out.append(first ? "" : ",").append(string(entry.getKey())).append(':').append(string(entry.getValue()));
            first = false;
        }
        return out.append('}').toString();
    }
}
