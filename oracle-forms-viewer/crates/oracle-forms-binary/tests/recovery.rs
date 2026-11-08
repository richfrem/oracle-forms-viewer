use std::fs;
use std::path::PathBuf;
use oracle_forms_binary::BinaryFmbParser;
use oracle_forms_core::model::{ModuleKey, ModuleType};

fn resolve_fmb(name: &str) -> PathBuf {
    let prefixes = ["", ".", "..", "../..", "../../..", "../../../.."];
    let subdirs = ["temp/example-fmb/internal-testing", "example-fmb/internal-testing", "example-fmb/samples", "example-fmb", "samples"];
    for p in &prefixes {
        for s in &subdirs {
            let candidate = if p.is_empty() {
                PathBuf::from(s).join(name)
            } else {
                PathBuf::from(p).join(s).join(name)
            };
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    panic!("FMB file {} not found in candidate paths", name);
}

#[test]
fn test_direct_binary_fmb_parsing() {
    let fmb_file = resolve_fmb("rccm0000.fmb");
    let temp_cache = std::env::temp_dir().join("test_fmb_cache");
    let _ = fs::create_dir_all(&temp_cache);

    let key = ModuleKey::new("RCCM0000", ModuleType::Form);
    let index = BinaryFmbParser::parse(&key, &fmb_file, &temp_cache).expect("Should parse rccm0000.fmb");

    // 1. Verify module identity
    assert_eq!(index.key.name, "RCCM0000");
    assert_eq!(index.key.module_type, ModuleType::Form);

    // 2. Verify program units extraction
    assert!(!index.program_units.is_empty(), "Program units should not be empty");
    assert!(index.program_units.len() >= 10, "Should have extracted at least 10 program units, found {}", index.program_units.len());
    let run_report = index.program_units.iter().find(|pu| pu.name.contains("RUN_RCCR0200_REPORT"))
        .expect("RUN_RCCR0200_REPORT program unit should be found");
    let text_ref = run_report.text_ref.as_ref().expect("Program unit textRef should not be null");
    let pu_file = temp_cache.join(&text_ref.file);
    assert!(pu_file.is_file(), "Program unit sidecar file should exist at {}", pu_file.display());
    let pu_code = fs::read_to_string(&pu_file).expect("Sidecar file should be readable");
    assert!(pu_code.to_ascii_uppercase().contains("PROCEDURE RUN_RCCR0200_REPORT") || pu_code.to_ascii_uppercase().contains("BEGIN"));

    // 3. Verify triggers extraction
    assert!(!index.triggers.is_empty(), "Triggers should not be empty");
    assert!(index.triggers.len() >= 50, "Should extract numerous triggers, found {}", index.triggers.len());
    let btn_trigger = index.triggers.iter().find(|t| t.name.contains("WHEN-BUTTON-PRESSED"))
        .expect("WHEN-BUTTON-PRESSED trigger should be found");
    let trig_ref = btn_trigger.text_ref.as_ref().expect("Trigger textRef should not be null");
    let trig_file = temp_cache.join(&trig_ref.file);
    assert!(trig_file.is_file(), "Trigger sidecar file should exist at {}", trig_file.display());

    // 4. Verify blocks & canvas extraction
    assert!(!index.blocks.is_empty(), "Blocks should not be empty");
    let mainmenu_block = index.blocks.iter().find(|b| b.name.eq_ignore_ascii_case("MAINMENU"))
        .expect("MAINMENU block should be found");
    assert!(!mainmenu_block.items.is_empty(), "MAINMENU block should contain extracted items");

    assert!(!index.canvases.is_empty(), "Canvases should not be empty");
    let preview_canvas = &index.canvases[0];
    assert!(preview_canvas.width > 0 && preview_canvas.height > 0, "Canvas should have valid dimensions");
}

#[test]
fn test_simple_module_binary_parsing() {
    let fmb_file = resolve_fmb("simple_module.fmb");
    let temp_cache = std::env::temp_dir().join("test_simple_fmb_cache");
    let _ = fs::create_dir_all(&temp_cache);

    let key = ModuleKey::new("SIMPLE_MODULE", ModuleType::Form);
    let index = BinaryFmbParser::parse(&key, &fmb_file, &temp_cache).expect("Should parse simple_module.fmb");

    assert_eq!(index.key.name, "SIMPLE_MODULE");
    assert!(!index.canvases.is_empty());
}
