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

    /// Query the database for a form's data blocks and display live records
    Query {
        /// Path to the module file (.fmb, .xml)
        file: PathBuf,

        /// Specific block to query (defaults to first database block)
        #[arg(short, long)]
        block: Option<String>,

        /// Database backend: 'oracle', 'sqlite', or 'mock'
        #[arg(long, default_value = "oracle")]
        backend: String,

        /// Maximum records to fetch
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
    },

    /// Compare two Oracle Forms modules and output visual and structural diffs
    Diff {
        /// Base module file
        file1: PathBuf,

        /// Modified module file
        file2: PathBuf,

        /// Output full diff as JSON
        #[arg(long)]
        json: bool,
    },

    /// Export an Oracle Forms module to Forms2XML XML format
    ExportXml {
        /// Path to the module file (.fmb, .xml, .pld)
        file: PathBuf,

        /// Output file path (defaults to stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
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

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
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
        Commands::Query { file, block, backend, limit } => {
            let temp_cache = std::env::temp_dir().join("forms-cli-cache");
            let _ = fs::create_dir_all(&temp_cache);
            let index = parse_file(&file, &temp_cache)?;

            if index.blocks.is_empty() {
                eprintln!("Error: Module contains no blocks to query.");
                return Ok(());
            }

            let target_block = match block {
                Some(b_name) => index.blocks.into_iter().find(|b| b.name.eq_ignore_ascii_case(&b_name))
                    .ok_or_else(|| format!("Block '{}' not found in module", b_name))?,
                None => {
                    // Default to first block with items or data source
                    index.blocks.into_iter().find(|b| b.query_data_source_name.is_some() || !b.items.is_empty())
                        .unwrap()
                }
            };

            let backend_type = match backend.to_ascii_lowercase().as_str() {
                "oracle" => oracle_forms_db::DbBackend::Oracle,
                "sqlite" => oracle_forms_db::DbBackend::Sqlite,
                _ => oracle_forms_db::DbBackend::Mock,
            };

            let mut config = oracle_forms_db::DbConfig {
                backend: backend_type,
                ..Default::default()
            };

            // Test connection; if Oracle fails, fallback gracefully to mock with notice
            let driver: Box<dyn oracle_forms_db::DatabaseDriver> = match oracle_forms_db::create_driver(&config) {
                Ok(d) => match d.test_connection().await {
                    Ok(_) => d,
                    Err(e) if backend_type == oracle_forms_db::DbBackend::Oracle => {
                        eprintln!("[Notice] Oracle listener not reachable ({}). Falling back to mock engine.", e);
                        config.backend = oracle_forms_db::DbBackend::Mock;
                        oracle_forms_db::create_driver(&config)?
                    }
                    Err(e) => return Err(e.into()),
                },
                Err(e) => return Err(e.into()),
            };

            println!("============================================================");
            println!("  Executing Query on Block: {} (Backend: {:?})", target_block.name, driver.backend());
            println!("============================================================");

            let res = driver.query_block(&target_block, Some(limit)).await?;
            println!("  Generated SQL:   {}", res.sql_statement);
            println!("  Table Name:      {}", res.table_name);
            println!("  Execution Time:  {} ms", res.execution_time_ms);
            println!("  Records Fetched: {}", res.records.len());
            println!("============================================================");

            if res.records.is_empty() {
                println!("  No records returned.");
            } else {
                for (idx, record) in res.records.iter().enumerate() {
                    println!("\n  [Record #{}]", idx + 1);
                    for (k, v) in &record.values {
                        println!("    {:<20} = {}", k, v);
                    }
                }
            }
        }

        Commands::Diff { file1, file2, json } => {
            let cache_dir = std::env::temp_dir().join("forms_diff_cache");
            let mod1 = parse_file(&file1, &cache_dir)?;
            let mod2 = parse_file(&file2, &cache_dir)?;

            let diff = oracle_forms_core::diff::FormDiffer::diff(&mod1, &mod2);
            if json {
                let serialized = serde_json::to_string_pretty(&diff)
                    .map_err(|e| FormsError::Internal(e.to_string()))?;
                println!("{}", serialized);
            } else {
                print!("{}", diff.to_text_report());
            }
        }

        Commands::ExportXml { file, output } => {
            let cache_dir = std::env::temp_dir().join("forms_export_cache");
            let module = parse_file(&file, &cache_dir)?;
            let xml = oracle_forms_xml::FormsXmlWriter::write_string(&module);

            if let Some(out_path) = output {
                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| FormsError::Internal(format!("Failed to create parent dir: {}", e)))?;
                }
                fs::write(&out_path, &xml)
                    .map_err(|e| FormsError::Internal(format!("Failed to write export XML: {}", e)))?;
                println!("Exported module '{}' to XML: {}", module.key.name, out_path.display());
            } else {
                println!("{}", xml);
            }
        }
    }

    Ok(())
}
