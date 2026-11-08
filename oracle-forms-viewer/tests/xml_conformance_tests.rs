use std::fs;
use std::path::{Path, PathBuf};
use oracle_forms_core::model::{ModuleKey, ModuleType};
use oracle_forms_xml::{FormsXmlParser, PldParser};

fn resolve_sample(name: &str) -> PathBuf {
    let candidates = [
        PathBuf::from("example-fmb/samples").join(name),
        PathBuf::from("../example-fmb/samples").join(name),
        PathBuf::from("../../example-fmb/samples").join(name),
        PathBuf::from("example-fmb/internal-testing").join(name),
        PathBuf::from("../example-fmb/internal-testing").join(name),
        PathBuf::from("../../example-fmb/internal-testing").join(name),
    ];
    for c in &candidates {
        if c.is_file() {
            return c.clone();
        }
    }
    panic!("Sample file {} not found in candidate paths", name);
}

#[test]
fn test_orders_geometry_and_triggers() {
    let sample = resolve_sample("orders_fmb.xml");
    let temp_cache = std::env::temp_dir().join("test_orders_cache");
    let _ = fs::create_dir_all(&temp_cache);

    let key = ModuleKey::new("ORDERS", ModuleType::Form);
    let index = FormsXmlParser::parse(&key, &sample, &temp_cache).expect("Should parse orders_fmb.xml");

    // 1. Verify Canvas dimensions
    assert!(!index.canvases.is_empty(), "Canvases should not be empty");
    let cv_main = index.canvases.iter().find(|c| c.name == "CV_MAIN").expect("CV_MAIN canvas should exist");
    assert!(cv_main.width > 0, "Canvas width should be > 0");
    assert!(cv_main.height > 0, "Canvas height should be > 0");

    // 2. Verify Items have parsed spatial and prompt values
    let orders_block = index.blocks.iter().find(|b| b.name == "ORDERS").expect("ORDERS block should exist");
    let order_id_item = orders_block.items.iter().find(|i| i.name == "ORDER_ID").expect("ORDER_ID item should exist");
    assert_eq!(order_id_item.width, Some(60));
    assert_eq!(order_id_item.height, Some(17));
    assert_eq!(order_id_item.prompt.as_deref(), Some("Order #"));
    assert_eq!(order_id_item.canvas_name.as_deref(), Some("CV_MAIN"));

    let order_lines_block = index.blocks.iter().find(|b| b.name == "ORDER_LINES").expect("ORDER_LINES block should exist");
    let product_item = order_lines_block.items.iter().find(|i| i.name == "PRODUCT").expect("PRODUCT item should exist");
    assert_eq!(product_item.width, Some(120));
    assert_eq!(product_item.height, Some(17));
    assert_eq!(product_item.prompt.as_deref(), Some("Product"));

    // 3. Verify Graphics element was extracted
    assert!(!index.graphics.is_empty(), "Graphics should not be empty");
    let frame1 = index.graphics.iter().find(|g| g.name == "FRAME1").expect("FRAME1 graphic should exist");
    assert_eq!(frame1.canvas_name.as_deref(), Some("CV_MAIN"));
    assert_eq!(frame1.graphics_type, "Frame");

    // 4. Verify Triggers are unescaped without XML entity artifacts like &#10;
    let trig = index.triggers.iter().find(|t| t.name == "WHEN-VALIDATE-ITEM" && t.item_name.as_deref() == Some("ORDER_ID"))
        .expect("WHEN-VALIDATE-ITEM trigger on ORDER_ID should exist");
    let text_ref = trig.text_ref.as_ref().expect("Trigger textRef should exist");

    let sidecar_file = temp_cache.join(&text_ref.file);
    assert!(sidecar_file.is_file(), "Sidecar file {} should exist", sidecar_file.display());
    let code = fs::read_to_string(&sidecar_file).expect("Sidecar file should be readable");
    assert!(!code.contains("&#10;"), "Extracted PL/SQL should not contain &#10;");
    assert!(code.contains("RAISE FORM_TRIGGER_FAILURE;"), "Code should contain PL/SQL statements");
    assert!(code.contains('\n'), "Code should have actual newline characters");
}

#[test]
fn test_claims_geometry_and_triggers() {
    let sample = resolve_sample("claims_fmb.xml");
    let temp_cache = std::env::temp_dir().join("test_claims_cache");
    let _ = fs::create_dir_all(&temp_cache);

    let key = ModuleKey::new("CLAIMS", ModuleType::Form);
    let index = FormsXmlParser::parse(&key, &sample, &temp_cache).expect("Should parse claims_fmb.xml");

    // 1. Verify Canvas dimensions
    let claim_canvas = index.canvases.iter().find(|c| c.name == "CV_CLAIM").expect("CV_CLAIM canvas should exist");
    assert!(claim_canvas.width > 0);
    assert!(claim_canvas.height > 0);

    // 2. Verify Items
    let claim_block = index.blocks.iter().find(|b| b.name == "CLAIM").expect("CLAIM block should exist");
    let claim_id_item = claim_block.items.iter().find(|i| i.name == "CLAIM_ID").expect("CLAIM_ID item should exist");
    assert_eq!(claim_id_item.prompt.as_deref(), Some("Claim"));
    assert_eq!(claim_id_item.width, Some(60));
    assert_eq!(claim_id_item.canvas_name.as_deref(), Some("CV_CLAIM"));

    // 3. Verify Item Trigger
    let item_trigger = index.triggers.iter().find(|t| t.name == "WHEN-VALIDATE-ITEM" && t.item_name.as_deref() == Some("REFERENCE"))
        .expect("WHEN-VALIDATE-ITEM on REFERENCE should exist");
    let sidecar_file = temp_cache.join(&item_trigger.text_ref.as_ref().unwrap().file);
    let code = fs::read_to_string(&sidecar_file).expect("Sidecar file should be readable");
    assert!(!code.contains("&#10;"), "Code should not contain &#10;");
    assert!(code.contains("check_reference(:CLAIM.REFERENCE);"), "Code should contain check_reference");
}

#[test]
fn test_pld_parsing() {
    let sample = resolve_sample("utils.pld");
    let temp_cache = std::env::temp_dir().join("test_pld_cache");
    let _ = fs::create_dir_all(&temp_cache);

    let key = ModuleKey::new("UTILS", ModuleType::Library);
    let index = PldParser::parse(&key, &sample, &temp_cache).expect("Should parse utils.pld");

    assert!(!index.program_units.is_empty(), "PLD program units should not be empty");
    assert_eq!(index.key.name, "UTILS");
}
