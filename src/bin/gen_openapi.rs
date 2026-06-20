use utoipa::OpenApi;

fn main() {
    match kash_server::openapi::ApiDoc::openapi().to_pretty_json() {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("failed to serialize OpenAPI document: {e}");
            std::process::exit(1);
        }
    }
}
