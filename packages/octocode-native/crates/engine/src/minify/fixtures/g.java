// comment
package p;
/** Javadoc */
@SuppressWarnings("unchecked")
public class G {
    String a = "http://x"; // c
    String t = """
        text // keep
          block
        """;
    char c = '"'; // quote
    /* c */ int x = 1;
    @Override public String toString() { return "G"; }
}
