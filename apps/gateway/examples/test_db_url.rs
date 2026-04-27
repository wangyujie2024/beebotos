use sqlx::sqlite::SqlitePoolOptions;

#[tokio::main]
async fn main() {
    let urls = vec![
        "sqlite:data/test1.db",
        "sqlite:data/test2.db",
        "sqlite:data/beebotos.db",
    ];
    for url in urls {
        println!("Testing: {}", url);
        match SqlitePoolOptions::new()
            .max_connections(1)
            .connect(url)
            .await
        {
            Ok(_) => println!("  SUCCESS"),
            Err(e) => println!("  FAILED: {}", e),
        }
    }
}
