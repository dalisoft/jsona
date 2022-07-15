fn main() {
    let args: Vec<String> = std::env::args().collect();
    let jsona_file = args
        .get(1)
        .expect("Usage: print_syntax <jsona-file> [compact]");
    let jsona_file_path = std::path::Path::new(&jsona_file);
    let jsona_content = std::fs::read_to_string(jsona_file_path).unwrap();

    let parse_result = jsona::parser::parse(&jsona_content);

    for err in &parse_result.errors {
        eprintln!("{}", err);
    }
    let node = parse_result.into_dom();
    if let Err(errs) = node.validate() {
        for err in errs {
            eprintln!("{}", err);
        }
    }
    let value = jsona::value::Value::from(&node);
    println!("{}", value.to_jsona());
}