# Oracle 12c schema (drop folder)

Paste the Oracle Forms 12c Forms XML Schema into `forms.xsd`, replacing the whole placeholder file. Only the Forms schema is expected; there is no 12c Reports placeholder (the 14c Reports validation uses `reports.dtd`, kept in the skills' `assets/`).

The placeholder is a valid but empty schema; it validates nothing. This subfolder (`assets/12c/`) is deliberately not directly in `assets/`, so the validator does not treat the placeholder as a real 12c schema.

After pasting, the agent will:
1. copy `forms.xsd` to `plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/assets/forms-12c.xsd` and `.../converting-oracle-artifacts/assets/forms-12c.xsd`, so `validate_forms_xml.py --version 12c` and `--version auto` use it;
2. compare it with `assets/forms-14c.xsd` (and the schema `extract_forms_schema.py` extracts from the 14c install) and record the differences in `plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/references/forms-xml-schema.md`;
3. update the docs and the change log.

Status: nothing pasted yet.
