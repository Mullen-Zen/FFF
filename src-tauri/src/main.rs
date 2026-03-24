mod db;

fn main() {
    match db::init_db() {
        Ok(_) => println!("App setup complete."),
        Err(e) => eprintln!("Database initialization failed: {}", e),
    }
}