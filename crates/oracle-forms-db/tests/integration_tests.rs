use oracle_forms_core::model::{BlockDml, BlockInfo, ItemInfo};
use oracle_forms_db::{create_driver, DatabaseDriver, DbConfig, MockDriver, SqliteDriver};

#[tokio::test]
async fn test_sqlite_in_memory_crud() {
    let driver = SqliteDriver::in_memory().expect("Failed to initialize SQLite driver");
    driver.test_connection().await.expect("Ping failed");

    let res = driver
        .query("SELECT item_id, item_name, amount FROM test_items ORDER BY item_id")
        .await
        .expect("Query failed");

    assert_eq!(res.records.len(), 5);
    let first = &res.records[0];
    assert_eq!(first.values.get("ITEM_NAME").unwrap(), "Widget A");
    assert_eq!(first.values.get("ITEM_ID").unwrap(), 1);
}

#[tokio::test]
async fn test_mock_driver_block_query() {
    let mock = MockDriver::new().expect("Failed to initialize MockDriver");
    let block = BlockInfo {
        name: "BLOCK5".to_string(),
        query_data_source_name: Some("test_items".to_string()),
        property_class: None,
        items: vec![
            ItemInfo {
                name: "TEXT_ITEM4".to_string(),
                item_type: Some("Text Item".to_string()),
                column_name: Some("ITEM_NAME".to_string()),
                data_type: Some("Char".to_string()),
                ..Default::default()
            },
            ItemInfo {
                name: "TEXT_ITEM284".to_string(),
                item_type: Some("Text Item".to_string()),
                column_name: Some("AMOUNT".to_string()),
                data_type: Some("Number".to_string()),
                ..Default::default()
            },
        ],
        trigger_names: vec![],
        inherited: None,
        source_ref: None,
        dml: Some(BlockDml {
            where_clause: None,
            order_by_clause: Some("item_id ASC".to_string()),
            ..Default::default()
        }),
        data_source_column_count: 2,
        relations: vec![],
        records_displayed: 1,
        distance_between_records: 0,
        record_orientation: "Vertical".to_string(),
    };

    let result = mock.query_block(&block, Some(3)).await.expect("Block query failed");
    assert_eq!(result.records.len(), 3);

    // Verify first row has values for both column name and form item name
    let row0 = &result.records[0];
    assert_eq!(row0.values.get("TEXT_ITEM4").unwrap(), "Widget A");
    assert_eq!(row0.values.get("TEXT_ITEM284").unwrap(), 10.5);

    let row1 = &result.records[1];
    assert_eq!(row1.values.get("TEXT_ITEM4").unwrap(), "Gadget B");
    assert_eq!(row1.values.get("TEXT_ITEM284").unwrap(), 99.5);
}

#[tokio::test]
async fn test_factory_creation() {
    let config = DbConfig::mock();
    let driver = create_driver(&config).expect("Factory should create mock driver");
    driver.test_connection().await.expect("Ping should succeed");
}

#[tokio::test]
async fn test_oracle_live_container_query() {
    let config = DbConfig::oracle_default();
    let driver = create_driver(&config).expect("Failed to create Oracle driver");

    // Only run if listener is reachable
    if driver.test_connection().await.is_err() {
        eprintln!("Oracle 23ai listener on 1521 not reachable; skipping live test");
        return;
    }

    let result = driver
        .query("SELECT item_id, item_name, amount FROM test_items ORDER BY item_id")
        .await
        .expect("Oracle 23ai query failed");

    assert!(!result.records.is_empty());
    println!("Fetched {} live rows from Oracle 23ai!", result.records.len());
}
