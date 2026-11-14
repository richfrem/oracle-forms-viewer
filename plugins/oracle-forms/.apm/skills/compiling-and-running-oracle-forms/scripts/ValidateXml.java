// ValidateXml.java: validates XML files with the JDK's built-in validators (no extra libraries).
//
// Used by validate_forms_xml.py when neither xmllint nor lxml is available. It runs inside the Forms 14c
// container (Oracle JDK 17) as a Java single-file program: `java ValidateXml.java <mode> <schema> <file>...`
//
//   mode xsd : validate each file against the XML Schema <schema> (Forms modules, menus, object libraries)
//   mode dtd : validate each file against the DTD <schema> (Reports XML; the DOCTYPE's "reports.dtd" is
//              resolved to <schema>)
//
// Output, one line per finding, tab separated:  <file> OK   |   <file> ERROR <line> <message>
// Exit code 0 when every file is valid, 1 when any file is invalid, 2 on a usage or setup problem.

import java.io.File;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import javax.xml.XMLConstants;
import javax.xml.parsers.SAXParserFactory;
import javax.xml.transform.stream.StreamSource;
import javax.xml.validation.Schema;
import javax.xml.validation.SchemaFactory;
import javax.xml.validation.Validator;
import org.xml.sax.ErrorHandler;
import org.xml.sax.InputSource;
import org.xml.sax.SAXException;
import org.xml.sax.SAXParseException;
import org.xml.sax.XMLReader;

public class ValidateXml {
    static final int MAX_ERRORS_PER_FILE = 50;

    static class Collector implements ErrorHandler {
        final String file;
        int errors = 0;

        Collector(String file) {
            this.file = file;
        }

        void report(SAXParseException e) {
            errors++;
            if (errors <= MAX_ERRORS_PER_FILE) {
                String message = e.getMessage() == null ? "" : e.getMessage().replace('\t', ' ').replace('\n', ' ');
                System.out.println(file + "\tERROR\t" + e.getLineNumber() + "\t" + message);
            }
        }

        public void warning(SAXParseException e) {
        }

        public void error(SAXParseException e) {
            report(e);
        }

        public void fatalError(SAXParseException e) {
            report(e);
        }
    }

    public static void main(String[] args) throws Exception {
        if (args.length < 3 || !(args[0].equals("xsd") || args[0].equals("dtd"))) {
            System.err.println("usage: java ValidateXml.java <xsd|dtd> <schema-file> <xml-file>...");
            System.exit(2);
        }
        String mode = args[0];
        File schemaFile = new File(args[1]);
        if (!schemaFile.isFile()) {
            System.err.println("schema not found: " + schemaFile);
            System.exit(2);
        }
        Schema schema = null;
        if (mode.equals("xsd")) {
            schema = SchemaFactory.newInstance(XMLConstants.W3C_XML_SCHEMA_NS_URI).newSchema(schemaFile);
        }
        List<String> invalid = new ArrayList<>();
        for (int i = 2; i < args.length; i++) {
            String file = args[i];
            Collector collector = new Collector(file);
            try {
                if (mode.equals("xsd")) {
                    Validator validator = schema.newValidator();
                    validator.setErrorHandler(collector);
                    validator.validate(new StreamSource(new File(file)));
                } else {
                    SAXParserFactory factory = SAXParserFactory.newInstance();
                    factory.setValidating(true);
                    factory.setNamespaceAware(true);
                    XMLReader reader = factory.newSAXParser().getXMLReader();
                    reader.setErrorHandler(collector);
                    reader.setEntityResolver((publicId, systemId) -> {
                        if (systemId != null && systemId.endsWith("reports.dtd")) {
                            return new InputSource(schemaFile.toURI().toString());
                        }
                        return null;
                    });
                    reader.parse(new InputSource(new File(file).toURI().toString()));
                }
            } catch (SAXException | IOException e) {
                if (collector.errors == 0) {
                    collector.errors++;
                    String message = e.getMessage() == null ? e.toString() : e.getMessage().replace('\n', ' ');
                    System.out.println(file + "\tERROR\t0\t" + message);
                }
            }
            if (collector.errors == 0) {
                System.out.println(file + "\tOK");
            } else {
                invalid.add(file);
            }
        }
        System.exit(invalid.isEmpty() ? 0 : 1);
    }
}
