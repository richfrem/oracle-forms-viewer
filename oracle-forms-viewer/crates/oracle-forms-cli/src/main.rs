use std::fs;
use std::path::{Path, PathBuf};
use clap::{Parser, Subcommand};
use oracle_forms_binary::BinaryFmbParser;
use oracle_forms_core::error::{FormsError, Result};
use oracle_forms_core::model::{ModuleIndex, ModuleKey, ModuleType};
use oracle_forms_xml::{FormsXmlParser, PldParser};

#[derive(Parser)]
#[command(name = "oracle-forms-cli")]
#[command(version = "0.1.0")]
#[command(about = "Pure native Rust toolkit for Oracle Forms (.fmb, .mmb, .olb, XML, PLD)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Inspect an Oracle Forms module (.fmb, .xml, .pld) and display summary or JSON
    Inspect {
        /// Path to the module file (.fmb, .xml, .pld, .mmb, .olb)
        file: PathBuf,

        /// Output full ModuleIndex as formatted JSON
        #[arg(long)]
        json: bool,
    },

    /// Extract and dump all PL/SQL blocks (triggers, program units) into a directory
    DumpSql {
        /// Path to the module file
        file: PathBuf,

        /// Output directory to write extracted SQL sidecars
        #[arg(short, long, default_value = "./dumped-plsql")]
        output: PathBuf,
    },
}

fn parse_file(file: &Path, cache_dir: &Path) -> Result<ModuleIndex> {
    if !file.is_file() {
        return Err(FormsError::NotFound(format!("File '{}' not found", file.display())));
    }

    let file_name = file.file_name().and_then(|n| n.to_str()).unwrap_or("module");
    let key = ModuleKey::from_file_name(file_name)
        .unwrap_or_else(|| ModuleKey::new("MODULE", ModuleType::Form));

    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".pld") {
        PldParser::parse(&key, file, cache_dir)
    } else if lower.ends_with(".xml") {
        FormsXmlParser::parse(&key, file, cache_dir)
    } else {
        // Binary form (.fmb, .mmb, .olb)
        // Check for sibling XML if present
        let base_name = &key.name;
        let ext_type = key.module_type.extension();
        let candidates = [
            format!("{}_{}.xml", base_name.to_ascii_lowercase(), ext_type),
            format!("{}_{}.xml", base_name.to_ascii_uppercase(), ext_type.to_ascii_uppercase()),
            format!("{}.xml", base_name.to_ascii_lowercase()),
            format!("{}.xml", base_name.to_ascii_uppercase()),
        ];
        let mut found_xml = None;
        if let Some(parent) = file.parent() {
            for c in &candidates {
                let p = parent.join(c);
                if p.is_file() {
                    found_xml = Some(p);
                    break;
                }
            }
        }
        if let Some(xml_path) = found_xml {
            FormsXmlParser::parse(&key, &xml_path, cache_dir)
        } else {
            BinaryFmbParser::parse(&key, file, cache_dir)
        }
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Inspect { file, json } => {
            let temp_cache = std::env::temp_dir().join("forms-cli-cache");
            let _ = fs::create_dir_all(&temp_cache);
            let index = parse_file(&file, &temp_cache)?;

            if json {
                println!("{}", serde_json::to_string_pretty(&index)?);
            } else {
                let summary = index.to_summary();
                println!("============================================================");
                println!("  Oracle Forms Module Summary: {}", summary.module_name);
                println!("============================================================");
                println!("  Module Type:     {}", summary.module_type);
                println!("  Title:           {}", summary.title.as_deref().unwrap_or("None"));
                println!("  Forms Version:   {}", summary.forms_version.as_deref().unwrap_or("Unknown"));
                println!("  Blocks:          {}", summary.block_count);
                println!("  Items:           {}", summary.item_count);
                println!("  Canvases:        {}", summary.canvas_count);
                println!("  Windows:         {}", summary.window_count);
                println!("  Triggers:        {}", summary.trigger_count);
                println!("  Program Units:   {}", summary.program_unit_count);
                println!("============================================================");

                if !index.blocks.is_empty() {
                    println!("\n  [Blocks]");
                    for b in &index.blocks {
                        println!("    - {} ({} items)", b.name, b.items.len());
                    }
                }

                if !index.triggers.is_empty() {
                    println!("\n  [Triggers (sample)]");
                    for t in index.triggers.iter().take(10) {
                        println!("    - {:<6} {:<30} (lines: {})", format!("{:?}", t.level), t.name, t.line_count);
                    }
                    if index.triggers.len() > 10 {
                        println!("      ... and {} more triggers", index.triggers.len() - 10);
                    }
                }

                if !index.program_units.is_empty() {
                    println!("\n  [Program Units]");
                    for pu in &index.program_units {
                        println!("    - {:<12} {:<30} (lines: {})", format!("{:?}", pu.unit_type), pu.name, pu.line_count);
                    }
                }
            }
        }
        Commands::DumpSql { file, output } => {
            fs::create_dir_all(&output)?;
            let index = parse_file(&file, &output)?;
            println!("Extracted PL/SQL from {} into {}", file.display(), output.display());
            println!("  Triggers:      {}", index.triggers.len());
            println!("  Program Units: {}", index.program_units.len());
        }
    }

    Ok(())
}
